use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use super::super::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/approvals", get(list_approvals))
        .route("/v1/approvals/pending", get(list_pending_approvals))
        .route("/v1/approvals/audit", get(get_approval_audit_log))
        .route(
            "/v1/approvals/{id}/approve",
            axum::routing::post(approve_request),
        )
        .route("/v1/approvals/{id}/deny", axum::routing::post(deny_request))
        .route(
            "/v1/approvals/config",
            get(get_approval_config).put(put_approval_config),
        )
}

/// Response for pending approvals list
#[derive(Debug, Serialize)]
struct PendingApprovalsResponse {
    pending: Vec<crate::tools::PendingApproval>,
    count: usize,
}

/// Response for approval audit log
#[derive(Debug, Serialize)]
struct ApprovalAuditResponse {
    log: Vec<crate::tools::ApprovalLogEntry>,
    count: usize,
}

/// Request for approving/denying
#[derive(Debug, Deserialize)]
struct ApprovalActionRequest {
    /// If true, add to session allowlist (approve all future same commands)
    #[serde(default)]
    always: bool,
}

/// Response for approval action
#[derive(Debug, Serialize)]
struct ApprovalActionResponse {
    success: bool,
    message: String,
}

/// Response for approval config
#[derive(Debug, Serialize)]
struct ApprovalConfigResponse {
    level: String,
    auto_approve: Vec<String>,
    always_ask: Vec<String>,
    pending_count: usize,
}

/// Request for updating approval config
#[derive(Debug, Deserialize)]
struct ApprovalConfigRequest {
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    auto_approve: Option<Vec<String>>,
    #[serde(default)]
    always_ask: Option<Vec<String>>,
}

/// List all approvals (pending + summary)
/// GET /api/v1/approvals
async fn list_approvals(State(_state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let manager = crate::tools::global_approval_manager();

    match manager {
        Some(m) => {
            let pending = m.get_pending();
            let audit = m.audit_log();
            Json(serde_json::json!({
                "pending": pending,
                "pending_count": pending.len(),
                "audit_count": audit.len(),
                "autonomy_level": format!("{:?}", m.autonomy_level()),
                "session_allowlist": m.session_allowlist().into_iter().collect::<Vec<_>>(),
            }))
        }
        None => Json(serde_json::json!({
            "error": "Approval manager not initialized",
            "pending": [],
            "pending_count": 0,
        })),
    }
}

/// List pending approvals
/// GET /api/v1/approvals/pending
async fn list_pending_approvals(
    State(_state): State<Arc<AppState>>,
) -> Json<PendingApprovalsResponse> {
    let manager = crate::tools::global_approval_manager();

    let pending = manager.map(|m| m.get_pending()).unwrap_or_default();
    let count = pending.len();

    Json(PendingApprovalsResponse { pending, count })
}

/// Get approval audit log
/// GET /api/v1/approvals/audit
async fn get_approval_audit_log(
    State(_state): State<Arc<AppState>>,
) -> Json<ApprovalAuditResponse> {
    let manager = crate::tools::global_approval_manager();

    let log = manager.map(|m| m.audit_log()).unwrap_or_default();
    let count = log.len();

    Json(ApprovalAuditResponse { log, count })
}

/// Approve a pending request
/// POST /api/v1/approvals/{id}/approve
async fn approve_request(
    Path(id): Path<String>,
    State(_state): State<Arc<AppState>>,
    Json(body): Json<ApprovalActionRequest>,
) -> Result<Json<ApprovalActionResponse>, (StatusCode, Json<ApprovalActionResponse>)> {
    let manager = crate::tools::global_approval_manager();

    match manager {
        Some(m) => match m.approve(&id, body.always) {
            Ok(()) => Ok(Json(ApprovalActionResponse {
                success: true,
                message: if body.always {
                    "Approved and added to session allowlist".to_string()
                } else {
                    "Approved for this session".to_string()
                },
            })),
            Err(e) => Err((
                StatusCode::NOT_FOUND,
                Json(ApprovalActionResponse {
                    success: false,
                    message: e,
                }),
            )),
        },
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApprovalActionResponse {
                success: false,
                message: "Approval manager not initialized".to_string(),
            }),
        )),
    }
}

/// Deny a pending request
/// POST /api/v1/approvals/{id}/deny
async fn deny_request(
    Path(id): Path<String>,
    State(_state): State<Arc<AppState>>,
) -> Result<Json<ApprovalActionResponse>, (StatusCode, Json<ApprovalActionResponse>)> {
    let manager = crate::tools::global_approval_manager();

    match manager {
        Some(m) => match m.deny(&id) {
            Ok(()) => Ok(Json(ApprovalActionResponse {
                success: true,
                message: "Request denied".to_string(),
            })),
            Err(e) => Err((
                StatusCode::NOT_FOUND,
                Json(ApprovalActionResponse {
                    success: false,
                    message: e,
                }),
            )),
        },
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApprovalActionResponse {
                success: false,
                message: "Approval manager not initialized".to_string(),
            }),
        )),
    }
}

/// Get approval configuration
/// GET /api/v1/approvals/config
async fn get_approval_config(State(state): State<Arc<AppState>>) -> Json<ApprovalConfigResponse> {
    let config = state.config.read().await;
    let manager = crate::tools::global_approval_manager();

    Json(ApprovalConfigResponse {
        level: format!("{:?}", config.permissions.approval.level).to_lowercase(),
        auto_approve: config.permissions.approval.auto_approve.clone(),
        always_ask: config.permissions.approval.always_ask.clone(),
        pending_count: manager.map(|m| m.get_pending().len()).unwrap_or(0),
    })
}

/// Update approval configuration
/// PUT /api/v1/approvals/config
async fn put_approval_config(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ApprovalConfigRequest>,
) -> Result<Json<ApprovalConfigResponse>, (StatusCode, String)> {
    use crate::config::AutonomyLevel;

    let mut config = state.config.write().await;

    if let Some(level) = body.level {
        let level = match level.to_lowercase().as_str() {
            "full" => AutonomyLevel::Full,
            "supervised" => AutonomyLevel::Supervised,
            "readonly" | "read_only" => AutonomyLevel::ReadOnly,
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("Invalid autonomy level: {}", level),
                ))
            }
        };
        config.permissions.approval.level = level;
    }

    if let Some(auto_approve) = body.auto_approve {
        config.permissions.approval.auto_approve = auto_approve;
    }

    if let Some(always_ask) = body.always_ask {
        config.permissions.approval.always_ask = always_ask;
    }

    // Save config
    if let Err(e) = config.save() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save config: {}", e),
        ));
    }

    let manager = crate::tools::global_approval_manager();

    Ok(Json(ApprovalConfigResponse {
        level: format!("{:?}", config.permissions.approval.level).to_lowercase(),
        auto_approve: config.permissions.approval.auto_approve.clone(),
        always_ask: config.permissions.approval.always_ask.clone(),
        pending_count: manager.map(|m| m.get_pending().len()).unwrap_or(0),
    }))
}
