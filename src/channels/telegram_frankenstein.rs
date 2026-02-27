//! Telegram channel using Frankenstein API client
//!
//! This is an experimental implementation using the `frankenstein` crate
//! instead of `teloxide` to evaluate compatibility and reduce dependencies.

use std::collections::HashSet;

use anyhow::Result;
use frankenstein::client_reqwest::Bot;
use frankenstein::types::{ChatId, AllowedUpdate};
use frankenstein::methods::{GetUpdatesParams, SendMessageParams};
use frankenstein::updates::UpdateContent;
use frankenstein::AsyncTelegramApi;
use tokio::sync::mpsc;

use crate::bus::{InboundMessage, OutboundMessage};
use crate::config::TelegramConfig;

/// Telegram channel — long polling bot via Frankenstein.
pub struct TelegramChannelFrankenstein {
    config: TelegramConfig,
}

impl TelegramChannelFrankenstein {
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
        let outbound_handle = tokio::spawn(Self::outbound_loop(
            api_for_outbound,
            outbound_rx,
        ));

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
        _api: &Bot,
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
        let chat_id = msg.chat.id.to_string();

        if !allow_all && !allow_from.contains(&sender_id) {
            tracing::warn!(sender_id = %sender_id, "Unauthorized user");
            return;
        }

        // TODO: Fix type inference issue with msg.text
        // For now, just log that we received a message
        tracing::info!(sender = %sender_id, chat = %chat_id, "Received Telegram message (Frankenstein)");

        // Placeholder: send a test message to verify the channel works
        let inbound = InboundMessage {
            channel: "telegram".to_string(),
            sender_id,
            chat_id: chat_id.clone(),
            content: "test message".to_string(),
            timestamp: chrono::Utc::now(),
        };

        if let Err(e) = inbound_tx.send(inbound).await {
            tracing::error!(error = %e, "Failed to send to inbound bus");
        }
    }

    async fn outbound_loop(
        api: Bot,
        mut outbound_rx: mpsc::Receiver<OutboundMessage>,
    ) {
        while let Some(msg) = outbound_rx.recv().await {
            if msg.channel != "telegram" {
                continue;
            }

            let chat_id: i64 = match msg.chat_id.parse() {
                Ok(id) => id,
                Err(_) => continue,
            };

            let params = SendMessageParams::builder()
                .chat_id(ChatId::Integer(chat_id))
                .text(&msg.content)
                .build();

            if let Err(e) = api.send_message(&params).await {
                tracing::error!(error = ?e, "Failed to send message");
            }
        }
    }
}
