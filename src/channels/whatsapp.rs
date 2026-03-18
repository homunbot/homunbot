use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::sync::{mpsc, Mutex};
use wa_rs::bot::Bot;
use wa_rs::store::SqliteStore;
use wa_rs_core::download::Downloadable;
use wa_rs_core::proto_helpers::MessageExt;
use wa_rs_core::types::events::Event;
use wa_rs_proto::whatsapp as wa;
use wa_rs_tokio_transport::TokioWebSocketTransportFactory;
use wa_rs_ureq_http::UreqHttpClient;

use crate::bus::{InboundMessage, MessageMetadata, OutboundMessage};
use crate::channels::traits::Channel;
use crate::config::WhatsAppConfig;

/// Max number of sent message IDs to track (prevents unbounded growth)
const SENT_IDS_MAX: usize = 500;

/// Grace period (seconds) after connection during which incoming messages are ignored.
/// This prevents the bot from replying to queued/offline messages received during reconnect.
const CONNECT_GRACE_PERIOD_SECS: u64 = 10;

/// WhatsApp channel — native Rust client using whatsapp-rust library.
///
/// Architecture:
///   homun (Rust) <--whatsapp-rust--> WhatsApp Web (direct)
///
/// No Node.js bridge needed. Session state is stored in a local SQLite database.
///
/// **Pairing flow**: Pairing is ONLY done from the TUI (`homun config` → WhatsApp tab).
/// The gateway only reconnects using an existing session. If no session exists,
/// it logs a warning and exits gracefully.
pub struct WhatsAppChannel {
    config: WhatsAppConfig,
}

