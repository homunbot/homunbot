use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::bus::{InboundMessage, MessageMetadata};

use super::server::AppState;

#[derive(Debug, Default, serde::Deserialize)]
struct WsChatQuery {
    conversation_id: Option<String>,
}

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

async fn persist_run_snapshot(state: &AppState, run: &super::run_state::WebChatRunSnapshot) {
    if let Some(db) = state.db.as_ref() {
        if let Err(error) = db.upsert_web_chat_run(run).await {
            tracing::error!(run_id = %run.run_id, %error, "Failed to persist web chat run");
        }
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsChatQuery>,
) -> impl IntoResponse {
    let conversation_id = query
        .conversation_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("default")
        .to_string();
    ws.on_upgrade(move |socket| handle_socket(socket, state, conversation_id))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>, conversation_id: String) {
    let (mut sender, mut receiver) = socket.split();

    let chat_id = conversation_id.clone();
    let session_key = format!("web:{conversation_id}");

    // Channel for sending full responses back to this WebSocket
    let (response_tx, mut response_rx) = mpsc::channel::<String>(32);

    // Channel for streaming text chunks and tool events (real-time delivery)
    let (stream_tx, mut stream_rx) = mpsc::channel::<WsStreamEvent>(128);
    let client_stream_tx = stream_tx.clone();

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
        "conversation_id": &conversation_id,
    });
    let _ = sender.send(Message::Text(welcome.to_string().into())).await;

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
                        } else if evt == "error" {
                            serde_json::json!({
                                "type": evt,
                                "message": event.delta,
                            })
                        } else if evt == "plan" {
                            serde_json::json!({
                                "type": evt,
                                "name": event.delta,
                            })
                        } else if evt == "workflow_progress" {
                            // delta contains JSON string of progress data
                            let progress: serde_json::Value = serde_json::from_str(&event.delta)
                                .unwrap_or_else(|_| serde_json::json!({}));
                            serde_json::json!({
                                "type": "workflow_progress",
                                "progress": progress,
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
                        let content = content.trim();
                        let attachments = parsed
                            .get("attachments")
                            .cloned()
                            .and_then(|value| {
                                serde_json::from_value::<
                                        Vec<super::chat_attachments::ChatAttachment>,
                                    >(value)
                                    .ok()
                            })
                            .unwrap_or_default();
                        let mcp_servers = parsed
                            .get("mcp_servers")
                            .cloned()
                            .and_then(|value| {
                                serde_json::from_value::<
                                    Vec<super::chat_attachments::ChatMcpServerRef>,
                                >(value)
                                .ok()
                            })
                            .unwrap_or_default();

                        if content.is_empty() && attachments.is_empty() && mcp_servers.is_empty() {
                            continue;
                        }

                        let stored_content = super::chat_attachments::encode_inline_context(
                            content,
                            &attachments,
                            &mcp_servers,
                        )
                        .unwrap_or_else(|| content.to_string());
                        let user_message_label = if !content.is_empty() {
                            content.to_string()
                        } else if let Some(attachment) = attachments.first() {
                            attachment.name.clone()
                        } else {
                            mcp_servers
                                .first()
                                .map(|server| server.name.clone())
                                .unwrap_or_default()
                        };

                        let run = match state.web_runs.start_run(&session_key, &user_message_label)
                        {
                            Ok(run) => run,
                            Err(message) => {
                                let _ = client_stream_tx
                                    .send(WsStreamEvent {
                                        delta: message,
                                        event_type: Some("error".to_string()),
                                        tool_call_data: None,
                                    })
                                    .await;
                                continue;
                            }
                        };
                        persist_run_snapshot(&state, &run).await;

                        let thinking_override = parsed
                            .get("thinking")
                            .and_then(|v| v.as_bool());

                        let inbound = InboundMessage {
                            channel: "web".to_string(),
                            sender_id: chat_id.clone(),
                            chat_id: chat_id.clone(),
                            content: stored_content,
                            timestamp: Utc::now(),
                            metadata: Some(MessageMetadata {
                                web_run_id: Some(run.run_id),
                                thinking_override,
                                ..MessageMetadata::default()
                            }),
                        };

                        // Only send if agent is available
                        if let Some(ref tx) = state.inbound_tx {
                            if let Err(e) = tx.send(inbound).await {
                                state.web_runs.clear_session(&session_key);
                                if let Some(db) = state.db.as_ref() {
                                    let _ = db.delete_web_chat_runs(&session_key).await;
                                }
                                tracing::error!(error = %e, "Failed to send WebSocket message to agent");
                                break;
                            }
                        } else {
                            state.web_runs.clear_session(&session_key);
                            if let Some(db) = state.db.as_ref() {
                                let _ = db.delete_web_chat_runs(&session_key).await;
                            }
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
