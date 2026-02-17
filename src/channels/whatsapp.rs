use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::{mpsc, Mutex};
use wacore::proto_helpers::MessageExt;
use wacore::types::events::Event;
use waproto::whatsapp as wa;
use whatsapp_rust::bot::Bot;
use whatsapp_rust::store::SqliteStore;
use whatsapp_rust_tokio_transport::TokioWebSocketTransportFactory;
use whatsapp_rust_ureq_http_client::UreqHttpClient;

use crate::bus::{InboundMessage, OutboundMessage};
use crate::config::WhatsAppConfig;

/// Max number of sent message IDs to track (prevents unbounded growth)
const SENT_IDS_MAX: usize = 500;

/// WhatsApp channel — native Rust client using whatsapp-rust library.
///
/// Architecture:
///   homunbot (Rust) <--whatsapp-rust--> WhatsApp Web (direct)
///
/// No Node.js bridge needed. Session state is stored in a local SQLite database.
///
/// **Pairing flow**: Pairing is ONLY done from the TUI (`homunbot config` → WhatsApp tab).
/// The gateway only reconnects using an existing session. If no session exists,
/// it logs a warning and exits gracefully.
pub struct WhatsAppChannel {
    config: WhatsAppConfig,
}

impl WhatsAppChannel {
    pub fn new(config: WhatsAppConfig) -> Self {
        Self { config }
    }

