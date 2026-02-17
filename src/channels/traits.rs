use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::bus::{InboundMessage, OutboundMessage};

/// Channel trait — abstraction for all messaging interfaces.
///
/// Each channel implementation:
/// 1. Listens for incoming messages on its platform
/// 2. Converts them to InboundMessage and sends to the bus
/// 3. Receives OutboundMessage from the bus and delivers to the platform
///
/// Follows nanobot's BaseChannel pattern.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Start listening for messages. Runs until cancelled.
    /// - `inbound_tx`: send received messages to the agent
    /// - `outbound_rx`: receive responses to send back
    async fn start(
        &self,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: mpsc::Receiver<OutboundMessage>,
    ) -> Result<()>;

    /// Channel name (e.g., "telegram", "whatsapp", "cli")
    fn name(&self) -> &str;
}
