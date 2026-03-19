//! Session routing API (API-2).
//!
//! CRUD endpoints for API sessions — create, list, get history, delete.
//! Sessions created via the OpenAI-compat API use the `api:` prefix.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};

use crate::web::auth::AuthUser;
use crate::web::server::AppState;

/// Register session routes under `/v1/sessions`.
pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/sessions", get(list_sessions))
        .route("/v1/sessions", post(create_session))
        .route("/v1/sessions/{id}/messages", get(get_messages))
        .route("/v1/sessions/{id}", delete(delete_session))
}

/// GET /api/v1/sessions — list API sessions.
async fn list_sessions(
    State(state): State<Arc<AppState>>,
    axum::Extension(_auth): axum::Extension<AuthUser>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let db = state
        .db
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "DB unavailable".into()))?;

    // Load sessions with api: prefix
    let sessions = db
        .list_sessions_by_prefix("api:%", 100)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let items: Vec<serde_json::Value> = sessions
        .iter()
        .map(|s| {
            let session_id = s.key.strip_prefix("api:").unwrap_or(&s.key);
            serde_json::json!({
                "session_id": session_id,
                "created_at": s.created_at,
                "updated_at": s.updated_at,
                "message_count": s.message_count,
                "last_message_at": s.last_message_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "sessions": items })))
}

/// POST /api/v1/sessions — create a new API session.
async fn create_session(
    State(state): State<Arc<AppState>>,
    axum::Extension(_auth): axum::Extension<AuthUser>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let db = state
        .db
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "DB unavailable".into()))?;

    let session_id = uuid::Uuid::new_v4().to_string();
    let session_key = format!("api:{session_id}");

    db.upsert_session(&session_key, 0)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "session_key": session_key,
    })))
}

/// GET /api/v1/sessions/{id}/messages — get message history for a session.
async fn get_messages(
    State(state): State<Arc<AppState>>,
    axum::Extension(_auth): axum::Extension<AuthUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let db = state
        .db
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "DB unavailable".into()))?;

    let session_key = format!("api:{id}");
    let messages = db
        .load_messages(&session_key, 1000)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let items: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content,
                "timestamp": m.timestamp,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "session_id": id,
        "messages": items,
    })))
}

/// DELETE /api/v1/sessions/{id} — delete a session and its messages.
async fn delete_session(
    State(state): State<Arc<AppState>>,
    axum::Extension(_auth): axum::Extension<AuthUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let db = state
        .db
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "DB unavailable".into()))?;

    let session_key = format!("api:{id}");
    db.clear_messages(&session_key)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let _ = db.delete_session(&session_key).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}