    /// Start the WhatsApp channel: reconnect using existing session, route messages.
    ///
    /// This does NOT initiate pairing. If the device has not been paired yet
    /// (no session in the SQLite store), it logs a warning and returns Ok.
    /// Use `homunbot config` (TUI) to pair the device first.
    pub async fn start(
        &self,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: mpsc::Receiver<OutboundMessage>,
    ) -> Result<()> {
        let allow_from: HashSet<String> = self.config.allow_from.iter().cloned().collect();
        let allow_all = allow_from.is_empty();

        // Resolve DB path
        let db_path = self.config.resolved_db_path();

        // Check if session database exists — if not, the device hasn't been paired
        if !db_path.exists() {
            tracing::warn!(
                "WhatsApp not paired yet. Run 'homunbot config' and use the WhatsApp tab to pair."
            );
            return Ok(());
        }

        tracing::info!(
            db_path = %self.config.db_path,
            allow_from = ?self.config.allow_from,
            allow_all,
            "WhatsApp channel starting (reconnect mode, no pairing)"
        );

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        // Initialize WhatsApp backend storage
        let backend = Arc::new(
            SqliteStore::new(&db_path.to_string_lossy())
                .await
                .with_context(|| format!("Failed to create WhatsApp SQLite store at {}", db_path.display()))?,
        );

        // Transport
        let transport_factory = TokioWebSocketTransportFactory::new();

        // HTTP client (for media, version fetching)
        let http_client = UreqHttpClient::new();

        // Wrap outbound_rx for the outbound sender task
        let outbound_rx = Arc::new(tokio::sync::Mutex::new(outbound_rx));

        // Track message IDs sent by the bot so we can distinguish bot-echo from user self-messages.
        // When the user sends a message to themselves, `is_from_me` is true (same as bot-sent).
        // We only skip messages whose ID we know we sent.
        let sent_ids: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

        // Build the bot with event handler — NO pair_code, reconnect only
        let inbound_tx_clone = inbound_tx.clone();
        let allow_from_clone = allow_from.clone();
        let sent_ids_for_handler = sent_ids.clone();

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

                async move {
                    match event {
                        Event::Connected(_) => {
                            tracing::info!("✅ WhatsApp connected");
                        }
                        Event::LoggedOut(_) => {
                            tracing::error!(
                                "❌ WhatsApp logged out! Re-pair with: homunbot config → WhatsApp tab"
                            );
                        }
                        Event::Message(msg, info) => {
                            handle_message(
                                msg,
                                info,
                                client,
                                &inbound_tx,
                                &allow_from,
                                allow_all,
                                &sent_ids,
                            )
                            .await;
                        }
                        // In gateway mode we don't expect pairing events, but log them if they occur
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

        // Do NOT call with_pair_code() — gateway only reconnects with existing session

        // Skip history sync if configured (default for bots)
        if self.config.skip_history_sync {
            builder = builder.skip_history_sync();
        }

        let mut bot = builder
            .build()
            .await
            .context("Failed to build WhatsApp bot")?;

        // Get client reference for sending messages
        let client = bot.client();

        // Run the bot
        let bot_handle = bot
            .run()
            .await
            .context("Failed to start WhatsApp bot")?;

        // Spawn outbound message loop
        let outbound_client = client.clone();
        let sent_ids_for_outbound = sent_ids.clone();
        let outbound_handle = tokio::spawn(async move {
            let mut rx = outbound_rx.lock().await;
            while let Some(msg) = rx.recv().await {
                if msg.channel != "whatsapp" {
                    continue;
                }

                // Split long messages (WhatsApp soft limit ~4000 chars)
                let chunks = split_message(&msg.content, 4000);

                for chunk in chunks {
                    // Parse the chat_id as a JID
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

                    match outbound_client.send_message(to, reply_message).await {
                        Ok(msg_id) => {
                            // Track this message ID so we can ignore the echo
                            let mut ids = sent_ids_for_outbound.lock().await;
                            // Prevent unbounded growth
                            if ids.len() >= SENT_IDS_MAX {
                                ids.clear();
                            }
                            ids.insert(msg_id);
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to send WhatsApp message");
                        }
                    }
                }
            }
        });

        // Wait for the bot to finish
        if let Err(e) = bot_handle.await {
            tracing::error!(error = %e, "WhatsApp bot task error");
        }

        outbound_handle.abort();
        Ok(())
    }
}

/// Handle an incoming WhatsApp message
async fn handle_message(
    msg: Box<wa::Message>,
    info: wacore::types::message::MessageInfo,
    _client: Arc<whatsapp_rust::Client>,
    inbound_tx: &mpsc::Sender<InboundMessage>,
    allow_from: &HashSet<String>,
    allow_all: bool,
    sent_ids: &Mutex<HashSet<String>>,
) {
    // Skip bot-sent messages (echo of our own replies).
    // We check the message ID against the set of IDs we sent.
    // This allows "self-messages" (user writing to their own chat) to pass through,
    // while filtering out the echo of messages the bot sent.
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

    // Extract text content
    let text = match msg.text_content() {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => return, // Skip non-text messages
    };

    // Get sender info — the sender JID may be a LID (Linked Identity) or a phone number (PN).
    // When using LID addressing, `sender_alt` contains the phone-number JID.
    // We try both for access control matching.
    let sender_jid = &info.source.sender;
    let chat_jid = &info.source.chat;

    let sender_id = if sender_jid.user.is_empty() {
        "unknown".to_string()
    } else {
        sender_jid.user.clone()
    };

    // Also extract the phone number from sender_alt (if available) or chat JID.
    // This covers the case where sender is a LID but allow_from has the phone number.
    let sender_alt_id = info
        .source
        .sender_alt
        .as_ref()
        .map(|j| j.user.clone())
        .filter(|u| !u.is_empty());

    // For self-messages, the chat JID is the user's own phone number
    let chat_user = if !chat_jid.user.is_empty() {
        Some(chat_jid.user.clone())
    } else {
        None
    };

    // Access control — match against sender, sender_alt, or chat user.
    // Self-messages (is_from_me && not bot-echo) always pass — the user is the account owner.
    if !allow_all && !is_self_message {
        let authorized = allow_from.contains(&sender_id)
            || sender_alt_id
                .as_ref()
                .is_some_and(|alt| allow_from.contains(alt))
            || chat_user
                .as_ref()
                .is_some_and(|cu| allow_from.contains(cu));

        if !authorized {
            tracing::warn!(
                sender_id = %sender_id,
                sender_alt = ?sender_alt_id,
                chat = %chat_jid,
                "WhatsApp: unauthorized user, ignoring"
            );
            return;
        }
    }

    // Skip group messages for now (personal assistant)
    if info.source.is_group {
        tracing::debug!(sender = %sender_id, "Skipping group message");
        return;
    }

    // Prefer phone number over LID for the sender_id used in InboundMessage
    let display_sender_id = sender_alt_id.unwrap_or(sender_id.clone());

    tracing::info!(
        sender = %display_sender_id,
        sender_raw = %sender_id,
        len = text.len(),
        "WhatsApp: received message"
    );

    let inbound = InboundMessage {
        channel: "whatsapp".to_string(),
        sender_id: display_sender_id,
        chat_id: chat_jid.to_string(), // Full JID as chat_id for replies
        content: text,
        timestamp: chrono::Utc::now(),
    };

    if let Err(e) = inbound_tx.send(inbound).await {
        tracing::error!(error = %e, "Failed to send to inbound bus");
    }
}

/// Parse a JID string into a whatsapp_rust Jid type
fn parse_jid(jid_str: &str) -> Option<whatsapp_rust::Jid> {
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
