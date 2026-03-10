//! Telegram channel using Frankenstein API client
//!
//! This implementation uses the `frankenstein` crate instead of `teloxide`
//! for better reqwest compatibility and simpler architecture.

use std::collections::HashSet;

use anyhow::Result;
use frankenstein::client_reqwest::Bot;
use frankenstein::methods::{
    GetFileParams, GetUpdatesParams, SendChatActionParams, SendMessageParams,
};
use frankenstein::types::{AllowedUpdate, ChatAction, ChatId, ChatType};
use frankenstein::updates::UpdateContent;
use frankenstein::{AsyncTelegramApi, ParseMode};
use tokio::sync::mpsc;

use crate::bus::{InboundMessage, MessageMetadata, OutboundMessage};
use crate::config::TelegramConfig;

/// Context passed to message handler (avoids too many function arguments).
struct BotContext {
    allow_from: HashSet<String>,
    allow_all: bool,
    mention_required: bool,
    bot_id: u64,
    bot_username: String,
    /// Bot token — needed to build file download URLs.
    token: String,
}

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
        let mention_required = self.config.mention_required;

        // Fetch bot identity for mention detection
        let me = api.get_me().await?;
        let bot_id = me.result.id;
        let bot_username = me.result.username.unwrap_or_default().to_lowercase();

        tracing::info!(
            allow_from = ?self.config.allow_from,
            allow_all,
            mention_required,
            bot_username = %bot_username,
            "Telegram channel (Frankenstein) starting"
        );

        let ctx = BotContext {
            allow_from,
            allow_all,
            mention_required,
            bot_id,
            bot_username,
            token: self.config.token.clone(),
        };

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
                            Self::handle_message(&api, *message, &inbound_tx, &ctx).await;
                        }
                    }
                }
                Err(e) => {
                    let err_str = format!("{e:?}");
                    if err_str.contains("TimedOut") || err_str.contains("timed out") {
                        tracing::debug!(error = %e, "Telegram poll timeout (normal)");
                        // Timeouts during long polling are expected — retry immediately
                    } else {
                        tracing::warn!(error = %e, "Telegram poll error, backing off");
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
            }
        }
    }

    async fn handle_message(
        api: &Bot,
        msg: frankenstein::types::Message,
        inbound_tx: &mpsc::Sender<InboundMessage>,
        ctx: &BotContext,
    ) {
        let sender_id = msg
            .from
            .as_ref()
            .map(|u| u.id.to_string())
            .unwrap_or_default();
        let chat_id = msg.chat.id;

        if !ctx.allow_all && !ctx.allow_from.contains(&sender_id) {
            tracing::warn!(sender_id = %sender_id, "Unauthorized user");
            return;
        }

        // Extract text content + optional file attachment
        let mut attachment_path: Option<String> = None;

        // Check for document attachment first
        if let Some(ref doc) = msg.document {
            match Self::download_document(api, doc, &ctx.token).await {
                Ok(path) => {
                    tracing::info!(
                        file_name = doc.file_name.as_deref().unwrap_or("unknown"),
                        path = %path.display(),
                        "Downloaded Telegram document"
                    );
                    attachment_path = Some(path.to_string_lossy().to_string());
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to download Telegram document");
                }
            }
        }

        let mut text = match msg.text.as_deref().or(msg.caption.as_deref()) {
            Some(t) if !t.is_empty() => t.to_string(),
            _ if attachment_path.is_some() => {
                // Document without text — use filename as content
                msg.document
                    .as_ref()
                    .and_then(|d| d.file_name.clone())
                    .unwrap_or_else(|| "[document]".to_string())
            }
            _ => {
                // Skip non-text, non-document messages (photos, stickers, etc.)
                return;
            }
        };

        // Mention gating: in groups, only respond when @mentioned or replied to
        let is_group = matches!(msg.chat.type_field, ChatType::Group | ChatType::Supergroup);
        if is_group && ctx.mention_required {
            let is_reply_to_bot = msg
                .reply_to_message
                .as_ref()
                .and_then(|r| r.from.as_ref())
                .map(|u| u.id == ctx.bot_id)
                .unwrap_or(false);

            let mention_tag = format!("@{}", ctx.bot_username);
            let has_mention = text.to_lowercase().contains(&mention_tag);

            if !has_mention && !is_reply_to_bot {
                return; // Not addressed to us
            }

            // Strip the mention from the text so the agent sees clean input
            if has_mention && !ctx.bot_username.is_empty() {
                text = strip_mention(&text, &ctx.bot_username);
            }
        }

        tracing::info!(sender = %sender_id, chat = %chat_id, len = text.len(), "Received Telegram message");

        // Handle commands
        if text == "/start" {
            let _ =
                Self::send_text_message(api, chat_id, "Homun is ready! Send me a message.").await;
            return;
        }

        if text == "/new" || text == "/reset" {
            let _ = Self::send_text_message(api, chat_id, "Session cleared. Starting fresh.").await;
            // Continue to forward to agent for session reset handling
        }

        // Send typing indicator before forwarding to agent
        Self::send_typing(api, chat_id).await;

        // Send to agent via bus
        let metadata = attachment_path.map(|path| MessageMetadata {
            attachment_path: Some(path),
            ..Default::default()
        });
        let inbound = InboundMessage {
            channel: "telegram".to_string(),
            sender_id,
            chat_id: chat_id.to_string(),
            content: text,
            timestamp: chrono::Utc::now(),
            metadata,
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

    /// Download a Telegram document to a temporary file.
    /// Returns the path to the downloaded file.
    async fn download_document(
        api: &Bot,
        doc: &frankenstein::types::Document,
        token: &str,
    ) -> Result<std::path::PathBuf> {
        // Step 1: get file path from Telegram API
        let params = GetFileParams::builder()
            .file_id(&doc.file_id)
            .build();
        let file_info = api.get_file(&params).await?;
        let file_path = file_info
            .result
            .file_path
            .ok_or_else(|| anyhow::anyhow!("Telegram did not return file_path"))?;

        // Step 2: download the file
        let url = format!("https://api.telegram.org/file/bot{token}/{file_path}");
        let response = reqwest::get(&url).await?;
        let bytes = response.bytes().await?;

        // Step 3: save to temp directory with original filename
        let file_name = doc
            .file_name
            .as_deref()
            .unwrap_or("telegram_document");
        let dir = std::env::temp_dir().join("homun_telegram");
        tokio::fs::create_dir_all(&dir).await?;
        let dest = dir.join(file_name);
        tokio::fs::write(&dest, &bytes).await?;

        Ok(dest)
    }

    /// Send a "typing..." indicator to the chat.
    async fn send_typing(api: &Bot, chat_id: i64) {
        let params = SendChatActionParams::builder()
            .chat_id(ChatId::Integer(chat_id))
            .action(ChatAction::Typing)
            .build();
        let _ = api.send_chat_action(&params).await;
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

                if api.send_message(&params).await.is_err() {
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

/// Strip @bot_username mention from message text (case-insensitive).
fn strip_mention(text: &str, bot_username: &str) -> String {
    let tag = format!("@{bot_username}");
    // Case-insensitive removal
    let lower = text.to_lowercase();
    let mut result = String::with_capacity(text.len());
    let mut pos = 0;
    while let Some(idx) = lower[pos..].find(&tag) {
        result.push_str(&text[pos..pos + idx]);
        pos += idx + tag.len();
    }
    result.push_str(&text[pos..]);
    let trimmed = result.trim().to_string();
    if trimmed.is_empty() {
        text.to_string()
    } else {
        trimmed
    }
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

    #[test]
    fn test_strip_mention_basic() {
        assert_eq!(strip_mention("@homun hello", "homun"), "hello");
        assert_eq!(strip_mention("hello @homun", "homun"), "hello");
        assert_eq!(
            strip_mention("hey @Homun what's up", "homun"),
            "hey  what's up"
        );
    }

    #[test]
    fn test_strip_mention_only_mention() {
        // If stripping the mention leaves empty text, return original
        assert_eq!(strip_mention("@homun", "homun"), "@homun");
    }
}
