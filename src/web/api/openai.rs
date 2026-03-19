//! OpenAI-compatible Chat Completions API (API-1).
//!
//! `POST /v1/chat/completions` — accepts OpenAI-format requests, routes through
//! the Homun agent loop (with tools, memory, skills), and returns OpenAI-format
//! responses (both streaming SSE and non-streaming JSON).
//!
//! Auth: Bearer token (webhook tokens with scope "chat" or "admin").

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::bus::InboundMessage;
use crate::web::auth::AuthUser;
use crate::web::server::AppState;
use crate::web::ws::WsStreamEvent;

/// Register OpenAI-compatible routes under `/v1/`.
pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/v1/chat/completions", post(chat_completions))
}

// ─── Request / Response Types ──────────────────────────────────

#[derive(Debug, Deserialize)]
struct CompletionRequest {
    #[serde(default)]
    model: String,
    messages: Vec<OaiMessage>,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    max_tokens: Option<u32>,
    #[serde(default)]
    temperature: Option<f32>,
    /// Custom extension: session ID for multi-turn conversations.
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OaiMessage {
    role: String,
    #[serde(default)]
    content: String,
}

#[derive(Serialize)]
struct CompletionResponse {
    id: String,
    object: &'static str,
    created: i64,
    model: String,
    choices: Vec<Choice>,
    usage: OaiUsage,
}

#[derive(Serialize)]
struct Choice {
    index: u32,
    message: OaiResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Serialize)]
struct OaiResponseMessage {
    role: &'static str,
    content: String,
}

#[derive(Serialize)]
struct OaiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Serialize)]
struct ChunkResponse {
    id: String,
    object: &'static str,
    created: i64,
    model: String,
    choices: Vec<ChunkChoice>,
}

#[derive(Serialize)]
struct ChunkChoice {
    index: u32,
    delta: ChunkDelta,
    finish_reason: Option<String>,
}

#[derive(Serialize)]
struct ChunkDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

// ─── Handler ───────────────────────────────────────────────────

/// POST /api/v1/chat/completions
async fn chat_completions(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Json(body): Json<CompletionRequest>,
) -> Response {
    // Validate: need at least one message
    if body.messages.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": {"message": "messages array is required", "type": "invalid_request_error"}})),
        ).into_response();
    }

    // Need agent available
    let inbound_tx = match &state.inbound_tx {
        Some(tx) => tx.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": {"message": "No agent available. Configure a provider first.", "type": "server_error"}})),
            ).into_response();
        }
    };

    // Session routing: use provided session_id or generate one
    let session_id = body
        .session_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let chat_id = format!("api-{session_id}");

    // Extract the last user message content
    let user_content = body
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.clone())
        .unwrap_or_default();

    if user_content.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": {"message": "No user message found in messages array", "type": "invalid_request_error"}})),
        ).into_response();
    }

    // Model name for responses (use configured model if not specified)
    let model_name = if body.model.is_empty() {
        state
            .config
            .read()
            .await
            .agent
            .model
            .clone()
    } else {
        body.model.clone()
    };

    let completion_id = format!("chatcmpl-{}", uuid::Uuid::new_v4().to_string().replace('-', "")[..24].to_string());
    let created = chrono::Utc::now().timestamp();

    if body.stream {
        handle_streaming(
            state, inbound_tx, chat_id, session_id, user_content,
            model_name, completion_id, created, auth,
        ).await
    } else {
        handle_non_streaming(
            state, inbound_tx, chat_id, session_id, user_content,
            model_name, completion_id, created, auth,
        ).await
    }
}

// ─── Non-Streaming ─────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn handle_non_streaming(
    state: Arc<AppState>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    chat_id: String,
    session_id: String,
    user_content: String,
    model_name: String,
    completion_id: String,
    created: i64,
    _auth: AuthUser,
) -> Response {
    // Register for full response
    let (response_tx, mut response_rx) = mpsc::channel::<String>(1);
    {
        let mut sessions = state.ws_sessions.write().await;
        sessions.insert(chat_id.clone(), response_tx);
    }

    // Send inbound message
    let inbound = InboundMessage {
        channel: "api".to_string(),
        sender_id: session_id.clone(),
        chat_id: chat_id.clone(),
        content: user_content,
        timestamp: chrono::Utc::now(),
        metadata: None,
    };

    if let Err(e) = inbound_tx.send(inbound).await {
        cleanup_sessions(&state, &chat_id).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"message": format!("Failed to send message: {e}"), "type": "server_error"}})),
        ).into_response();
    }

    // Wait for response (timeout 120s)
    let response_content = tokio::time::timeout(Duration::from_secs(120), response_rx.recv()).await;
    cleanup_sessions(&state, &chat_id).await;

    let content = match response_content {
        Ok(Some(c)) => c,
        Ok(None) => String::from("(no response)"),
        Err(_) => {
            return (
                StatusCode::GATEWAY_TIMEOUT,
                Json(serde_json::json!({"error": {"message": "Request timed out", "type": "server_error"}})),
            ).into_response();
        }
    };

    let approx_tokens = (content.len() / 4) as u32;

    Json(CompletionResponse {
        id: completion_id,
        object: "chat.completion",
        created,
        model: model_name,
        choices: vec![Choice {
            index: 0,
            message: OaiResponseMessage {
                role: "assistant",
                content,
            },
            finish_reason: Some("stop".to_string()),
        }],
        usage: OaiUsage {
            prompt_tokens: 0,
            completion_tokens: approx_tokens,
            total_tokens: approx_tokens,
        },
    })
    .into_response()
}

