use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::bus::InboundMessage;

use super::server::AppState;

/// A stream event delivered to an individual WebSocket connection.
/// Carries either a text delta (normal streaming) or a tool-call event.
#[derive(Debug)]
pub struct WsStreamEvent {
    pub delta: String,
    pub event_type: Option<String>,
    /// Tool call details for tool_start events
    pub tool_call_data: Option<crate::provider::ToolCallData>,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/ws/chat", get(ws_handler))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Stable session — all Web UI tabs share one conversation in the DB.
    // This ensures messages accumulate for memory consolidation to trigger.
    // (WebSocket routing still works: each connection gets its own response_tx/stream_tx.)
    let chat_id = "default".to_string();

    // Channel for sending full responses back to this WebSocket
    let (response_tx, mut response_rx) = mpsc::channel::<String>(32);

    // Channel for streaming text chunks and tool events (real-time delivery)
    let (stream_tx, mut stream_rx) = mpsc::channel::<WsStreamEvent>(128);

    // Register this session for both full responses and streaming
    {
        let mut sessions = state.ws_sessions.write().await;
        sessions.insert(chat_id.clone(), response_tx);
    }
    {
        let mut streams = state.stream_sessions.write().await;
        streams.insert(chat_id.clone(), stream_tx);
    }

    tracing::info!(session = %chat_id, "WebSocket client connected");

    // Send welcome message
    let welcome = serde_json::json!({
        "type": "connected",
        "session_id": &chat_id,
    });
    let _ = sender
        .send(Message::Text(welcome.to_string().into()))
        .await;

    // Task: forward both full responses and stream chunks to WebSocket.
    // Stream chunks arrive as `type: "stream"` messages.
    // Full responses arrive as `type: "response"` messages.
    let chat_id_for_forward = chat_id.clone();
    let forward_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(msg) = response_rx.recv() => {
                    let payload = serde_json::json!({
                        "type": "response",
                        "content": msg,
                    });
                    if sender
                        .send(Message::Text(payload.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Some(event) = stream_rx.recv() => {
                    let payload = if let Some(ref evt) = event.event_type {
                        // Tool event: tool_start or tool_end
                        if evt == "tool_start" {
                            // Include tool call data for tool_start events
                            serde_json::json!({
                                "type": evt,
                                "name": event.delta,
                                "tool_call": event.tool_call_data,
                            })
                        } else {
                            serde_json::json!({
                                "type": evt,
                                "name": event.delta,
                            })
                        }
                    } else {
                        // Regular text streaming chunk
                        serde_json::json!({
                            "type": "stream",
                            "delta": event.delta,
                        })
                    };
                    if sender
                        .send(Message::Text(payload.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                else => break,
            }
        }
        tracing::info!(session = %chat_id_for_forward, "WebSocket forward task ended");
    });

    // Main loop: receive messages from WebSocket, send to agent
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => {
                let text = text.to_string();
                // Parse JSON message from client
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(content) = parsed.get("content").and_then(|v| v.as_str()) {
                        let inbound = InboundMessage {
                            channel: "web".to_string(),
                            sender_id: chat_id.clone(),
                            chat_id: chat_id.clone(),
                            content: content.to_string(),
                            timestamp: Utc::now(),
                        };

                        // Only send if agent is available
                        if let Some(ref tx) = state.inbound_tx {
                            if let Err(e) = tx.send(inbound).await {
                                tracing::error!(error = %e, "Failed to send WebSocket message to agent");
                                break;
                            }
                        } else {
                            tracing::warn!("No agent available. Configure a provider first.");
                            break;
                        }
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    // Cleanup
    {
        let mut sessions = state.ws_sessions.write().await;
        sessions.remove(&chat_id);
    }
    {
        let mut streams = state.stream_sessions.write().await;
        streams.remove(&chat_id);
    }

    forward_task.abort();
    tracing::info!(session = %chat_id, "WebSocket client disconnected");
}
