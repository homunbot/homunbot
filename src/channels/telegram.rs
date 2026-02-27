//! Telegram channel using Frankenstein API client
//!
//! This implementation uses the `frankenstein` crate instead of `teloxide`
//! for better reqwest compatibility and simpler architecture.

use std::collections::HashSet;

use anyhow::Result;
use frankenstein::client_reqwest::Bot;
use frankenstein::methods::{GetUpdatesParams, SendMessageParams};
use frankenstein::types::{AllowedUpdate, ChatId};
use frankenstein::updates::UpdateContent;
use frankenstein::{AsyncTelegramApi, ParseMode};
use tokio::sync::mpsc;

use crate::bus::{InboundMessage, OutboundMessage};
use crate::config::TelegramConfig;

/// Telegram channel — long polling bot via Frankenstein.
pub struct TelegramChannel {
    config: TelegramConfig,
}

impl TelegramChannel {
    pub fn new(config: TelegramConfig) -> Self {
        Self { config }
    }

    /// Start the Telegram bot using Frankenstein API
    pub async fn start(
        &self,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: mpsc::Receiver<OutboundMessage>,
    ) -> Result<()> {
        let api = Bot::new(&self.config.token);

        let allow_from: HashSet<String> = self.config.allow_from.iter().cloned().collect();
        let allow_all = allow_from.is_empty();

        tracing::info!(
            allow_from = ?self.config.allow_from,
            allow_all,
            "Telegram channel (Frankenstein) starting"
        );

        // Spawn outbound handler
        let api_for_outbound = api.clone();
        let _outbound_handle = tokio::spawn(Self::outbound_loop(api_for_outbound, outbound_rx));

        // Long polling loop
        let mut offset: u32 = 0;
        loop {
            let params = GetUpdatesParams {
                offset: Some(offset as i64),
                limit: Some(100),
                timeout: Some(60),
                allowed_updates: Some(vec![AllowedUpdate::Message]),
            };

            match api.get_updates(&params).await {
                Ok(response) => {
                    for update in response.result {
                        offset = update.update_id + 1;
                        if let UpdateContent::Message(message) = update.content {
                            Self::handle_message(
                                &api,
                                *message,
                                &inbound_tx,
                                &allow_from,
                                allow_all,
                            )
                            .await;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(error = ?e, "Failed to get updates");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn handle_message(
        api: &Bot,
        msg: frankenstein::types::Message,
        inbound_tx: &mpsc::Sender<InboundMessage>,
        allow_from: &HashSet<String>,
        allow_all: bool,
    ) {
        let sender_id = msg
            .from
            .as_ref()
            .map(|u| u.id.to_string())
            .unwrap_or_default();
        let chat_id = msg.chat.id;

        if !allow_all && !allow_from.contains(&sender_id) {
            tracing::warn!(sender_id = %sender_id, "Unauthorized user");
            return;
        }

        // Extract text content from message
        let text = match msg.text {
            Some(ref t) if !t.is_empty() => t.clone(),
            _ => {
                // Skip non-text messages (photos, stickers, etc.)
                return;
            }
        };

        tracing::info!(sender = %sender_id, chat = %chat_id, len = text.len(), "Received Telegram message");

        // Handle commands
        if text == "/start" {
            let _ = Self::send_text_message(api, chat_id, "🧪 Homun is ready! Send me a message.")
                .await;
            return;
        }

        if text == "/new" || text == "/reset" {
            let _ = Self::send_text_message(api, chat_id, "Session cleared. Starting fresh.").await;
            // Continue to forward to agent for session reset handling
        }

        // Send to agent via bus
        let inbound = InboundMessage {
            channel: "telegram".to_string(),
            sender_id,
            chat_id: chat_id.to_string(),
            content: text,
            timestamp: chrono::Utc::now(),
        };

        if let Err(e) = inbound_tx.send(inbound).await {
            tracing::error!(error = %e, "Failed to send to inbound bus");
            let _ = Self::send_text_message(
                api,
                chat_id,
                "Sorry, I'm having trouble processing messages right now.",
            )
            .await;
        }
    }

    async fn send_text_message(api: &Bot, chat_id: i64, text: &str) -> Result<()> {
        let params = SendMessageParams::builder()
            .chat_id(ChatId::Integer(chat_id))
            .text(text)
            .build();

        api.send_message(&params).await?;
        Ok(())
    }

    async fn outbound_loop(api: Bot, mut outbound_rx: mpsc::Receiver<OutboundMessage>) {
        while let Some(msg) = outbound_rx.recv().await {
            if msg.channel != "telegram" {
                continue;
            }

            let chat_id: i64 = match msg.chat_id.parse() {
                Ok(id) => id,
                Err(_) => continue,
            };

            // Split long messages (Telegram limit: 4096 chars)
            let chunks = split_message(&msg.content, 4000);

            for chunk in chunks {
                // Try HTML format first
                let html_chunk = markdown_to_html(&chunk);
                let params = SendMessageParams::builder()
                    .chat_id(ChatId::Integer(chat_id))
                    .text(&html_chunk)
                    .parse_mode(ParseMode::Html)
                    .build();

                if let Err(_) = api.send_message(&params).await {
                    // Fallback to plain text
                    let params = SendMessageParams::builder()
                        .chat_id(ChatId::Integer(chat_id))
                        .text(&chunk)
                        .build();

                    if let Err(e) = api.send_message(&params).await {
                        tracing::error!(error = ?e, "Failed to send Telegram message");
                    }
                }
            }
        }
    }
}

/// Convert basic Markdown to Telegram-compatible HTML
fn markdown_to_html(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 128);
    let mut in_code_block = false;

    for line in text.lines() {
        if line.starts_with("```") {
            if in_code_block {
                result.push_str("</code></pre>\n");
                in_code_block = false;
            } else {
                in_code_block = true;
                result.push_str("<pre><code>");
            }
            continue;
        }

        if in_code_block {
            result.push_str(&escape_html(line));
            result.push('\n');
            continue;
        }

        let processed = convert_inline_markdown(line);
        result.push_str(&processed);
        result.push('\n');
    }

    if in_code_block {
        result.push_str("</code></pre>\n");
    }

    if result.ends_with('\n') {
        result.pop();
    }

    result
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

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

    // Bullet points
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

    // Italic: *text* → <i>text</i>
    let line = replace_paired_marker(&line, '*', "<i>", "</i>");

    // Strikethrough: ~~text~~ → <s>text</s>
    replace_paired_double(&line, "~~", "<s>", "</s>")
}

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

    if is_open {
        return text.to_string();
    }

    result
}

fn replace_paired_double(text: &str, marker: &str, open: &str, close: &str) -> String {
    let parts: Vec<&str> = text.split(marker).collect();
    if parts.len() < 3 || parts.len() % 2 == 0 {
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

/// Split a message into chunks that fit within Telegram's character limit
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
