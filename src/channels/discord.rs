use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::Result;
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::id::ChannelId;
use serenity::prelude::*;
use tokio::sync::mpsc;

use crate::bus::{InboundMessage, MessageMetadata, OutboundMessage};
use crate::channels::traits::Channel;
use crate::channels::ChannelHealthTracker;
use crate::config::DiscordConfig;

/// Discord channel — bot via serenity.
///
/// Access control: only users in `allow_from` can interact.
/// Empty allow_from = allow everyone (not recommended in production).
pub struct DiscordChannel {
    config: DiscordConfig,
    health: Option<Arc<ChannelHealthTracker>>,
}

impl DiscordChannel {
    pub fn new(config: DiscordConfig) -> Self {
        Self {
            config,
            health: None,
        }
    }

    /// Attach a health tracker for reconnect monitoring.
    pub fn with_health(mut self, health: Arc<ChannelHealthTracker>) -> Self {
        self.health = Some(health);
        self
    }
}

#[async_trait::async_trait]
impl Channel for DiscordChannel {
    fn name(&self) -> &str {
        "discord"
    }

    async fn start(
        &self,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: mpsc::Receiver<OutboundMessage>,
    ) -> Result<()> {
        if self.config.default_channel_id.is_empty() {
            tracing::warn!(
                "Discord: default_channel_id not set — proactive messaging disabled. \
                 Set [channels.discord] default_channel_id in config.toml to enable."
            );
        }

        tracing::info!(
            default_channel_id = %self.config.default_channel_id,
            "Discord channel starting"
        );

        let mention_required = self.config.mention_required;

        let handler = Handler {
            inbound_tx,
            mention_required,
            bot_user_id: Arc::new(AtomicU64::new(0)),
            health: self.health.clone(),
        };

        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;

        let mut client = Client::builder(&self.config.token, intents)
            .event_handler(handler)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create Discord client: {e}"))?;

        // Store outbound_rx in TypeMap so the ready handler can spawn the outbound loop
        {
            let mut data = client.data.write().await;
            data.insert::<OutboundRxKey>(Arc::new(tokio::sync::Mutex::new(Some(outbound_rx))));
        }

        if let Err(e) = client.start().await {
            tracing::error!(error = %e, "Discord client error");
        }

        Ok(())
    }
}

// --- TypeMap key for passing outbound_rx through serenity's data store ---

struct OutboundRxKey;
impl TypeMapKey for OutboundRxKey {
    type Value = Arc<tokio::sync::Mutex<Option<mpsc::Receiver<OutboundMessage>>>>;
}

/// Serenity event handler — receives Discord events and routes messages to the agent.
struct Handler {
    inbound_tx: mpsc::Sender<InboundMessage>,
    mention_required: bool,
    bot_user_id: Arc<AtomicU64>,
    health: Option<Arc<ChannelHealthTracker>>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        // Ignore messages from bots (including ourselves)
        if msg.author.bot {
            return;
        }

        let sender_id = msg.author.id.to_string();
        let chat_id = msg.channel_id.to_string();

        // Auth is handled by the gateway — channels are transport-only.

        let mut text = msg.content.clone();

        // Download first attachment (if any)
        let attachment_path = if let Some(attachment) = msg.attachments.first() {
            match download_discord_attachment(attachment).await {
                Ok(path) => {
                    tracing::info!(
                        filename = %attachment.filename,
                        size = attachment.size,
                        "Discord: downloaded attachment"
                    );
                    // If no text, use filename as content
                    if text.is_empty() {
                        text = format!("[document] {}", attachment.filename);
                    }
                    Some(path)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Discord: failed to download attachment");
                    None
                }
            }
        } else {
            None
        };

        if text.is_empty() {
            return;
        }

        // Mention gating: in guilds, only respond when @mentioned
        let is_dm = msg.guild_id.is_none();
        if !is_dm && self.mention_required {
            let bot_id = self.bot_user_id.load(Ordering::Relaxed);
            if bot_id == 0 {
                return; // Bot ID not yet known (ready hasn't fired)
            }
            let mentioned = msg.mentions.iter().any(|u| u.id.get() == bot_id);
            if !mentioned {
                return; // Not addressed to us
            }
            // Strip mention tags from text: <@123> or <@!123>
            text = text
                .replace(&format!("<@{bot_id}>"), "")
                .replace(&format!("<@!{bot_id}>"), "");
            text = text.trim().to_string();
            if text.is_empty() {
                return;
            }
        }

        // Handle commands
        if text == "!start" {
            if let Err(e) = msg
                .channel_id
                .say(&ctx.http, "Homun is ready! Send me a message.")
                .await
            {
                tracing::error!(error = %e, "Failed to send start message");
            }
            return;
        }

        if text == "!new" || text == "!reset" {
            if let Err(e) = msg
                .channel_id
                .say(&ctx.http, "Session cleared. Starting fresh.")
                .await
            {
                tracing::error!(error = %e, "Failed to send reset message");
            }
        }