// ─── Streaming (SSE) ───────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn handle_streaming(
    state: Arc<AppState>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    chat_id: String,
    session_id: String,
    user_content: String,
    model_name: String,
    completion_id: String,
    created: i64,
    _auth: AuthUser,
) -> Response {
    // Register for streaming chunks
    let (stream_tx, stream_rx) = mpsc::channel::<WsStreamEvent>(64);
    // Also register for final response (to know when done)
    let (response_tx, response_rx) = mpsc::channel::<String>(1);
    {
        let mut streams = state.stream_sessions.write().await;
        streams.insert(chat_id.clone(), stream_tx);
    }
    {
        let mut sessions = state.ws_sessions.write().await;
        sessions.insert(chat_id.clone(), response_tx);
    }

    // Send inbound message
    let inbound = InboundMessage {
        channel: "api".to_string(),
        sender_id: session_id.clone(),
        chat_id: chat_id.clone(),
        content: user_content,
        timestamp: chrono::Utc::now(),
        metadata: None,
    };

    if let Err(e) = inbound_tx.send(inbound).await {
        cleanup_sessions(&state, &chat_id).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": {"message": format!("Failed to send message: {e}"), "type": "server_error"}})),
        ).into_response();
    }

    // Build SSE stream
    let state_cleanup = state.clone();
    let chat_id_cleanup = chat_id.clone();

    let sse_stream = build_sse_stream(
        stream_rx, response_rx, completion_id, model_name, created,
    );

    // Spawn cleanup after stream ends
    let state_c2 = state_cleanup.clone();
    let cid = chat_id_cleanup.clone();
    tokio::spawn(async move {
        // Give the stream a moment to fully drain
        tokio::time::sleep(Duration::from_secs(2)).await;
        cleanup_sessions(&state_c2, &cid).await;
    });

    Sse::new(sse_stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text(""))
        .into_response()
}

/// Build the SSE event stream from streaming chunks.
fn build_sse_stream(
    stream_rx: mpsc::Receiver<WsStreamEvent>,
    response_rx: mpsc::Receiver<String>,
    completion_id: String,
    model_name: String,
    created: i64,
) -> impl futures::Stream<Item = Result<Event, Infallible>> {
    let sent_role = false;

    futures::stream::unfold(
        (stream_rx, response_rx, false, sent_role),
        move |(mut srx, mut rrx, done, sr)| {
            let cid = completion_id.clone();
            let model = model_name.clone();

            async move {
                if done {
                    return None;
                }

                tokio::select! {
                    chunk = srx.recv() => {
                        match chunk {
                            Some(event) => {
                                // Skip non-content events (tool calls, errors, plans)
                                if event.event_type.is_some() {
                                    return Some((Ok(Event::default().comment("")), (srx, rrx, false, true)));
                                }

                                if event.delta.is_empty() {
                                    return Some((Ok(Event::default().comment("")), (srx, rrx, false, true)));
                                }

                                let chunk_resp = ChunkResponse {
                                    id: cid,
                                    object: "chat.completion.chunk",
                                    created,
                                    model,
                                    choices: vec![ChunkChoice {
                                        index: 0,
                                        delta: ChunkDelta {
                                            role: if !sr { Some("assistant") } else { None },
                                            content: Some(event.delta),
                                        },
                                        finish_reason: None,
                                    }],
                                };

                                let data = serde_json::to_string(&chunk_resp).unwrap_or_default();
                                let sse_event = Event::default().data(data);
                                // After first content chunk, don't send role again
                                Some((Ok(sse_event), (srx, rrx, false, true)))
                            }
                            None => {
                                // Stream channel closed
                                let done_event = Event::default().data("[DONE]");
                                Some((Ok(done_event), (srx, rrx, true, true)))
                            }
                        }
                    }
                    _response = rrx.recv() => {
                        // Final response received — send finish chunk + [DONE]
                        let finish_resp = ChunkResponse {
                            id: cid,
                            object: "chat.completion.chunk",
                            created,
                            model,
                            choices: vec![ChunkChoice {
                                index: 0,
                                delta: ChunkDelta { role: None, content: None },
                                finish_reason: Some("stop".to_string()),
                            }],
                        };
                        let data = serde_json::to_string(&finish_resp).unwrap_or_default();
                        let sse_event = Event::default().data(format!("{data}\n\ndata: [DONE]"));
                        Some((Ok(sse_event), (srx, rrx, true, true)))
                    }
                }
            }
        },
    )
}

/// Remove session registrations after request completes.
async fn cleanup_sessions(state: &AppState, chat_id: &str) {
    {
        let mut sessions = state.ws_sessions.write().await;
        sessions.remove(chat_id);
    }
    {
        let mut streams = state.stream_sessions.write().await;
        streams.remove(chat_id);
    }
}
