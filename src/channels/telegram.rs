use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use reqwest::Client;
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use teloxide::update_listeners::Polling;
use tokio::sync::{mpsc, Mutex};

use crate::bus::{InboundMessage, OutboundMessage};
use crate::config::TelegramConfig;
use crate::utils::set_network_online;

/// Telegram channel — long polling bot via teloxide.
///
/// Access control: only users in `allow_from` can interact.
/// Empty allow_from = allow everyone (not recommended in production).
pub struct TelegramChannel {
    config: TelegramConfig,
}

impl TelegramChannel {
    pub fn new(config: TelegramConfig) -> Self {
        Self { config }
    }

    /// Start the Telegram bot: listen for messages and route responses.
    pub async fn start(
        &self,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: mpsc::Receiver<OutboundMessage>,
    ) -> Result<()> {
        // Build HTTP client with extended timeout for slow connections
        // Long polling waits up to 60s for updates, so we need at least 90s HTTP timeout
        let http_client = Client::builder()
            .timeout(Duration::from_secs(90))
            .connect_timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create HTTP client: {}", e))?;

        let bot = Bot::with_client(&self.config.token, http_client);

        // Build allow-list as a HashSet for O(1) lookups
        let allow_from: HashSet<String> = self.config.allow_from.iter().cloned().collect();
        let allow_all = allow_from.is_empty();

        // Log token status (masked for security)
        let token_preview = if self.config.token.len() > 10 {
            format!(
                "{}...{}",
                &self.config.token[..6],
                &self.config.token[self.config.token.len() - 4..]
            )
        } else {
            "[too short]".to_string()
        };

        tracing::info!(
            allow_from = ?self.config.allow_from,
            allow_all,
            token_preview = %token_preview,
            "Telegram channel starting"
        );

        // Spawn outbound message dispatcher
        let bot_for_outbound = bot.clone();
        let outbound_rx = Arc::new(Mutex::new(outbound_rx));
        let outbound_handle = tokio::spawn(Self::outbound_loop(bot_for_outbound, outbound_rx));

        // Set up message handler with long polling
        let handler =
            Update::filter_message().endpoint(
                move |bot: Bot, msg: Message, inbound_tx: mpsc::Sender<InboundMessage>| {
                    let allow_from = allow_from.clone();
                    let allow_all = allow_all;
                    async move {
                        Self::handle_message(bot, msg, &inbound_tx, &allow_from, allow_all).await
                    }
                },
            );

        // Configure polling with:
        // - Extended timeout (60s) for slow connections
        // - Drop pending updates on restart
        // - Custom exponential backoff (2s to 120s)
        let bot_for_listener = bot.clone();
        let listener = Polling::builder(bot_for_listener)
            .timeout(Duration::from_secs(60))
            .drop_pending_updates()
            .backoff_strategy(|error_count| {
                // Exponential backoff: 2s, 4s, 8s, 16s, 32s, 64s, 120s (capped)
                let base = 2u64;
                let delay = base.saturating_pow(error_count.min(6)); // Cap at 64s
                Duration::from_secs(delay.min(120)) // Max 2 minutes
            })
            .build();

        let mut dispatcher = Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![inbound_tx])
            .default_handler(|_upd| async {})
            .build();

        // Custom error handler that tracks network state
        let error_handler = Arc::new(|error: teloxide::RequestError| {
            let err_str = error.to_string().to_lowercase();
            if err_str.contains("timeout")
                || err_str.contains("connection")
                || err_str.contains("network")
                || err_str.contains("timedout")
            {
                set_network_online(false);
                tracing::warn!(
                    error = %error,
                    "Telegram network error - will retry with exponential backoff"
                );
            } else {
                tracing::error!(error = %error, "Telegram API error");
            }
            async {}
        });

        // Run both loops concurrently
        tokio::select! {
            _ = dispatcher.dispatch_with_listener(listener, error_handler) => {
                tracing::info!("Telegram dispatcher stopped");
            }
            result = outbound_handle => {
                if let Err(e) = result {
                    tracing::error!(error = %e, "Outbound loop panicked");
                }
            }
        }

