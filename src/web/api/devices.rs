//! Trusted device management API (REM-3).
//!
//! Endpoints for listing, approving, and revoking trusted devices.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};

use crate::web::auth::AuthUser;
use crate::web::server::AppState;

/// Register device management routes under `/v1/devices`.
pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/devices", get(list_devices))
        .route("/v1/devices/{id}/approve", post(approve_device))
        .route("/v1/devices/{id}", delete(revoke_device))
}

/// GET /api/v1/devices — list trusted devices for the current user.
async fn list_devices(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let db = state
        .db
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "DB unavailable".into()))?;

    let devices = db
        .load_trusted_devices(&auth.user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "devices": devices })))
}

/// POST /api/v1/devices/{id}/approve — approve a pending device from an existing session.
async fn approve_device(
    State(state): State<Arc<AppState>>,
    axum::Extension(_auth): axum::Extension<AuthUser>,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let db = state
        .db
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "DB unavailable".into()))?;

    let approved = db
        .approve_trusted_device(&device_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if approved {
        tracing::info!(device_id = %device_id, "Device approved via authenticated session");
        Ok(Json(serde_json::json!({ "ok": true })))
    } else {
        Err((StatusCode::NOT_FOUND, "Device not found".into()))
    }
}

/// DELETE /api/v1/devices/{id} — revoke a trusted device.
async fn revoke_device(
    State(state): State<Arc<AppState>>,
    axum::Extension(_auth): axum::Extension<AuthUser>,
    Path(device_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let db = state
        .db
        .as_ref()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "DB unavailable".into()))?;

    let deleted = db
        .delete_trusted_device(&device_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if deleted {
        tracing::info!(device_id = %device_id, "Device revoked");
        Ok(Json(serde_json::json!({ "ok": true })))
    } else {
        Err((StatusCode::NOT_FOUND, "Device not found".into()))
    }
}
