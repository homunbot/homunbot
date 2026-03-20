use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use super::super::auth::{require_admin, AuthUser};
use super::super::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/account", get(get_account))
        .route(
            "/v1/account/identities",
            get(list_identities).post(add_identity),
        )
        .route(
            "/v1/account/identities/{channel}/{platform_id}",
            axum::routing::delete(remove_identity),
        )
        .route("/v1/account/tokens", get(list_tokens).post(create_token))
        .route(
            "/v1/account/tokens/{token_id}",
            axum::routing::delete(delete_token).post(toggle_token),
        )
}

#[derive(Debug, Serialize)]
struct AccountResponse {
    id: String,
    username: String,
    role: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct IdentityResponse {
    channel: String,
    platform_id: String,
    display_name: Option<String>,
    created_at: String,
}

/// Token listed in GET response — masked, no full token.
#[derive(Debug, Serialize)]
struct TokenResponse {
    /// Stable identifier (first 16 chars of the token) — used for delete/toggle.
    token_id: String,
    /// Masked display value, e.g. `wh_****…abcd`.
    display_token: String,
    name: String,
    enabled: bool,
    scope: String,
    last_used: Option<String>,
    created_at: String,
    expires_at: Option<String>,
}

/// Token returned on creation — includes the full token (shown once).
#[derive(Debug, Serialize)]
struct CreateTokenResponse {
    /// Full token value — copy it now, it won't be shown again.
    token: String,
    token_id: String,
    name: String,
    scope: String,
    expires_at: Option<String>,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct AddIdentityRequest {
    channel: String,
    platform_id: String,
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateTokenRequest {
    name: String,
    /// Token scope: "admin" (default), "read", "write"
    scope: Option<String>,
    /// Optional expiry duration: "7d", "30d", "90d". Omit or null for no expiry.
    expires_in: Option<String>,
}

/// Get the owner account info (first user in database)
async fn get_account(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Option<AccountResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let db = match &state.db {
        Some(db) => db,
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database not available"})),
            ))
        }
    };

    let users = db.load_all_users().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    // Return the first user (owner)
    let owner = users.into_iter().next().map(|u| {
        let roles: Vec<String> = serde_json::from_str(&u.roles).unwrap_or_default();
        let role = roles.first().cloned().unwrap_or_else(|| "user".to_string());
        AccountResponse {
            id: u.id,
            username: u.username,
            role,
            created_at: u.created_at,
        }
    });

    Ok(Json(owner))
}

/// List all channel identities for the owner
async fn list_identities(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<IdentityResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let db = match &state.db {
        Some(db) => db,
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database not available"})),
            ))
        }
    };

    // Get owner user ID
    let users = db.load_all_users().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    let owner = match users.into_iter().next() {
        Some(u) => u,
        None => return Ok(Json(Vec::new())),
    };

    let identities = db.load_user_identities(&owner.id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    let response: Vec<IdentityResponse> = identities
        .into_iter()
        .map(|i| IdentityResponse {
            channel: i.channel,
            platform_id: i.platform_id,
            display_name: i.display_name,
            created_at: i.created_at,
        })
        .collect();

    Ok(Json(response))
}

/// Add a new channel identity
async fn add_identity(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AddIdentityRequest>,
) -> Result<Json<IdentityResponse>, (StatusCode, Json<serde_json::Value>)> {
    let db = match &state.db {
        Some(db) => db,
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database not available"})),
            ))
        }
    };

    // Get owner user ID
    let users = db.load_all_users().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    let owner = match users.into_iter().next() {
        Some(u) => u,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "No owner user found. Create one first."})),
            ))
        }
    };

    db.add_user_identity(
        &owner.id,
        &body.channel,
        &body.platform_id,
        body.display_name.as_deref(),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    Ok(Json(IdentityResponse {
        channel: body.channel,
        platform_id: body.platform_id,
        display_name: body.display_name,
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Remove a channel identity
async fn remove_identity(
    State(state): State<Arc<AppState>>,
    Path((channel, platform_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let db = match &state.db {
        Some(db) => db,
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database not available"})),
            ))
        }
    };

    // Get owner user ID
    let users = db.load_all_users().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    let owner = match users.into_iter().next() {
        Some(u) => u,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "No owner user found"})),
            ))
        }
    };

    let removed = db
        .remove_user_identity(&owner.id, &channel, &platform_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })?;

    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Identity not found"})),
        ))
    }
}