        Ok(())
    }

    /// Handle an incoming Telegram message
    async fn handle_message(
        bot: Bot,
        msg: Message,
        inbound_tx: &mpsc::Sender<InboundMessage>,
        allow_from: &HashSet<String>,
        allow_all: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Extract sender info
        let sender_id = msg
            .from
            .as_ref()
            .map(|u| u.id.0.to_string())
            .unwrap_or_default();
        let chat_id = msg.chat.id.0.to_string();

        // Access control
        if !allow_all && !allow_from.contains(&sender_id) {
            tracing::warn!(
                sender_id = %sender_id,
                chat_id = %chat_id,
                "Telegram: unauthorized user, ignoring"
            );
            return Ok(());
        }

        // Extract text content
        let text = match msg.text() {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => {
                // Ignore non-text messages (photos, stickers, etc.) for now
                return Ok(());
            }
        };

        // Handle commands
        if text == "/start" {
            bot.send_message(msg.chat.id, "🧪 Homun is ready! Send me a message.")
                .await?;
            return Ok(());
        }

        if text == "/new" || text == "/reset" {
            // Session reset is handled by the agent loop when it receives this
            // For now, forward it as a regular message
            bot.send_message(msg.chat.id, "Session cleared. Starting fresh.")
                .await?;
            // We still send it to the agent so it can clear the session
        }

        tracing::info!(
            sender = %sender_id,
            chat = %chat_id,
            len = text.len(),
            "Telegram: received message"
        );

        // Send to agent via bus
        let inbound = InboundMessage {
            channel: "telegram".to_string(),
            sender_id,
            chat_id: chat_id.clone(),
            content: text,
            timestamp: chrono::Utc::now(),
        };

        if let Err(e) = inbound_tx.send(inbound).await {
            tracing::error!(error = %e, "Failed to send to inbound bus");
            bot.send_message(
                msg.chat.id,
                "Sorry, I'm having trouble processing messages right now.",
            )
            .await?;
        }

        Ok(())
    }

    /// Outbound loop: receive agent responses and send to Telegram
    async fn outbound_loop(bot: Bot, outbound_rx: Arc<Mutex<mpsc::Receiver<OutboundMessage>>>) {
        let mut rx = outbound_rx.lock().await;

        while let Some(msg) = rx.recv().await {
            if msg.channel != "telegram" {
                continue;
            }

            let chat_id: i64 = match msg.chat_id.parse() {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!(
                        chat_id = %msg.chat_id,
                        error = %e,
                        "Invalid Telegram chat_id"
                    );
                    continue;
                }
            };

            // Split long messages (Telegram limit: 4096 chars)
            let chunks = split_message(&msg.content, 4000);

            for chunk in chunks {
                // Try HTML first (more lenient than MarkdownV2), fallback to plain text
                let html_chunk = markdown_to_html(&chunk);
                if let Err(e) = bot
                    .send_message(ChatId(chat_id), &html_chunk)
                    .parse_mode(ParseMode::Html)
                    .await
                {
                    tracing::debug!(error = %e, "HTML send failed, retrying plain text");
                    if let Err(e2) = bot.send_message(ChatId(chat_id), &chunk).await {
                        tracing::error!(error = %e2, "Failed to send Telegram message");
                    }
                }
            }
        }
    }
}

/// Convert basic Markdown (from LLM output) to Telegram-compatible HTML.
///
/// Telegram HTML supports: <b>, <i>, <code>, <pre>, <a href="">, <s>, <u>
/// This handles the most common LLM markdown patterns.
fn markdown_to_html(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 128);
    let mut in_code_block = false;
    let mut code_block_lang = String::new();

    for line in text.lines() {
        if line.starts_with("```") {
            if in_code_block {
                // Closing code block
                result.push_str("</code></pre>\n");
                in_code_block = false;
                code_block_lang.clear();
            } else {
                // Opening code block
                in_code_block = true;
                code_block_lang = line.trim_start_matches('`').trim().to_string();
                result.push_str("<pre><code>");
            }
            continue;
        }

        if in_code_block {
            // Inside code block — escape HTML entities only
            result.push_str(&escape_html(line));
            result.push('\n');
            continue;
        }

        // Process inline markdown on this line
        let processed = convert_inline_markdown(line);
        result.push_str(&processed);
        result.push('\n');
    }

    // Close unclosed code block
    if in_code_block {
        result.push_str("</code></pre>\n");
    }

    // Remove trailing newline
    if result.ends_with('\n') {
        result.pop();
    }

    result
}