impl WhatsAppChannel {
    pub fn new(config: WhatsAppConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl Channel for WhatsAppChannel {
    fn name(&self) -> &str {
        "whatsapp"
    }

    /// Start the WhatsApp channel with automatic reconnect on failure.
    ///
    /// This does NOT initiate pairing. If the device has not been paired yet
    /// (no session in the SQLite store), it logs a warning and returns Ok.
    /// Use `homun config` (TUI) to pair the device first.
    ///
    /// On disconnection or error, the bot reconnects with exponential backoff
    /// (2s → 4s → 8s → ... → 120s cap). Backoff resets after a stable connection.
    async fn start(
        &self,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: mpsc::Receiver<OutboundMessage>,
    ) -> Result<()> {
        let allow_from: HashSet<String> = self.config.allow_from.iter().cloned().collect();

        // SAFETY: if allow_from is empty, reject ALL messages (fail-closed).
        if allow_from.is_empty() {
            tracing::error!(
                "WhatsApp allow_from is empty! For safety, the bot will NOT respond to anyone. \
                 Set [channels.whatsapp] allow_from = [\"your_phone_number\"] in config.toml"
            );
        }

        // Proactive messaging info
        if !self.config.phone_number.is_empty() {
            tracing::info!(
                phone = %self.config.phone_number,
                "WhatsApp proactive messaging enabled (phone_number configured)"
            );
        } else {
            tracing::warn!(
                "WhatsApp: phone_number not set — proactive messaging disabled. \
                 Set [channels.whatsapp] phone_number in config.toml to enable."
            );
        }

        // Resolve DB path
        let db_path = self.config.resolved_db_path();

        // Check if session database exists — if not, the device hasn't been paired
        if !db_path.exists() {
            tracing::warn!(
                "WhatsApp not paired yet. Run 'homun config' and use the WhatsApp tab to pair."
            );
            return Ok(());
        }

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        // Wrap outbound_rx for sharing across reconnect sessions
        let outbound_rx = Arc::new(tokio::sync::Mutex::new(Some(outbound_rx)));

        // Reconnect loop with exponential backoff
        let mut backoff = Duration::from_secs(2);
        const MAX_BACKOFF: Duration = Duration::from_secs(120);

        loop {
            tracing::info!(
                db_path = %self.config.db_path,
                allow_from = ?self.config.allow_from,
                "WhatsApp channel starting session (reconnect mode)"
            );

            match self
                .run_session(&inbound_tx, &outbound_rx, &allow_from, &db_path)
                .await
            {
                Ok(()) => {
                    // Clean exit (e.g. logged out, channel closed)
                    tracing::info!("WhatsApp session ended cleanly");
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        backoff_secs = backoff.as_secs(),
                        "WhatsApp session failed, reconnecting after backoff"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                }
            }
        }
    }
}

impl WhatsAppChannel {
    /// Run a single WhatsApp bot session. Returns Ok(()) on clean exit, Err on failure.
    async fn run_session(
        &self,
        inbound_tx: &mpsc::Sender<InboundMessage>,
        outbound_rx: &Arc<tokio::sync::Mutex<Option<mpsc::Receiver<OutboundMessage>>>>,
        allow_from: &HashSet<String>,
        db_path: &std::path::Path,
    ) -> Result<()> {
        let allow_all = false; // NEVER allow all — always require explicit allow_from

        // Initialize WhatsApp backend storage
        let backend = Arc::new(
            SqliteStore::new(&db_path.to_string_lossy())
                .await
                .with_context(|| {
                    format!(
                        "Failed to create WhatsApp SQLite store at {}",
                        db_path.display()
                    )
                })?,
        );

        let transport_factory = TokioWebSocketTransportFactory::new();
        let http_client = UreqHttpClient::new();

        // Track message IDs sent by the bot so we can distinguish bot-echo from user self-messages.
        let sent_ids: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

        let inbound_tx_clone = inbound_tx.clone();
        let allow_from_clone = allow_from.clone();
        let sent_ids_for_handler = sent_ids.clone();

        // Track when the bot connects to apply grace period
        let is_ready = Arc::new(AtomicBool::new(false));
        let is_ready_for_handler = is_ready.clone();
        let connect_time: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
        let connect_time_for_handler = connect_time.clone();

        // Track if session was logged out (clean exit, don't reconnect)
        let logged_out = Arc::new(AtomicBool::new(false));
        let logged_out_for_handler = logged_out.clone();

        let bot_name = self.config.bot_name.clone();

        let mut builder = Bot::builder()
            .with_backend(backend)
            .with_transport_factory(transport_factory)
            .with_http_client(http_client)
            .with_device_props(
                Some("Linux".to_string()),
                None,
                Some(wa::device_props::PlatformType::Chrome),
            )
            .on_event(move |event, client| {
                let inbound_tx = inbound_tx_clone.clone();
                let allow_from = allow_from_clone.clone();
                let sent_ids = sent_ids_for_handler.clone();
                let is_ready = is_ready_for_handler.clone();
                let connect_time = connect_time_for_handler.clone();
                let logged_out = logged_out_for_handler.clone();
                let bot_name = bot_name.clone();

                async move {
                    match event {
                        Event::Connected(_) => {
                            tracing::info!("WhatsApp connected — grace period {CONNECT_GRACE_PERIOD_SECS}s (ignoring queued messages)");
                            {
                                let mut ct = connect_time.lock().await;
                                *ct = Some(Instant::now());
                            }
                            // Set presence to "online"
                            if let Err(e) = client.presence().set_available().await {
                                tracing::debug!("WhatsApp: failed to set presence: {e}");
                            }
                            let is_ready_delayed = is_ready.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep(tokio::time::Duration::from_secs(CONNECT_GRACE_PERIOD_SECS)).await;
                                is_ready_delayed.store(true, Ordering::SeqCst);
                                tracing::info!("WhatsApp grace period ended — now processing messages");
                            });
                        }
                        Event::LoggedOut(_) => {
                            is_ready.store(false, Ordering::SeqCst);
                            logged_out.store(true, Ordering::SeqCst);
                            tracing::error!(
                                "WhatsApp logged out! Re-pair with: homun config -> WhatsApp tab"
                            );
                        }
                        Event::Message(msg, info) => {
                            if !is_ready.load(Ordering::SeqCst) {
                                tracing::debug!(
                                    msg_id = %info.id,
                                    "Dropping message received during grace period"
                                );
                                return;
                            }

                            handle_message(
                                msg,
                                info,
                                client,
                                &inbound_tx,
                                &allow_from,
                                allow_all,
                                &sent_ids,
                                &bot_name,
                            )
                            .await;
                        }
                        Event::PairingCode { code, .. } => {
                            tracing::warn!(
                                code = %code,
                                "Unexpected pairing code in gateway mode. Use TUI to pair."
                            );
                        }
                        Event::PairError(err) => {
                            tracing::error!(?err, "WhatsApp pairing error in gateway mode");
                        }
                        _ => {}
                    }
                }
            });

        if self.config.skip_history_sync {
            builder = builder.skip_history_sync();
        }

        let mut bot = builder
            .build()
            .await
            .context("Failed to build WhatsApp bot")?;

        let client = bot.client();
        let bot_handle = bot.run().await.context("Failed to start WhatsApp bot")?;

        // Spawn outbound message loop (take the receiver — only first session gets it)
        let outbound_client = client.clone();
        let sent_ids_for_outbound = sent_ids.clone();
        let rx_arc = outbound_rx.clone();
        let outbound_handle = tokio::spawn(async move {
            let mut guard = rx_arc.lock().await;
            let rx = match guard.take() {
                Some(rx) => rx,
                None => return, // Already taken by previous session
            };
            let mut rx = rx;
            while let Some(msg) = rx.recv().await {
                if msg.channel != "whatsapp" {
                    continue;
                }

                let chunks = split_message(&msg.content, 4000);

                for chunk in chunks {
                    let to = match parse_jid(&msg.chat_id) {
                        Some(jid) => jid,
                        None => {
                            tracing::error!(chat_id = %msg.chat_id, "Invalid JID for WhatsApp reply");
                            continue;
                        }
                    };

                    let reply_message = wa::Message {
                        conversation: Some(chunk),
                        ..Default::default()
                    };

                    match outbound_client
                        .send_message(to.clone(), reply_message)
                        .await
                    {
                        Ok(msg_id) => {
                            let mut ids = sent_ids_for_outbound.lock().await;
                            if ids.len() >= SENT_IDS_MAX {
                                ids.clear();
                            }
                            ids.insert(msg_id);
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to send WhatsApp message");
                        }
                    }

                    // Clear typing indicator after reply
                    if let Err(e) = outbound_client.chatstate().send_paused(&to).await {
                        tracing::debug!("WhatsApp: failed to clear typing: {e}");
                    }
                }
            }
        });

        // Wait for the bot to finish
        if let Err(e) = bot_handle.await {
            tracing::error!(error = %e, "WhatsApp bot task error");
        }

        outbound_handle.abort();

        // If logged out, return Ok (clean exit, don't reconnect)
        if logged_out.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Otherwise it's an unexpected disconnect — return error to trigger reconnect
        anyhow::bail!("WhatsApp session disconnected unexpectedly")
    }
}