        tracing::info!(
            sender = %sender_id,
            chat = %chat_id,
            len = text.len(),
            "Discord: received message"
        );

        // Record successful message in health tracker
        if let Some(ref health) = self.health {
            health.record_message("discord");
        }

        // Send typing indicator
        let _ = ctx.http.broadcast_typing(msg.channel_id).await;

        let metadata = if attachment_path.is_some() {
            Some(MessageMetadata {
                attachment_path,
                ..Default::default()
            })
        } else {
            None
        };

        let inbound = InboundMessage {
            channel: "discord".to_string(),
            sender_id,
            chat_id: chat_id.clone(),
            content: text,
            timestamp: chrono::Utc::now(),
            metadata,
        };

        if let Err(e) = self.inbound_tx.send(inbound).await {
            tracing::error!(error = %e, "Failed to send to inbound bus");
            if let Err(e2) = msg
                .channel_id
                .say(
                    &ctx.http,
                    "Sorry, I'm having trouble processing messages right now.",
                )
                .await
            {
                tracing::error!(error = %e2, "Failed to send error message");
            }
            return;
        }

        // React with checkmark to acknowledge receipt
        if let Err(e) = msg.react(&ctx.http, '✅').await {
            tracing::debug!(error = %e, "Discord: failed to add reaction ACK");
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        // Store bot user ID for mention detection
        self.bot_user_id
            .store(ready.user.id.get(), Ordering::Relaxed);

        tracing::info!(
            user = %ready.user.name,
            bot_id = %ready.user.id,
            "Discord bot connected"
        );

        // Take the outbound_rx from the data store and spawn the outbound loop.
        // We take it (Option::take) so this only happens once, even if ready fires again.
        let http = ctx.http.clone();
        let rx_opt = {
            let data = ctx.data.read().await;
            data.get::<OutboundRxKey>().cloned()
        };

        if let Some(rx_lock) = rx_opt {
            let mut guard = rx_lock.lock().await;
            if let Some(rx) = guard.take() {
                tokio::spawn(outbound_loop(http, rx));
            }
        }
    }

    async fn resume(&self, _ctx: Context, _event: serenity::model::event::ResumedEvent) {
        tracing::info!("Discord: session resumed after reconnect");
        if let Some(ref health) = self.health {
            health.record_message("discord"); // Treat resume as healthy signal
        }
    }

    async fn cache_ready(&self, _ctx: Context, guilds: Vec<serenity::model::id::GuildId>) {
        tracing::info!(
            guild_count = guilds.len(),
            "Discord: cache ready, guilds loaded"
        );
    }
}

/// Outbound loop: receive agent responses and send to Discord.
///
/// Supports thread routing via `metadata.thread_id` — if present, sends to
/// that thread instead of the main channel. This enables proactive messaging
/// to specific threads (e.g. from automations or cross-channel routing).
async fn outbound_loop(http: Arc<serenity::http::Http>, mut rx: mpsc::Receiver<OutboundMessage>) {
    while let Some(msg) = rx.recv().await {
        if msg.channel != "discord" {
            continue;
        }

        let channel_id: u64 = match msg.chat_id.parse() {
            Ok(id) => id,
            Err(e) => {
                tracing::error!(
                    chat_id = %msg.chat_id,
                    error = %e,
                    "Invalid Discord channel_id"
                );
                continue;
            }
        };

        // Use thread_id from metadata if available (thread routing)
        let target_id = msg
            .metadata
            .as_ref()
            .and_then(|m| m.thread_id.as_ref())
            .and_then(|tid| tid.parse::<u64>().ok())
            .unwrap_or(channel_id);

        // Discord message limit is 2000 chars
        let chunks = split_message(&msg.content, 1900);

        for chunk in chunks {
            if let Err(e) = ChannelId::new(target_id).say(&http, &chunk).await {
                tracing::error!(error = %e, channel_id, target_id, "Failed to send Discord message");
            }
        }
    }
}

/// Split a message into chunks that fit within Discord's 2000 character limit.
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

        remaining = rest.strip_prefix('\n').unwrap_or(rest);
    }

    chunks
}

/// Download a Discord attachment to a temp directory.
async fn download_discord_attachment(
    attachment: &serenity::model::channel::Attachment,
) -> Result<String> {
    let dir = std::env::temp_dir().join("homun_discord");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create Discord attachment dir: {e}"))?;

    let bytes = reqwest::get(&attachment.url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to download attachment: {e}"))?
        .bytes()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read attachment bytes: {e}"))?;

    let dest = dir.join(&attachment.filename);
    tokio::fs::write(&dest, &bytes)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to write attachment: {e}"))?;

    Ok(dest.to_string_lossy().to_string())
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
    fn test_split_exact_limit() {
        let msg = "a".repeat(50);
        let chunks = split_message(&msg, 50);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 50);
    }
}
