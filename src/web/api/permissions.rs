use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::web::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/permissions", get(get_permissions).put(put_permissions))
        .route("/v1/permissions/acl", axum::routing::post(add_acl_entry))
        .route(
            "/v1/permissions/acl/{idx}",
            axum::routing::delete(delete_acl_entry),
        )
        .route(
            "/v1/permissions/test",
            axum::routing::post(test_path_permission),
        )
        .route("/v1/permissions/presets", get(get_permission_presets))
        .route("/v1/permissions/browse", get(browse_directories))
}

// ─── Permissions API ─────────────────────────────────────────────

/// Get current permissions configuration
async fn get_permissions(
    State(state): State<Arc<AppState>>,
) -> Json<crate::config::PermissionsConfig> {
    let config = state.config.read().await;
    Json(config.permissions.clone())
}

/// Update permissions configuration
async fn put_permissions(
    State(state): State<Arc<AppState>>,
    Json(perms): Json<crate::config::PermissionsConfig>,
) -> Result<Json<crate::config::PermissionsConfig>, StatusCode> {
    let mut config = state.config.write().await;
    config.permissions = perms;

    // Save to file
    if let Err(e) = config.save() {
        tracing::error!("Failed to save permissions config: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(config.permissions.clone()))
}

#[derive(Deserialize)]
struct AddAclRequest {
    path: String,
    #[serde(default)]
    entry_type: String,
    read: bool,
    write: bool,
    delete: bool,
    confirm: Option<String>,
}

/// Add a new ACL entry
async fn add_acl_entry(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AddAclRequest>,
) -> Result<Json<Vec<crate::config::AclEntry>>, StatusCode> {
    let mut config = state.config.write().await;

    let entry = crate::config::AclEntry {
        path: req.path,
        entry_type: if req.entry_type.is_empty() {
            "allow".to_string()
        } else {
            req.entry_type
        },
        permissions: crate::config::PathPermissions {
            read: crate::config::PermissionValue::Bool(req.read),
            write: crate::config::PermissionValue::Bool(req.write),
            delete: crate::config::PermissionValue::Bool(req.delete),
        },
    };

    config.permissions.acl.push(entry);

    if let Err(e) = config.save() {
        tracing::error!("Failed to save ACL entry: {}", e);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(Json(config.permissions.acl.clone()))
}

/// Delete an ACL entry by index
async fn delete_acl_entry(
    State(state): State<Arc<AppState>>,
    Path(idx): Path<usize>,
) -> Result<Json<Vec<crate::config::AclEntry>>, StatusCode> {
    let mut config = state.config.write().await;

    if idx < config.permissions.acl.len() {
        config.permissions.acl.remove(idx);

        if let Err(e) = config.save() {
            tracing::error!("Failed to save after ACL delete: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    Ok(Json(config.permissions.acl.clone()))
}

#[derive(Deserialize)]
struct TestPathRequest {
    path: String,
    operation: String,
}

#[derive(Serialize)]
struct TestPathResponse {
    allowed: bool,
    reason: Option<String>,
    needs_confirmation: bool,
}

/// Test if a path is allowed for an operation
async fn test_path_permission(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TestPathRequest>,
) -> Json<TestPathResponse> {
    use crate::tools::file::{check_path_permission, FileOp, PermissionResult};
    use std::path::PathBuf;

    let config = state.config.read().await;
    let path = PathBuf::from(&req.path);
    let op = match req.operation.as_str() {
        "read" => FileOp::Read,
        "write" => FileOp::Write,
        "delete" => FileOp::Delete,
        _ => FileOp::Read,
    };

    let result = check_path_permission(&path, op, Some(&config.permissions), None);

    let response = match result {
        PermissionResult::Allowed => TestPathResponse {
            allowed: true,
            reason: None,
            needs_confirmation: false,
        },
        PermissionResult::Denied(reason) => TestPathResponse {
            allowed: false,
            reason: Some(reason),
            needs_confirmation: false,
        },
        PermissionResult::NeedsConfirmation(reason) => TestPathResponse {
            allowed: true,
            reason: Some(reason),
            needs_confirmation: true,
        },
    };

    Json(response)
}

#[derive(Serialize)]
struct PermissionPreset {
    name: String,
    description: String,
    config: crate::config::PermissionsConfig,
}

/// Get available permission presets
#[allow(clippy::field_reassign_with_default)]
async fn get_permission_presets() -> Json<Vec<PermissionPreset>> {
    use crate::config::{DefaultPermissions, PathPermissions, PermissionMode, PermissionValue};

    let mut presets = Vec::new();

    // Developer preset
    let mut dev = crate::config::PermissionsConfig::default();
    dev.mode = PermissionMode::Acl;
    dev.default = DefaultPermissions {
        read: true,
        write: true,
        delete: false,
    };
    dev.acl.push(crate::config::AclEntry {
        path: "~/**".to_string(),
        entry_type: "allow".to_string(),
        permissions: PathPermissions {
            read: PermissionValue::Bool(true),
            write: PermissionValue::Bool(true),
            delete: PermissionValue::Confirm,
        },
    });
    presets.push(PermissionPreset {
        name: "developer".to_string(),
        description: "Full access to home directory with confirmation on delete".to_string(),
        config: dev,
    });

    // Restricted preset
    let mut restricted = crate::config::PermissionsConfig::default();
    restricted.mode = PermissionMode::Acl;
    restricted.default = DefaultPermissions {
        read: false,
        write: false,
        delete: false,
    };
    restricted.acl = vec![
        crate::config::AclEntry {
            path: "~/.homun/workspace/**".to_string(),
            entry_type: "allow".to_string(),
            permissions: PathPermissions {
                read: PermissionValue::Bool(true),
                write: PermissionValue::Bool(true),
                delete: PermissionValue::Bool(true),
            },
        },
        crate::config::AclEntry {
            path: "~/.homun/brain/**".to_string(),
            entry_type: "allow".to_string(),
            permissions: PathPermissions {
                read: PermissionValue::Bool(true),
                write: PermissionValue::Bool(true),
                delete: PermissionValue::Bool(false),
            },
        },
        crate::config::AclEntry {
            path: "~/.homun/memory/**".to_string(),
            entry_type: "allow".to_string(),
            permissions: PathPermissions {
                read: PermissionValue::Bool(true),
                write: PermissionValue::Bool(true),
                delete: PermissionValue::Bool(false),
            },
        },
    ];
    presets.push(PermissionPreset {
        name: "restricted".to_string(),
        description: "Only workspace, brain, and memory directories".to_string(),
        config: restricted,
    });

    // Paranoid preset
    let mut paranoid = crate::config::PermissionsConfig::default();
    paranoid.mode = PermissionMode::Acl;
    paranoid.default = DefaultPermissions {
        read: false,
        write: false,
        delete: false,
    };
    paranoid.acl = vec![
        crate::config::AclEntry {
            path: "~/**".to_string(),
            entry_type: "deny".to_string(),
            permissions: PathPermissions {
                read: PermissionValue::Bool(false),
                write: PermissionValue::Bool(false),
                delete: PermissionValue::Bool(false),
            },
        },
        crate::config::AclEntry {
            path: "~/.homun/brain/**".to_string(),
            entry_type: "allow".to_string(),
            permissions: PathPermissions {
                read: PermissionValue::Bool(true),
                write: PermissionValue::Confirm,
                delete: PermissionValue::Confirm,
            },
        },
    ];
    presets.push(PermissionPreset {
        name: "paranoid".to_string(),
        description: "Deny all by default, only brain with confirmation".to_string(),
        config: paranoid,
    });

    Json(presets)
}

// ─── Directory Browser API ──────────────────────────────────────

#[derive(Deserialize)]
struct BrowseQuery {
    path: Option<String>,
}

#[derive(Serialize)]
struct BrowseEntry {
    name: String,
    path: String,
    is_dir: bool,
}

#[derive(Serialize)]
struct BrowseResult {
    current_path: String,
    parent_path: Option<String>,
    entries: Vec<BrowseEntry>,
}

/// Browse directories for path picker
async fn browse_directories(
    Query(q): Query<BrowseQuery>,
) -> Result<Json<BrowseResult>, StatusCode> {
    use std::fs;

    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"));

    // Resolve the requested path
    let current = match q.path {
        Some(ref p) if !p.is_empty() => {
            let expanded = if let Some(stripped) = p.strip_prefix("~/") {
                home.join(stripped)
            } else if p == "~" {
                home.clone()
            } else {
                std::path::PathBuf::from(p)
            };

            // Canonicalize if exists, otherwise use as-is
            if expanded.exists() {
                expanded.canonicalize().unwrap_or(expanded)
            } else {
                expanded
            }
        }
        _ => home.clone(),
    };

    // Get parent path
    let parent = current.parent().map(|p| {
        if p == home {
            "~".to_string()
        } else {
            p.to_string_lossy().to_string()
        }
    });

    // List directory entries
    let mut entries = Vec::new();
    if current.is_dir() {
        if let Ok(read_dir) = fs::read_dir(&current) {
            for entry in read_dir.filter_map(|e| e.ok()) {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                // Only show directories and hidden check
                if path.is_dir() && !name.starts_with('.') {
                    let display_path = if path.starts_with(&home) {
                        format!("~/{}", path.strip_prefix(&home).unwrap().to_string_lossy())
                    } else {
                        path.to_string_lossy().to_string()
                    };

                    entries.push(BrowseEntry {
                        name,
                        path: display_path,
                        is_dir: true,
                    });
                }
            }
        }
    }

    // Sort by name
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let current_display = if current == home {
        "~".to_string()
    } else if current.starts_with(&home) {
        format!(
            "~/{}",
            current.strip_prefix(&home).unwrap().to_string_lossy()
        )
    } else {
        current.to_string_lossy().to_string()
    };

    Ok(Json(BrowseResult {
        current_path: current_display,
        parent_path: parent,
        entries,
    }))
}
