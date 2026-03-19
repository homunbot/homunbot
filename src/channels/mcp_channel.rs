//! MCP-based messaging channel.
//!
//! Bridges an MCP server into the gateway pipeline, allowing external
//! servers to act as messaging channels. The MCP server must expose:
//! - A `receive_messages` resource or notification for inbound messages
//! - A `send_message` tool for outbound messages
//!
//! Channel name format: `mcp:<config_name>` (e.g. "mcp:sms")

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::bus::{InboundMessage, OutboundMessage};
use crate::channels::traits::Channel;
use crate::config::McpChannelConfig;

/// MCP-based channel that bridges an external MCP server to the gateway.
pub struct McpChannel {
    name: String,
    config: McpChannelConfig,
}

impl McpChannel {
    pub fn new(name: String, config: McpChannelConfig) -> Self {
        Self { name, config }
    }
}

#[async_trait]
impl Channel for McpChannel {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(
        &self,
        _inbound_tx: mpsc::Sender<InboundMessage>,
        _outbound_rx: mpsc::Receiver<OutboundMessage>,
    ) -> Result<()> {
        if self.config.server.is_empty() {
            tracing::warn!(
                channel = %self.name,
                "MCP channel has no server configured — skipping"
            );
            return Ok(());
        }

        tracing::info!(
            channel = %self.name,
            server = %self.config.server,
            "MCP channel registered (awaiting MCP messaging protocol support)"
        );

        // TODO: When MCP defines a messaging protocol:
        // 1. Connect to the MCP server (by name from MCP registry)
        // 2. Subscribe to inbound message notifications
        // 3. Route outbound messages via MCP tool calls
        //
        // For now, MCP channels are registered in the gateway's auth pipeline
        // (allow_from, pairing) but don't actively send/receive messages.
        // They will be activated when the MCP messaging spec is finalized.

        // Keep the channel alive (don't exit immediately)
        tokio::signal::ctrl_c().await.ok();
        Ok(())
    }
}