/// Max age (seconds) for a message to be processed. Messages older than this are dropped.
/// This prevents the bot from replying to old queued messages on reconnect.
const MAX_MESSAGE_AGE_SECS: i64 = 120;

/// Handle an incoming WhatsApp message
#[allow(clippy::too_many_arguments)]
async fn handle_message(
    msg: Box<wa::Message>,
    info: wa_rs_core::types::message::MessageInfo,
    client: Arc<wa_rs::Client>,
    inbound_tx: &mpsc::Sender<InboundMessage>,
    allow_from: &HashSet<String>,
    allow_all: bool,
    sent_ids: &Mutex<HashSet<String>>,
    bot_name: &str,
) {
    // --- SAFETY CHECK 1: Message age ---
    let now = chrono::Utc::now();
    let age = now.signed_duration_since(info.timestamp);
    let age_secs = age.num_seconds();
    if age_secs > MAX_MESSAGE_AGE_SECS {
        tracing::debug!(
            msg_id = %info.id,
            age_secs,
            "Dropping old message (age > {}s)",
            MAX_MESSAGE_AGE_SECS,
        );
        return;
    }
    if age_secs < -60 {
        tracing::debug!(msg_id = %info.id, age_secs, "Dropping message with future timestamp");
        return;
    }

    // --- SAFETY CHECK 2: Bot echo ---
    let is_self_message = info.source.is_from_me;
    if is_self_message {
        let is_bot_echo = {
            let mut ids = sent_ids.lock().await;
            ids.remove(&info.id)
        };
        if is_bot_echo {
            tracing::debug!(msg_id = %info.id, "Skipping bot-sent echo");
            return;
        }
        tracing::debug!(msg_id = %info.id, "Processing self-message (not a bot echo)");
    }

    // Unwrap wrappers (ephemeral, view-once, document_with_caption, edited)
    let base_msg = msg.get_base_message();

    // Extract text content — try text first, then caption for media messages
    let mut text = base_msg
        .text_content()
        .map(|t| t.to_string())
        .or_else(|| base_msg.get_caption().map(|c| c.to_string()))
        .unwrap_or_default();

    // Try to download media (image or document)
    let attachment_path = download_media(base_msg, &client).await;

    // If no text but we have media, use a descriptive placeholder
    if text.is_empty() {
        if let Some(ref _path) = attachment_path {
            text = "[media attachment]".to_string();
        } else {
            return; // No text, no media — nothing to process
        }
    }

    // --- SAFETY CHECK 3: Group mention gating ---
    if info.source.is_group {
        if !is_mentioned_in_group(base_msg, bot_name, &info) {
            tracing::debug!(msg_id = %info.id, "Skipping group message (bot not mentioned)");
            return;
        }
        // Strip bot mention from text
        let mention_tag = format!("@{bot_name}");
        text = text.replace(&mention_tag, "").trim().to_string();
        if text.is_empty() && attachment_path.is_none() {
            return;
        }
    }

    // Get sender info
    let sender_jid = &info.source.sender;
    let chat_jid = &info.source.chat;

    let sender_id = if sender_jid.user.is_empty() {
        "unknown".to_string()
    } else {
        sender_jid.user.clone()
    };

    let sender_alt_id = info
        .source
        .sender_alt
        .as_ref()
        .map(|j| j.user.clone())
        .filter(|u| !u.is_empty());

    let chat_user = if !chat_jid.user.is_empty() {
        Some(chat_jid.user.clone())
    } else {
        None
    };

    // --- SAFETY CHECK 4: Access control ---
    if !allow_all && !is_self_message {
        let authorized = allow_from.contains(&sender_id)
            || sender_alt_id
                .as_ref()
                .is_some_and(|alt| allow_from.contains(alt))
            || chat_user.as_ref().is_some_and(|cu| allow_from.contains(cu));

        if !authorized {
            tracing::warn!(
                sender_id = %sender_id,
                sender_alt = ?sender_alt_id,
                chat = %chat_jid,
                "WhatsApp: unauthorized sender, ignoring"
            );
            return;
        }
    }

    let display_sender_id = sender_alt_id.unwrap_or(sender_id.clone());

    tracing::info!(
        sender = %display_sender_id,
        sender_raw = %sender_id,
        is_group = info.source.is_group,
        has_attachment = attachment_path.is_some(),
        len = text.len(),
        "WhatsApp: received message"
    );

    // Send typing indicator ("composing") to show the bot is processing
    if let Some(jid) = parse_jid(&chat_jid.to_string()) {
        if let Err(e) = client.chatstate().send_composing(&jid).await {
            tracing::debug!("WhatsApp: failed to send typing indicator: {e}");
        }
    }

    let metadata = if attachment_path.is_some() {
        Some(MessageMetadata {
            attachment_path,
            ..Default::default()
        })
    } else {
        None
    };

    let inbound = InboundMessage {
        channel: "whatsapp".to_string(),
        sender_id: display_sender_id,
        chat_id: chat_jid.to_string(),
        content: text,
        timestamp: chrono::Utc::now(),
        metadata,
    };

    if let Err(e) = inbound_tx.send(inbound).await {
        tracing::error!(error = %e, "Failed to send to inbound bus");
    }
}

