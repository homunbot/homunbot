use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::web::auth::{require_write, AuthUser};
use crate::web::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/v1/workflows",
            get(list_workflows_api).post(create_workflow_api),
        )
        .route("/v1/workflows/{id}", get(get_workflow_api))
        .route(
            "/v1/workflows/{id}/approve",
            axum::routing::post(approve_workflow_api),
        )
        .route(
            "/v1/workflows/{id}/cancel",
            axum::routing::post(cancel_workflow_api),
        )
        .route(
            "/v1/workflows/{id}/delete",
            axum::routing::post(delete_workflow_api),
        )
        .route(
            "/v1/workflows/{id}/restart",
            axum::routing::post(restart_workflow_api),
        )
}

// --- Types ---

#[derive(Deserialize)]
struct WorkflowListQuery {
    status: Option<String>,
}

// --- Handlers ---

/// GET /api/v1/workflows?status=running
async fn list_workflows_api(
    Query(q): Query<WorkflowListQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let engine = state.workflow_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Workflow engine not available".into(),
    ))?;
    let workflows = engine
        .list(q.status.as_deref())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total = workflows.len();
    let running = workflows
        .iter()
        .filter(|w| w.status == crate::workflows::WorkflowStatus::Running)
        .count();
    let paused = workflows
        .iter()
        .filter(|w| w.status == crate::workflows::WorkflowStatus::Paused)
        .count();
    let completed = workflows
        .iter()
        .filter(|w| w.status == crate::workflows::WorkflowStatus::Completed)
        .count();
    let failed = workflows
        .iter()
        .filter(|w| w.status == crate::workflows::WorkflowStatus::Failed)
        .count();

    Ok(Json(serde_json::json!({
        "workflows": workflows,
        "stats": {
            "total": total,
            "running": running,
            "paused": paused,
            "completed": completed,
            "failed": failed,
        }
    })))
}

/// POST /api/v1/workflows
async fn create_workflow_api(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Json(req): Json<crate::workflows::WorkflowCreateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    require_write(&auth).map_err(|(s, j)| (s, j.0.to_string()))?;
    let engine = state.workflow_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Workflow engine not available".into(),
    ))?;
    let workflow_id = engine
        .create_and_start(req, "web", "web")
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(serde_json::json!({ "workflow_id": workflow_id })))
}

/// GET /api/v1/workflows/{id}
async fn get_workflow_api(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let engine = state.workflow_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Workflow engine not available".into(),
    ))?;
    let workflow = engine
        .status(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, format!("Workflow {id} not found")))?;
    Ok(Json(serde_json::to_value(&workflow).unwrap_or_default()))
}

/// POST /api/v1/workflows/{id}/approve
async fn approve_workflow_api(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    require_write(&auth).map_err(|(s, j)| (s, j.0.to_string()))?;
    let engine = state.workflow_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Workflow engine not available".into(),
    ))?;
    let msg = engine
        .approve_and_resume(&id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(serde_json::json!({ "message": msg })))
}

/// POST /api/v1/workflows/{id}/cancel
async fn cancel_workflow_api(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    require_write(&auth).map_err(|(s, j)| (s, j.0.to_string()))?;
    let engine = state.workflow_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Workflow engine not available".into(),
    ))?;
    let msg = engine
        .cancel(&id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(serde_json::json!({ "message": msg })))
}

/// POST /api/v1/workflows/{id}/delete
async fn delete_workflow_api(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    require_write(&auth).map_err(|(s, j)| (s, j.0.to_string()))?;
    let engine = state.workflow_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Workflow engine not available".into(),
    ))?;
    let msg = engine
        .delete(&id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(serde_json::json!({ "message": msg })))
}

/// POST /api/v1/workflows/{id}/restart
async fn restart_workflow_api(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    require_write(&auth).map_err(|(s, j)| (s, j.0.to_string()))?;
    let engine = state.workflow_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Workflow engine not available".into(),
    ))?;
    let msg = engine
        .restart(&id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(serde_json::json!({ "message": msg })))
}