/// Escape HTML special characters
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Convert inline markdown to HTML: **bold**, *italic*, `code`, ~~strikethrough~~
fn convert_inline_markdown(line: &str) -> String {
    let line = escape_html(line);

    // Headers: # → <b>
    if let Some(header_text) = line.strip_prefix("### ") {
        return format!("<b>{header_text}</b>");
    }
    if let Some(header_text) = line.strip_prefix("## ") {
        return format!("<b>{header_text}</b>");
    }
    if let Some(header_text) = line.strip_prefix("# ") {
        return format!("<b>{header_text}</b>");
    }

    // Bullet points: keep as-is with • for cleaner look
    let line = if let Some(rest) = line.strip_prefix("- ") {
        format!("• {rest}")
    } else if let Some(rest) = line.strip_prefix("* ") {
        format!("• {rest}")
    } else {
        line.to_string()
    };

    // Inline code: `code` → <code>code</code>
    let line = replace_paired_marker(&line, '`', "<code>", "</code>");

    // Bold: **text** → <b>text</b>
    let line = replace_paired_double(&line, "**", "<b>", "</b>");

    // Italic: *text* → <i>text</i> (but not inside <b> tags)
    let line = replace_paired_marker(&line, '*', "<i>", "</i>");

    // Strikethrough: ~~text~~ → <s>text</s>
    replace_paired_double(&line, "~~", "<s>", "</s>")
}

/// Replace paired single-char markers: `x` → <tag>x</tag>
fn replace_paired_marker(text: &str, marker: char, open: &str, close: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut is_open = false;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == marker {
            if is_open {
                result.push_str(close);
            } else {
                result.push_str(open);
            }
            is_open = !is_open;
        } else {
            result.push(chars[i]);
        }
        i += 1;
    }

    // If we have an unclosed marker, it wasn't actually markdown
    if is_open {
        // Undo: rebuild without conversion
        return text.to_string();
    }

    result
}

/// Replace paired double-char markers: **x** → <tag>x</tag>
fn replace_paired_double(text: &str, marker: &str, open: &str, close: &str) -> String {
    let parts: Vec<&str> = text.split(marker).collect();
    if parts.len() < 3 || parts.len() % 2 == 0 {
        // Not enough pairs or odd splits — not valid markdown
        return text.to_string();
    }

    let mut result = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i % 2 == 1 {
            result.push_str(open);
            result.push_str(part);
            result.push_str(close);
        } else {
            result.push_str(part);
        }
    }
    result
}

/// Split a message into chunks that fit within Telegram's character limit.
fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }

        // Try to split at a newline before the limit
        let split_at = remaining[..max_len].rfind('\n').unwrap_or(max_len);

        let (chunk, rest) = remaining.split_at(split_at);
        chunks.push(chunk.to_string());

        // Skip the newline if we split at one
        remaining = rest.strip_prefix('\n').unwrap_or(rest);
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_short_message() {
        let chunks = split_message("hello", 100);
        assert_eq!(chunks, vec!["hello"]);
    }

    #[test]
    fn test_split_long_message() {
        let msg = "line1\nline2\nline3\nline4";
        let chunks = split_message(msg, 12);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "line1\nline2");
        assert_eq!(chunks[1], "line3\nline4");
    }

    #[test]
    fn test_split_no_newline() {
        let msg = "a".repeat(100);
        let chunks = split_message(&msg, 30);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(chunk.len() <= 30);
        }
    }

    #[test]
    fn test_markdown_to_html_bold() {
        assert_eq!(
            markdown_to_html("This is **bold** text"),
            "This is <b>bold</b> text"
        );
    }

    #[test]
    fn test_markdown_to_html_code_block() {
        let input = "Hello\n```rust\nfn main() {}\n```\nDone";
        let html = markdown_to_html(input);
        assert!(html.contains("<pre><code>"));
        assert!(html.contains("fn main() {}"));
        assert!(html.contains("</code></pre>"));
    }

    #[test]
    fn test_markdown_to_html_inline_code() {
        assert_eq!(
            markdown_to_html("Use `cargo run` here"),
            "Use <code>cargo run</code> here"
        );
    }

    #[test]
    fn test_markdown_to_html_headers() {
        assert_eq!(markdown_to_html("## Title"), "<b>Title</b>");
        assert_eq!(markdown_to_html("# Big Title"), "<b>Big Title</b>");
    }

    #[test]
    fn test_markdown_to_html_bullets() {
        assert_eq!(markdown_to_html("- item one"), "• item one");
        assert_eq!(markdown_to_html("* item two"), "• item two");
    }

    #[test]
    fn test_markdown_to_html_escapes_html() {
        assert_eq!(markdown_to_html("a < b & c > d"), "a &lt; b &amp; c &gt; d");
    }

    #[test]
    fn test_markdown_to_html_plain_text() {
        assert_eq!(markdown_to_html("just plain text"), "just plain text");
    }
}
