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
            "/v1/account/tokens/{token}",
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

#[derive(Debug, Serialize)]
struct TokenResponse {
    token: String,
    name: String,
    enabled: bool,
    scope: String,
    last_used: Option<String>,
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
        .map(|t| TokenResponse {
            token: t.token,
            name: t.name,
            enabled: t.enabled,
            scope: t.scope,
            last_used: t.last_used,
            created_at: t.created_at,
        })
        .collect();

    Ok(Json(response))
}

/// Create a new webhook token
async fn create_token(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<Json<TokenResponse>, (StatusCode, Json<serde_json::Value>)> {
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

    // Generate token
    let token = format!("wh_{}", uuid::Uuid::new_v4().simple());

    let scope = body.scope.as_deref().unwrap_or("admin");
    db.create_webhook_token(&token, &owner.id, &body.name, scope)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })?;

    Ok(Json(TokenResponse {
        token,
        name: body.name,
        enabled: true,
        scope: scope.to_string(),
        last_used: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Delete a webhook token
async fn delete_token(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
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

    let removed = db.delete_webhook_token(&token).await.map_err(|e| {
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
            Json(serde_json::json!({"error": "Token not found"})),
        ))
    }
}

/// Toggle a webhook token (enable/disable)
async fn toggle_token(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<Json<TokenResponse>, (StatusCode, Json<serde_json::Value>)> {
    let db = match &state.db {
        Some(db) => db,
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database not available"})),
            ))
        }
    };

    // Get current state and toggle
    let tokens = db.load_webhook_tokens("owner").await.unwrap_or_default();
    let current = tokens.iter().find(|t| t.token == token);
    let new_enabled = current.map(|t| !t.enabled).unwrap_or(false);

    let updated = db
        .toggle_webhook_token(&token, new_enabled)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })?;

    if !updated {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Token not found"})),
        ));
    }

    Ok(Json(TokenResponse {
        token,
        name: current.map(|t| t.name.clone()).unwrap_or_default(),
        enabled: new_enabled,
        scope: current
            .map(|t| t.scope.clone())
            .unwrap_or_else(|| "admin".to_string()),
        last_used: current.and_then(|t| t.last_used.clone()),
        created_at: current.map(|t| t.created_at.clone()).unwrap_or_default(),
    }))
}
