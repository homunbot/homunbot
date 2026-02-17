use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::bus::{InboundMessage, OutboundMessage};
use crate::channels::{DiscordChannel, TelegramChannel, WhatsAppChannel};
use crate::config::Config;
use crate::scheduler::{CronEvent, CronScheduler};
use crate::session::SessionManager;

use super::AgentLoop;

/// A running channel: name + task handle + outbound sender
struct ChannelHandle {
    name: String,
    handle: tokio::task::JoinHandle<()>,
    outbound_tx: mpsc::Sender<OutboundMessage>,
}

/// Gateway — orchestrates channels, agent loop, cron scheduler, and message routing.
///
/// Architecture:
/// ```text
/// Telegram ─┐
/// Cron ──────┤──→ InboundMessage ──→ Gateway ──→ AgentLoop ──→ OutboundMessage ──→ Channel
/// (future) ──┘
/// ```
pub struct Gateway {
    agent: Arc<AgentLoop>,
    config: Config,
    #[allow(dead_code)]
    session_manager: SessionManager,
    cron_scheduler: Arc<CronScheduler>,
    cron_event_rx: mpsc::Receiver<CronEvent>,
    /// Receiver for messages sent by tools (MessageTool) that need routing to channels
    tool_message_rx: Option<mpsc::Receiver<OutboundMessage>>,
}

impl Gateway {
    pub fn new(
        agent: Arc<AgentLoop>,
        config: Config,
        session_manager: SessionManager,
        cron_scheduler: Arc<CronScheduler>,
        cron_event_rx: mpsc::Receiver<CronEvent>,
    ) -> Self {
        Self {
            agent,
            config,
            session_manager,
            cron_scheduler,
            cron_event_rx,
            tool_message_rx: None,
        }
    }

    /// Set the receiver for tool-originated messages (from MessageTool)
    pub fn set_tool_message_rx(&mut self, rx: mpsc::Receiver<OutboundMessage>) {
        self.tool_message_rx = Some(rx);
    }

