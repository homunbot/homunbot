use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Message from a channel to the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    pub channel: String,
    pub sender_id: String,
    pub chat_id: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

impl InboundMessage {
    pub fn session_key(&self) -> String {
        format!("{}:{}", self.channel, self.chat_id)
    }
}

/// Message from the agent to a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub channel: String,
    pub chat_id: String,
    pub content: String,
}

/// A streaming text chunk routed to a specific chat session.
/// Used by the gateway to push incremental LLM output to the web UI
/// via the WebSocket stream_sessions map.
#[derive(Debug, Clone)]
pub struct StreamMessage {
    pub chat_id: String,
    pub delta: String,
    pub done: bool,
    /// Optional event type for non-text chunks (e.g. "tool_start", "tool_end").
    pub event_type: Option<String>,
    /// Tool call details for tool_start events
    pub tool_call_data: Option<crate::provider::ToolCallData>,
}

/// Message bus — routes messages between channels and the agent loop
/// using tokio mpsc channels.
pub struct MessageBus {
    pub inbound_tx: mpsc::Sender<InboundMessage>,
    pub inbound_rx: mpsc::Receiver<InboundMessage>,
    pub outbound_tx: mpsc::Sender<OutboundMessage>,
    pub outbound_rx: mpsc::Receiver<OutboundMessage>,
}

impl MessageBus {
    pub fn new(buffer_size: usize) -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(buffer_size);
        let (outbound_tx, outbound_rx) = mpsc::channel(buffer_size);
        Self {
            inbound_tx,
            inbound_rx,
            outbound_tx,
            outbound_rx,
        }
    }
}