/// List all webhook tokens for the owner
async fn list_tokens(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<TokenResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let db = match &state.db {
        Some(db) => db,
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database not available"})),
            ))
        }
    };

    // Get owner user ID
    let users = db.load_all_users().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    let owner = match users.into_iter().next() {
        Some(u) => u,
        None => return Ok(Json(Vec::new())),
    };

    let tokens = db.load_webhook_tokens(&owner.id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    let response: Vec<TokenResponse> = tokens
        .into_iter()
        .map(|t| {
            let token_id = t.token.chars().take(16).collect::<String>();
            let last4 = if t.token.len() > 4 {
                &t.token[t.token.len() - 4..]
            } else {
                &t.token
            };
            TokenResponse {
                token_id,
                display_token: format!("wh_****…{last4}"),
                name: t.name,
                enabled: t.enabled,
                scope: t.scope,
                last_used: t.last_used,
                created_at: t.created_at,
                expires_at: t.expires_at,
            }
        })
        .collect();

    Ok(Json(response))
}

/// Create a new webhook token (admin-only).
async fn create_token(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>, (StatusCode, Json<serde_json::Value>)> {
    require_admin(&auth)?;

    let db = match &state.db {
        Some(db) => db,
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database not available"})),
            ))
        }
    };

    // Get owner user ID
    let users = db.load_all_users().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    let owner = match users.into_iter().next() {
        Some(u) => u,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "No owner user found. Create one first."})),
            ))
        }
    };

    // Compute expiry timestamp from duration string
    let expires_at = match body.expires_in.as_deref() {
        Some("7d") => Some(chrono::Utc::now() + chrono::Duration::days(7)),
        Some("30d") => Some(chrono::Utc::now() + chrono::Duration::days(30)),
        Some("90d") => Some(chrono::Utc::now() + chrono::Duration::days(90)),
        _ => None,
    };
    let expires_at_str = expires_at.map(|dt| dt.to_rfc3339());

    // Generate token
    let token = format!("wh_{}", uuid::Uuid::new_v4().simple());
    let token_id = token.chars().take(16).collect::<String>();

    let scope = body.scope.as_deref().unwrap_or("admin");
    db.create_webhook_token(&token, &owner.id, &body.name, scope, expires_at_str.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })?;

    Ok(Json(CreateTokenResponse {
        token,
        token_id,
        name: body.name,
        scope: scope.to_string(),
        expires_at: expires_at_str,
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Delete a webhook token by prefix ID (admin-only).
async fn delete_token(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(token_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    require_admin(&auth)?;

    let db = match &state.db {
        Some(db) => db,
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database not available"})),
            ))
        }
    };

    // Resolve the full token from the prefix
    let row = db.find_token_by_prefix(&token_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    let row = row.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Token not found"})),
        )
    })?;

    db.delete_webhook_token(&row.token).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Toggle a webhook token enable/disable by prefix ID (admin-only).
async fn toggle_token(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(token_id): Path<String>,
) -> Result<Json<TokenResponse>, (StatusCode, Json<serde_json::Value>)> {
    require_admin(&auth)?;

    let db = match &state.db {
        Some(db) => db,
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database not available"})),
            ))
        }
    };

    // Resolve full token from prefix
    let row = db.find_token_by_prefix(&token_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    let row = row.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Token not found"})),
        )
    })?;

    let new_enabled = !row.enabled;
    db.toggle_webhook_token(&row.token, new_enabled)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })?;

    let display_last4 = if row.token.len() > 4 {
        &row.token[row.token.len() - 4..]
    } else {
        &row.token
    };

    Ok(Json(TokenResponse {
        token_id,
        display_token: format!("wh_****…{display_last4}"),
        name: row.name,
        enabled: new_enabled,
        scope: row.scope,
        last_used: row.last_used,
        created_at: row.created_at,
        expires_at: row.expires_at,
    }))
}