    /// Start the gateway — runs all channels + cron + agent loop.
    /// Blocks until Ctrl+C.
    pub async fn run(self) -> Result<()> {
        let (inbound_tx, mut inbound_rx) = mpsc::channel::<InboundMessage>(100);
        let mut channels: Vec<ChannelHandle> = Vec::new();

        // --- Start Telegram channel ---
        if self.config.channels.telegram.enabled {
            let tg_config = self.config.channels.telegram.clone();
            let tg_inbound_tx = inbound_tx.clone();
            let (tg_outbound_tx, tg_outbound_rx) = mpsc::channel::<OutboundMessage>(100);

            let handle = tokio::spawn(async move {
                let channel = TelegramChannel::new(tg_config);
                if let Err(e) = channel.start(tg_inbound_tx, tg_outbound_rx).await {
                    tracing::error!(error = %e, "Telegram channel error");
                }
            });

            channels.push(ChannelHandle {
                name: "telegram".to_string(),
                handle,
                outbound_tx: tg_outbound_tx,
            });
            tracing::info!("Telegram channel started");
        }

        // --- Start Discord channel ---
        if self.config.channels.discord.enabled {
            let dc_config = self.config.channels.discord.clone();
            let dc_inbound_tx = inbound_tx.clone();
            let (dc_outbound_tx, dc_outbound_rx) = mpsc::channel::<OutboundMessage>(100);

            let handle = tokio::spawn(async move {
                let channel = DiscordChannel::new(dc_config);
                if let Err(e) = channel.start(dc_inbound_tx, dc_outbound_rx).await {
                    tracing::error!(error = %e, "Discord channel error");
                }
            });

            channels.push(ChannelHandle {
                name: "discord".to_string(),
                handle,
                outbound_tx: dc_outbound_tx,
            });
            tracing::info!("Discord channel started");
        }

        // --- Start WhatsApp channel ---
        if self.config.channels.whatsapp.enabled {
            let wa_config = self.config.channels.whatsapp.clone();
            let wa_inbound_tx = inbound_tx.clone();
            let (wa_outbound_tx, wa_outbound_rx) = mpsc::channel::<OutboundMessage>(100);

            let handle = tokio::spawn(async move {
                let channel = WhatsAppChannel::new(wa_config);
                if let Err(e) = channel.start(wa_inbound_tx, wa_outbound_rx).await {
                    tracing::error!(error = %e, "WhatsApp channel error");
                }
            });

            channels.push(ChannelHandle {
                name: "whatsapp".to_string(),
                handle,
                outbound_tx: wa_outbound_tx,
            });
            tracing::info!("WhatsApp channel started");
        }

        // --- Start Cron scheduler (created externally, started here) ---
        let _cron_handle = self.cron_scheduler.clone().start().await?;
        let mut cron_event_rx = self.cron_event_rx;
        tracing::info!("Cron scheduler started");

        if channels.is_empty() {
            println!("No channels enabled. Set [channels.telegram] enabled = true in ~/.homunbot/config.toml");
            return Ok(());
        }

        // Drop our copy — channels hold their own
        drop(inbound_tx);

        let active = channels.len();
        tracing::info!(channels = active, "Gateway running");
        println!("🧪 HomunBot gateway running ({active} channel(s) + cron). Press Ctrl+C to stop.");

        // Build outbound routing table: channel_name → sender
        let outbound_senders: Vec<(String, mpsc::Sender<OutboundMessage>)> = channels
            .iter()
            .map(|ch| (ch.name.clone(), ch.outbound_tx.clone()))
            .collect();

        // --- Main message routing loop ---
        let agent = self.agent.clone();
        let senders_for_routing = outbound_senders.clone();

        let routing_loop = tokio::spawn(async move {
            while let Some(inbound) = inbound_rx.recv().await {
                let session_key = inbound.session_key();
                let channel_name = inbound.channel.clone();
                let chat_id = inbound.chat_id.clone();

                tracing::info!(
                    channel = %channel_name,
                    session = %session_key,
                    "Processing inbound message"
                );

                // Skip /new — already handled by channel
                if inbound.content == "/new" || inbound.content == "/reset" {
                    continue;
                }

                // Process through agent loop (spawned per-message)
                let agent = agent.clone();
                let senders = senders_for_routing.clone();

                tokio::spawn(async move {
                    let response = match agent
                        .process_message(&inbound.content, &session_key, &channel_name, &chat_id)
                        .await
                    {
                        Ok(text) => text,
                        Err(e) => {
                            tracing::error!(error = %e, "Agent error");
                            format!("Sorry, I encountered an error: {e}")
                        }
                    };

                    tracing::info!(
                        channel = %channel_name,
                        response_len = response.len(),
                        "Agent response ready, routing to channel"
                    );

                    let outbound = OutboundMessage {
                        channel: channel_name.clone(),
                        chat_id,
                        content: response,
                    };

                    route_outbound(outbound, &senders).await;
                });
            }
        });

        // --- Cron event loop: process cron fires through agent ---
        let agent_for_cron = self.agent.clone();
        let senders_for_cron = outbound_senders.clone();

        let cron_loop = tokio::spawn(async move {
            while let Some(event) = cron_event_rx.recv().await {
                tracing::info!(
                    job_id = %event.job_id,
                    job_name = %event.job_name,
                    "Processing cron event"
                );

                let session_key = format!("cron:{}", event.job_id);
                let agent = agent_for_cron.clone();
                let senders = senders_for_cron.clone();
                let deliver_to = event.deliver_to.clone();

                tokio::spawn(async move {
                    // Parse deliver_to for channel routing
                    let (cron_channel, cron_chat_id) = deliver_to
                        .as_deref()
                        .and_then(|d| d.split_once(':'))
                        .map(|(c, id)| (c.to_string(), id.to_string()))
                        .unwrap_or_else(|| ("cron".to_string(), event.job_id.clone()));

                    let response = match agent
                        .process_message(&event.message, &session_key, &cron_channel, &cron_chat_id)
                        .await
                    {
                        Ok(text) => text,
                        Err(e) => {
                            tracing::error!(error = %e, "Cron agent error");
                            return;
                        }
                    };

                    // Deliver response if configured
                    if deliver_to.is_some() {
                        let outbound = OutboundMessage {
                            channel: cron_channel,
                            chat_id: cron_chat_id,
                            content: response,
                        };
                        route_outbound(outbound, &senders).await;
                    }
                });
            }
        });

        // --- Tool message loop: forward messages from MessageTool to channels ---
        let tool_msg_loop = if let Some(mut tool_rx) = self.tool_message_rx {
            let senders_for_tools = outbound_senders.clone();
            Some(tokio::spawn(async move {
                while let Some(outbound) = tool_rx.recv().await {
                    tracing::info!(
                        channel = %outbound.channel,
                        chat_id = %outbound.chat_id,
                        "Routing tool-originated message"
                    );
                    route_outbound(outbound, &senders_for_tools).await;
                }
            }))
        } else {
            None
        };

        // Wait for Ctrl+C
        tokio::signal::ctrl_c().await?;
        tracing::info!("Shutdown signal received");
        println!("\nShutting down...");

        routing_loop.abort();
        cron_loop.abort();
        if let Some(handle) = tool_msg_loop {
            handle.abort();
        }
        for ch in channels {
            ch.handle.abort();
            tracing::info!(channel = %ch.name, "Channel stopped");
        }

        Ok(())
    }
}

/// Route an outbound message to the correct channel
async fn route_outbound(
    outbound: OutboundMessage,
    senders: &[(String, mpsc::Sender<OutboundMessage>)],
) {
    let channel_name = outbound.channel.clone();
    let mut routed = false;
    for (name, tx) in senders {
        if *name == channel_name {
            if let Err(e) = tx.send(outbound).await {
                tracing::error!(
                    channel = %name,
                    error = %e,
                    "Failed to route outbound message"
                );
            } else {
                tracing::info!(channel = %name, "Outbound message routed");
            }
            routed = true;
            break;
        }
    }
    if !routed {
        tracing::error!(channel = %channel_name, "No sender found for channel");
    }
}
