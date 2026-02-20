use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::bus::InboundMessage;

use super::server::AppState;

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

    // Unique session for this WebSocket connection
    let session_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let chat_id = format!("web-{session_id}");

    // Channel for sending responses back to this WebSocket
    let (response_tx, mut response_rx) = mpsc::channel::<String>(32);

    // Register this session
    {
        let mut sessions = state.ws_sessions.write().await;
        sessions.insert(chat_id.clone(), response_tx);
    }

    tracing::info!(session = %chat_id, "WebSocket client connected");

    // Send welcome message
    let welcome = serde_json::json!({
        "type": "connected",
        "session_id": session_id,
    });
    let _ = sender
        .send(Message::Text(welcome.to_string().into()))
        .await;

    // Task: forward agent responses to WebSocket
    let chat_id_for_forward = chat_id.clone();
    let forward_task = tokio::spawn(async move {
        while let Some(msg) = response_rx.recv().await {
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

    forward_task.abort();
    tracing::info!(session = %chat_id, "WebSocket client disconnected");
}
