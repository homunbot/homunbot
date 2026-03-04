use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::bus::InboundMessage;

/// Heartbeat service — periodically sends a proactive wake-up message
/// to the agent, enabling background tasks like memory consolidation,
/// daily briefings, or self-initiated actions.
///
/// Following nanobot's heartbeat pattern:
/// - Fires at a configurable interval (default: 1 hour)
/// - Sends an internal "heartbeat" message through the inbound channel
/// - The agent processes it like any other message (can trigger tools, skills, etc.)
/// - The heartbeat prompt encourages the agent to check pending tasks
pub struct HeartbeatService {
    interval_secs: u64,
    inbound_tx: mpsc::Sender<InboundMessage>,
}

impl HeartbeatService {
    pub fn new(interval_secs: u64, inbound_tx: mpsc::Sender<InboundMessage>) -> Self {
        Self {
            interval_secs,
            inbound_tx,
        }
    }

    /// Start the heartbeat as a background task.
    /// Returns a JoinHandle for the spawned task.
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            service.run_loop().await;
        })
    }

    async fn run_loop(&self) {
        let interval = Duration::from_secs(self.interval_secs);

        // Wait one full interval before first heartbeat (don't fire immediately)
        tokio::time::sleep(interval).await;

        let mut ticker = tokio::time::interval(interval);
        ticker.tick().await; // consume the first immediate tick

        loop {
            ticker.tick().await;

            tracing::debug!("Heartbeat firing");

            let message = InboundMessage {
                channel: "heartbeat".to_string(),
                chat_id: "system".to_string(),
                content: HEARTBEAT_PROMPT.to_string(),
                sender_id: "system".to_string(),
                timestamp: chrono::Utc::now(),
                metadata: None,
            };

            if let Err(e) = self.inbound_tx.send(message).await {
                tracing::error!(error = %e, "Failed to send heartbeat message");
                break; // Channel closed, stop the heartbeat
            }
        }
    }
}

/// The prompt sent to the agent on each heartbeat.
/// Designed to trigger proactive behaviors without being intrusive.
const HEARTBEAT_PROMPT: &str = "\
[SYSTEM HEARTBEAT] This is an automated periodic check-in. \
Review any pending tasks, scheduled reminders that are due, \
or maintenance actions needed. If there's nothing to do, \
respond briefly with 'Heartbeat OK - no pending actions.' \
Do NOT send this response to the user unless there's something actionable.";

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_heartbeat_sends_message() {
        let (tx, mut rx) = mpsc::channel::<InboundMessage>(10);

        // Create a heartbeat with 1 second interval
        let service = Arc::new(HeartbeatService::new(1, tx));
        let handle = service.start();

        // Wait for the heartbeat to fire (1 sec delay + 1 sec interval)
        tokio::time::sleep(Duration::from_millis(2500)).await;

        // Should have received at least one heartbeat message
        let msg = rx.try_recv();
        assert!(msg.is_ok(), "Expected heartbeat message");
        let msg = msg.unwrap();
        assert_eq!(msg.channel, "heartbeat");
        assert_eq!(msg.sender_id, "system");
        assert!(msg.content.contains("HEARTBEAT"));

        handle.abort();
    }
}