/// Check if the bot is mentioned in a group message.
///
/// Checks three sources:
/// 1. `mentioned_jid` in ContextInfo (formal @mention)
/// 2. Text contains `@bot_name` (informal text mention)
/// 3. Text contains the bot's phone number
fn is_mentioned_in_group(
    msg: &wa::Message,
    bot_name: &str,
    _info: &wa_rs_core::types::message::MessageInfo,
) -> bool {
    // Check formal mention via ContextInfo.mentioned_jid
    // ContextInfo can be in extended_text_message or other message types
    let mentioned_jids = msg
        .extended_text_message
        .as_ref()
        .and_then(|ext| ext.context_info.as_ref())
        .map(|ctx| &ctx.mentioned_jid);

    if let Some(jids) = mentioned_jids {
        // Check if any mentioned JID matches the chat's own JID (bot's JID)
        // In groups, info.source.chat is the group JID — we need the bot's own JID
        // which isn't directly available here. Instead check by bot_name pattern.
        let bot_jid_suffix = "@s.whatsapp.net";
        for jid in jids {
            // The mentioned_jid could be the bot's full JID or LID
            if jid.contains(bot_name) || jid.ends_with(bot_jid_suffix) {
                return true;
            }
        }
    }

    // Fallback: check text content for informal @mention
    let text = msg
        .text_content()
        .or_else(|| msg.get_caption())
        .unwrap_or("");

    let mention_tag = format!("@{bot_name}");
    if text.contains(&mention_tag) {
        return true;
    }

    false
}

