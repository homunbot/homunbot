use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::Router;
use serde::{Deserialize, Serialize};

use super::super::server::AppState;

/// Routes registered inside the authenticated API router.
/// Note: `health` and `webhook_ingress` are NOT here — they are public routes
/// registered directly in server.rs.
pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/v1/emergency-stop",
            axum::routing::post(emergency_stop_handler),
        )
        .route("/v1/resume", axum::routing::post(resume_handler))
}

// --- Health check (public) ---

#[derive(Serialize)]
pub(crate) struct HealthResponse {
    status: &'static str,
    version: &'static str,
    uptime_secs: u64,
}

pub async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        uptime_secs: state.started_at.elapsed().as_secs(),
    })
}

// --- Emergency Stop ---

async fn emergency_stop_handler(
    State(state): State<Arc<AppState>>,
) -> Json<crate::security::EStopReport> {
    let report = crate::security::emergency_stop(&state.estop_handles).await;
    Json(report)
}

async fn resume_handler(State(_state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    crate::security::resume();
    Json(serde_json::json!({ "status": "resumed", "network": "online" }))
}

// --- Webhook Ingress (public) ---

/// Request body for webhook ingress
#[derive(Debug, Deserialize)]
pub(crate) struct WebhookRequest {
    /// The message to send to the agent
    message: String,
    /// Optional: conversation ID for threading (defaults to "webhook")
    #[serde(default)]
    conversation_id: Option<String>,
}

/// Response for webhook ingress
#[derive(Debug, Serialize)]
pub(crate) struct WebhookResponse {
    status: &'static str,
    user: String,
    conversation_id: String,
}

/// Error response for webhook ingress
#[derive(Debug, Serialize)]
pub(crate) struct WebhookError {
    error: &'static str,
    message: String,
}

/// Handle incoming webhook requests.
///
/// Validates the webhook token, resolves the user, and forwards the message
/// to the agent for processing.
///
/// POST /api/v1/webhook/{token}
/// Body: { "message": "...", "conversation_id": "optional" }
pub async fn webhook_ingress(
    Path(token): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<WebhookRequest>,
) -> Result<Json<WebhookResponse>, (StatusCode, Json<WebhookError>)> {
    // Validate token format (should start with "wh_")
    if !token.starts_with("wh_") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(WebhookError {
                error: "invalid_token",
                message: "Token must start with 'wh_'".to_string(),
            }),
        ));
    }

    // Look up the user by webhook token
    let db = match &state.db {
        Some(db) => db,
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(WebhookError {
                    error: "no_database",
                    message: "Database not available".to_string(),
                }),
            ));
        }
    };

    let user = db.lookup_user_by_webhook_token(&token).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(WebhookError {
                error: "database_error",
                message: e.to_string(),
            }),
        )
    })?;

    let user = match user {
        Some(u) => u,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(WebhookError {
                    error: "invalid_token",
                    message: "Unknown or disabled webhook token".to_string(),
                }),
            ));
        }
    };

    // Update token last_used
    let _ = db.touch_webhook_token(&token).await;

    // Build session key: webhook:{conversation_id}
    let conversation_id = body
        .conversation_id
        .unwrap_or_else(|| "default".to_string());
    let session_key = format!("webhook:{}", conversation_id);

    // Create inbound message
    let inbound = crate::bus::InboundMessage {
        channel: "webhook".to_string(),
        sender_id: user.id.clone(),
        chat_id: session_key.clone(),
        content: body.message,
        timestamp: chrono::Utc::now(),
        metadata: None,
    };

    // Send to agent
    let inbound_tx = match &state.inbound_tx {
        Some(tx) => tx,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(WebhookError {
                    error: "agent_not_running",
                    message: "Agent is not running. Start the gateway first.".to_string(),
                }),
            ));
        }
    };

    inbound_tx.send(inbound).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(WebhookError {
                error: "send_failed",
                message: format!("Failed to send message to agent: {}", e),
            }),
        )
    })?;

    tracing::info!(
        token = &token[..12.min(token.len())],
        user = %user.username,
        session = %session_key,
        "Webhook message received"
    );

    Ok(Json(WebhookResponse {
        status: "queued",
        user: user.username,
        conversation_id,
    }))
}
