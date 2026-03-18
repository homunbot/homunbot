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
        .route(
            "/v1/health/components",
            axum::routing::get(components_health),
        )
        .route(
            "/v1/channels/health",
            axum::routing::get(channels_health),
        )
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

// --- Detailed component health (authenticated) ---

#[derive(Serialize)]
struct ComponentsHealthResponse {
    status: &'static str,
    version: &'static str,
    uptime_secs: u64,
    components: Vec<ComponentHealth>,
}

#[derive(Serialize)]
struct ComponentHealth {
    name: &'static str,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}

impl ComponentHealth {
    fn healthy(name: &'static str) -> Self {
        Self {
            name,
            status: "healthy",
            message: None,
            details: None,
        }
    }
    fn degraded(name: &'static str, msg: impl Into<String>) -> Self {
        Self {
            name,
            status: "degraded",
            message: Some(msg.into()),
            details: None,
        }
    }
    fn unhealthy(name: &'static str, msg: impl Into<String>) -> Self {
        Self {
            name,
            status: "unhealthy",
            message: Some(msg.into()),
            details: None,
        }
    }
    fn unchecked(name: &'static str) -> Self {
        Self {
            name,
            status: "unchecked",
            message: None,
            details: None,
        }
    }
    fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

/// GET /api/v1/health/components — detailed health for every subsystem.
async fn components_health(State(state): State<Arc<AppState>>) -> Json<ComponentsHealthResponse> {
    let mut components = Vec::with_capacity(6);

    // 1. Database
    components.push(check_database(&state).await);

    // 2. LLM providers
    components.push(check_providers(&state));

    // 3. Channels
    components.push(check_channels(&state).await);

    // 4. Tools
    components.push(check_tools(&state).await);

    // 5. Knowledge (RAG) — only with embeddings feature
    #[cfg(feature = "embeddings")]
    components.push(check_knowledge(&state).await);

    // 6. Data directory
    components.push(check_data_dir());

    // Derive overall status from worst component
    let overall = if components.iter().any(|c| c.status == "unhealthy") {
        "unhealthy"
    } else if components.iter().any(|c| c.status == "degraded") {
        "degraded"
    } else {
        "healthy"
    };

    Json(ComponentsHealthResponse {
        status: overall,
        version: env!("CARGO_PKG_VERSION"),
        uptime_secs: state.started_at.elapsed().as_secs(),
        components,
    })
}

async fn check_database(state: &AppState) -> ComponentHealth {
    let Some(db) = &state.db else {
        return ComponentHealth::unchecked("database");
    };
    match sqlx::query("SELECT 1").execute(db.pool()).await {
        Ok(_) => ComponentHealth::healthy("database"),
        Err(e) => ComponentHealth::unhealthy("database", format!("Query failed: {e}")),
    }
}

fn check_providers(state: &AppState) -> ComponentHealth {
    let Some(tracker) = &state.health_tracker else {
        return ComponentHealth::unchecked("llm_providers");
    };
    let snapshots = tracker.snapshots();
    if snapshots.is_empty() {
        return ComponentHealth::degraded("llm_providers", "No providers registered");
    }

    use crate::provider::health::ProviderStatus;
    let down_count = snapshots
        .iter()
        .filter(|s| s.status == ProviderStatus::Down)
        .count();
    let total = snapshots.len();
    let details = serde_json::json!(snapshots
        .iter()
        .map(|s| serde_json::json!({
            "name": s.name,
            "status": s.status,
            "error_rate": format!("{:.0}%", s.error_rate_recent * 100.0),
            "avg_latency_ms": format!("{:.0}", s.avg_latency_ms),
        }))
        .collect::<Vec<_>>());

    let comp = if down_count == total {
        ComponentHealth::unhealthy("llm_providers", format!("All {total} providers down"))
    } else if down_count > 0 {
        ComponentHealth::degraded(
            "llm_providers",
            format!("{down_count}/{total} providers down"),
        )
    } else {
        ComponentHealth::healthy("llm_providers")
    };
    comp.with_details(details)
}

async fn check_channels(state: &AppState) -> ComponentHealth {
    use crate::channels::health::ChannelStatus;

    // If we have runtime health data, use it; otherwise fall back to config check.
    if let Some(tracker) = &state.channel_health {
        let snaps = tracker.snapshots();
        if snaps.is_empty() {
            return ComponentHealth::degraded("channels", "No channels running")
                .with_details(serde_json::json!({ "channels": [] }));
        }
        let down_count = snaps
            .iter()
            .filter(|s| s.status == ChannelStatus::Down || s.status == ChannelStatus::Stopped)
            .filter(|s| s.enabled)
            .count();
        let total_enabled = snaps.iter().filter(|s| s.enabled).count();

        let details = serde_json::json!({ "channels": snaps });
        if down_count == total_enabled {
            ComponentHealth::degraded("channels", "All channels down").with_details(details)
        } else if down_count > 0 {
            ComponentHealth::degraded("channels", &format!("{down_count} channel(s) down"))
                .with_details(details)
        } else {
            ComponentHealth::healthy("channels").with_details(details)
        }
    } else {
        // Fallback: config-only check (no gateway running)
        let config = state.config.read().await;
        let ch = &config.channels;
        let mut enabled = Vec::new();
        if ch.telegram.enabled { enabled.push("telegram"); }
        if ch.discord.enabled { enabled.push("discord"); }
        if ch.slack.enabled { enabled.push("slack"); }
        if ch.whatsapp.enabled { enabled.push("whatsapp"); }
        if ch.web.enabled { enabled.push("web"); }
        if ch.email.enabled || !ch.active_email_accounts().is_empty() { enabled.push("email"); }

        let details = serde_json::json!({ "enabled": enabled });
        if enabled.is_empty() {
            ComponentHealth::degraded("channels", "No channels enabled").with_details(details)
        } else {
            ComponentHealth::healthy("channels").with_details(details)
        }
    }
}

/// Per-channel runtime health data.
async fn channels_health(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let channels = match &state.channel_health {
        Some(tracker) => serde_json::to_value(tracker.snapshots()).unwrap_or_default(),
        None => serde_json::json!([]),
    };
    Json(serde_json::json!({ "channels": channels }))
}

async fn check_tools(state: &AppState) -> ComponentHealth {
    let Some(registry) = &state.tool_registry else {
        return ComponentHealth::unchecked("tools");
    };
    let count = registry.read().await.len();
    ComponentHealth::healthy("tools").with_details(serde_json::json!({ "count": count }))
}

#[cfg(feature = "embeddings")]
async fn check_knowledge(state: &AppState) -> ComponentHealth {
    let Some(engine) = &state.rag_engine else {
        return ComponentHealth::unchecked("knowledge");
    };
    match engine.lock().await.stats().await {
        Ok(stats) => ComponentHealth::healthy("knowledge").with_details(serde_json::json!({
            "sources": stats.source_count,
            "chunks": stats.chunk_count,
            "vectors": stats.index_vectors,
        })),
        Err(e) => ComponentHealth::unhealthy("knowledge", format!("Stats failed: {e}")),
    }
}

fn check_data_dir() -> ComponentHealth {
    let data_dir = dirs::home_dir()
        .map(|h| h.join(".homun"))
        .unwrap_or_default();
    if !data_dir.exists() {
        return ComponentHealth::unhealthy("data_dir", "Data directory does not exist");
    }
    // Verify writable by checking metadata
    match std::fs::metadata(&data_dir) {
        Ok(meta) if meta.is_dir() => ComponentHealth::healthy("data_dir")
            .with_details(serde_json::json!({ "path": data_dir.display().to_string() })),
        Ok(_) => ComponentHealth::unhealthy("data_dir", "Path exists but is not a directory"),
        Err(e) => ComponentHealth::unhealthy("data_dir", format!("Cannot access: {e}")),
    }
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