/// Download media from a WhatsApp message (image or document).
/// Returns the local file path if download was successful.
async fn download_media(msg: &wa::Message, client: &wa_rs::Client) -> Option<String> {
    let dir = std::env::temp_dir().join("homun_whatsapp");
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        tracing::warn!(error = %e, "Failed to create WhatsApp media dir");
        return None;
    }

    // Try image message
    if let Some(ref img) = msg.image_message {
        let filename = format!("image_{}.jpg", chrono::Utc::now().timestamp_millis());
        return download_and_save(client, img.as_ref(), &dir, &filename).await;
    }

    // Try document message
    if let Some(ref doc) = msg.document_message {
        let filename = doc.file_name.as_deref().unwrap_or("document").to_string();
        return download_and_save(client, doc.as_ref(), &dir, &filename).await;
    }

    // Try audio message
    if let Some(ref audio) = msg.audio_message {
        let filename = format!("audio_{}.ogg", chrono::Utc::now().timestamp_millis());
        return download_and_save(client, audio.as_ref(), &dir, &filename).await;
    }

    // Try video message
    if let Some(ref video) = msg.video_message {
        let filename = format!("video_{}.mp4", chrono::Utc::now().timestamp_millis());
        return download_and_save(client, video.as_ref(), &dir, &filename).await;
    }

    None
}

/// Download a Downloadable media item and save to disk.
async fn download_and_save(
    client: &wa_rs::Client,
    downloadable: &dyn Downloadable,
    dir: &std::path::Path,
    filename: &str,
) -> Option<String> {
    match client.download(downloadable).await {
        Ok(bytes) => {
            let dest = dir.join(filename);
            match tokio::fs::write(&dest, &bytes).await {
                Ok(()) => {
                    tracing::info!(
                        filename = %filename,
                        size = bytes.len(),
                        "WhatsApp: downloaded media"
                    );
                    Some(dest.to_string_lossy().to_string())
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to write WhatsApp media file");
                    None
                }
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, filename = %filename, "Failed to download WhatsApp media");
            None
        }
    }
}

/// Parse a JID string into a wa_rs Jid type
fn parse_jid(jid_str: &str) -> Option<wa_rs::Jid> {
    // JID format: "user@server" or "user@server/device"
    let full = if jid_str.contains('@') {
        jid_str.to_string()
    } else {
        // If no @, assume it's a phone number — add @s.whatsapp.net
        format!("{jid_str}@s.whatsapp.net")
    };
    full.parse().ok()
}

/// Split a message into chunks for WhatsApp's soft limit.
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

        let split_at = remaining[..max_len].rfind('\n').unwrap_or(max_len);

        let (chunk, rest) = remaining.split_at(split_at);
        chunks.push(chunk.to_string());

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
    fn test_parse_jid_full() {
        let jid = parse_jid("393331234567@s.whatsapp.net");
        assert!(jid.is_some());
    }

    #[test]
    fn test_parse_jid_phone_only() {
        let jid = parse_jid("393331234567");
        assert!(jid.is_some());
    }

    #[test]
    fn test_sender_id_extraction() {
        // Test stripping @s.whatsapp.net
        let sender = "393331234567@s.whatsapp.net";
        let id = sender.split('@').next().unwrap_or(sender);
        assert_eq!(id, "393331234567");
    }
}
