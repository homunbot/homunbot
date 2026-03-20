use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::super::auth::{check_admin, AuthUser};
use super::super::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/vault", get(list_vault_keys).post(set_vault_secret))
        .route(
            "/v1/vault/{key}/reveal",
            axum::routing::post(reveal_vault_secret),
        )
        .route(
            "/v1/vault/{key}",
            axum::routing::delete(delete_vault_secret),
        )
        // VLT-4: Vault access audit log
        .route("/v1/vault/audit", get(get_vault_audit_log))
        // --- Vault 2FA ---
        .route("/v1/vault/2fa/status", get(get_2fa_status))
        .route("/v1/vault/2fa/setup", axum::routing::post(setup_2fa))
        .route(
            "/v1/vault/2fa/confirm",
            axum::routing::post(confirm_2fa_setup),
        )
        .route("/v1/vault/2fa/verify", axum::routing::post(verify_2fa))
        .route("/v1/vault/2fa/disable", axum::routing::post(disable_2fa))
        .route(
            "/v1/vault/2fa/recovery",
            axum::routing::post(get_recovery_codes),
        )
        .route(
            "/v1/vault/2fa/settings",
            axum::routing::patch(update_2fa_settings),
        )
}

// Local copy of OkResponse for this module
#[derive(Serialize)]
struct OkResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Serialize)]
struct VaultKeysResponse {
    keys: Vec<String>,
}

async fn list_vault_keys(
    State(state): State<Arc<AppState>>,
) -> Result<Json<VaultKeysResponse>, StatusCode> {
    audit_log(&state, "*", "list", true);
    let secrets =
        crate::storage::global_secrets().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let all = secrets
        .load()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut keys: Vec<String> = all
        .keys()
        .filter_map(|k| k.strip_prefix("vault.").map(|s| s.to_string()))
        .collect();
    keys.sort_unstable();

    Ok(Json(VaultKeysResponse { keys }))
}

#[derive(Deserialize)]
struct SetVaultRequest {
    key: String,
    value: String,
}

async fn set_vault_secret(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Json(req): Json<SetVaultRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    check_admin(&auth)?;
    // Validate key: lowercase alphanumeric + underscore only
    if req.key.is_empty()
        || !req
            .key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Ok(Json(OkResponse {
            ok: false,
            message: Some("Key must match [a-z0-9_]+".to_string()),
        }));
    }

    let secrets =
        crate::storage::global_secrets().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let secret_key = crate::storage::SecretKey::custom(&format!("vault.{}", req.key));
    secrets
        .set(&secret_key, &req.value)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    audit_log(&state, &req.key, "store", true);

    Ok(Json(OkResponse {
        ok: true,
        message: None,
    }))
}

#[derive(Serialize)]
struct RevealResponse {
    ok: bool,
    key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    requires_2fa: Option<bool>,
}

#[derive(Deserialize)]
struct RevealRequest {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

#[cfg(feature = "vault-2fa")]
async fn reveal_vault_secret(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
    Json(req): Json<RevealRequest>,
) -> Result<Json<RevealResponse>, StatusCode> {
    use crate::security::{global_session_manager, TotpManager, TwoFactorStorage};

    // Check if 2FA is enabled
    let storage = match TwoFactorStorage::new() {
        Ok(s) => s,
        Err(_) => {
            // If we can't load 2FA config, allow access (fail open for availability)
            tracing::warn!("Could not load 2FA config, allowing vault access");
            audit_log(&state, &key, "reveal", true);
            return do_reveal_secret(&key).await;
        }
    };

    let config = match storage.load() {
        Ok(c) => c,
        Err(_) => {
            tracing::warn!("Could not load 2FA config, allowing vault access");
            audit_log(&state, &key, "reveal", true);
            return do_reveal_secret(&key).await;
        }
    };

    if !config.enabled {
        // 2FA not enabled, allow access
        audit_log(&state, &key, "reveal", true);
        return do_reveal_secret(&key).await;
    }

    // 2FA is enabled - verify authentication
    let authenticated = if let Some(ref session_id) = req.session_id {
        // Check session
        let session_manager = global_session_manager();
        session_manager.verify_session(session_id).await
    } else if let Some(ref code) = req.code {
        // Verify code directly
        let manager = match TotpManager::new(&config.totp_secret, &config.account) {
            Ok(m) => m,
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        };
        manager.verify(code)
    } else {
        false
    };

    if !authenticated {
        audit_log(&state, &key, "reveal", false);
        return Ok(Json(RevealResponse {
            ok: false,
            key: key.clone(),
            value: None,
            message: Some(
                "Two-factor authentication required. Provide 'code' or 'session_id'.".to_string(),
            ),
            requires_2fa: Some(true),
        }));
    }

    audit_log(&state, &key, "reveal", true);
    do_reveal_secret(&key).await
}

#[cfg(not(feature = "vault-2fa"))]
async fn reveal_vault_secret(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
    _req: Json<RevealRequest>,
) -> Result<Json<RevealResponse>, StatusCode> {
    // 2FA feature not enabled, allow direct access
    audit_log(&state, &key, "reveal", true);
    do_reveal_secret(&key).await
}

async fn do_reveal_secret(key: &str) -> Result<Json<RevealResponse>, StatusCode> {
    let secrets =
        crate::storage::global_secrets().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let secret_key = crate::storage::SecretKey::custom(&format!("vault.{key}"));

    match secrets.get(&secret_key) {
        Ok(Some(value)) => Ok(Json(RevealResponse {
            ok: true,
            key: key.to_string(),
            value: Some(value),
            message: None,
            requires_2fa: None,
        })),
        Ok(None) => Ok(Json(RevealResponse {
            ok: false,
            key: key.to_string(),
            value: None,
            message: Some("Secret not found".to_string()),
            requires_2fa: None,
        })),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn delete_vault_secret(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(key): Path<String>,
) -> Result<Json<OkResponse>, StatusCode> {
    check_admin(&auth)?;
    let secrets =
        crate::storage::global_secrets().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let secret_key = crate::storage::SecretKey::custom(&format!("vault.{key}"));

    secrets
        .delete(&secret_key)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    audit_log(&state, &key, "delete", true);

    Ok(Json(OkResponse {
        ok: true,
        message: None,
    }))
}

// ─── VLT-4: Vault Audit Logging ─────────────────────────────────

/// Fire-and-forget audit log for web API vault operations.
fn audit_log(state: &Arc<AppState>, key: &str, action: &str, success: bool) {
    if let Some(db) = &state.db {
        let db = db.clone();
        let key = key.to_string();
        let action = action.to_string();
        tokio::spawn(async move {
            if let Err(e) = db
                .insert_vault_access(&key, &action, "web_api", success, None)
                .await
            {
                tracing::warn!(error = ?e, "Failed to write vault audit log");
            }
        });
    }
}

#[derive(Deserialize)]
struct AuditQuery {
    #[serde(default = "default_audit_limit")]
    limit: i64,
}

fn default_audit_limit() -> i64 {
    50
}

async fn get_vault_audit_log(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<AuditQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let rows = db
        .list_vault_access_log(q.limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({ "entries": rows })))
}

// ─── Vault 2FA ──────────────────────────────────────────────────

#[cfg(feature = "vault-2fa")]
/// Pending 2FA setup data (stored in memory until confirmed)
static PENDING_2FA_SETUP: std::sync::Mutex<Option<Pending2FaSetup>> = std::sync::Mutex::new(None);

#[cfg(feature = "vault-2fa")]
#[derive(Clone)]
struct Pending2FaSetup {
    secret: String,
    account: String,
    recovery_codes: Vec<String>,
    qr_image: String,
    qr_url: String,
}

// Response structs - always available for API contract

#[derive(Serialize)]
struct TwoFaStatusResponse {
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    account: Option<String>,
    session_timeout_secs: u64,
    recovery_codes_remaining: usize,
}

#[derive(Serialize)]
struct TwoFaSetupResponse {
    qr_image: String,
    secret: String,
    uri: String,
}

#[derive(Deserialize)]
struct Confirm2FaSetupRequest {
    code: String,
}

#[derive(Serialize)]
struct Confirm2FaSetupResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery_codes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Deserialize)]
struct Verify2FaRequest {
    code: String,
}

#[derive(Serialize)]
struct Verify2FaResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Deserialize)]
struct Disable2FaRequest {
    code: String,
}

#[derive(Deserialize)]
struct RecoveryCodesRequest {
    session_id: String,
}

#[derive(Serialize)]
struct RecoveryCodesResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    codes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Deserialize)]
struct Update2FaSettingsRequest {
    session_id: String,
    session_timeout_secs: Option<u64>,
}

// === Feature-gated implementations ===

#[cfg(feature = "vault-2fa")]
async fn get_2fa_status() -> Result<Json<TwoFaStatusResponse>, StatusCode> {
    let storage =
        crate::security::TwoFactorStorage::new().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config = storage
        .load()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(TwoFaStatusResponse {
        enabled: config.enabled,
        created_at: if config.enabled {
            Some(config.created_at.to_rfc3339())
        } else {
            None
        },
        account: if config.enabled {
            Some(config.account.clone())
        } else {
            None
        },
        session_timeout_secs: config.session_timeout_secs,
        recovery_codes_remaining: config.recovery_codes.len(),
    }))
}

#[cfg(not(feature = "vault-2fa"))]
async fn get_2fa_status() -> Result<Json<TwoFaStatusResponse>, StatusCode> {
    Ok(Json(TwoFaStatusResponse {
        enabled: false,
        created_at: None,
        account: None,
        session_timeout_secs: 300,
        recovery_codes_remaining: 0,
    }))
}

#[cfg(feature = "vault-2fa")]
async fn setup_2fa() -> Result<Json<TwoFaSetupResponse>, StatusCode> {
    use crate::security::{TotpManager, TwoFactorStorage};

    // Check if already enabled
    let storage = TwoFactorStorage::new().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let config = storage
        .load()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if config.enabled {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Generate new secret
    let secret = TotpManager::generate_secret();

    // Get account name (hostname + username)
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "localhost".to_string());
    let username = whoami::username();
    let account = format!("{}@{}", username, hostname);

    // Create TOTP manager for QR generation
    let manager =
        TotpManager::new(&secret, &account).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let qr_image = manager
        .generate_qr_base64()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let qr_url = manager.get_url();

    // Generate recovery codes
    let recovery_codes = crate::security::generate_recovery_codes();

    // Store pending setup
    let pending = Pending2FaSetup {
        secret: secret.clone(),
        account: account.clone(),
        recovery_codes: recovery_codes.clone(),
        qr_image: qr_image.clone(),
        qr_url: qr_url.clone(),
    };

    *PENDING_2FA_SETUP.lock().unwrap() = Some(pending);

    Ok(Json(TwoFaSetupResponse {
        qr_image,
        secret,
        uri: qr_url,
    }))
}

#[cfg(not(feature = "vault-2fa"))]
async fn setup_2fa() -> Result<Json<TwoFaSetupResponse>, StatusCode> {
    Ok(Json(TwoFaSetupResponse {
        qr_image: String::new(),
        secret: String::new(),
        uri: String::new(),
    }))
}

#[cfg(feature = "vault-2fa")]
async fn confirm_2fa_setup(
    Json(req): Json<Confirm2FaSetupRequest>,
) -> Result<Json<Confirm2FaSetupResponse>, StatusCode> {
    use crate::security::{TotpManager, TwoFactorConfig, TwoFactorStorage};

    // Get pending setup
    let pending = {
        let guard = PENDING_2FA_SETUP.lock().unwrap();
        guard.clone()
    };

    let pending = match pending {
        Some(p) => p,
        None => {
            return Ok(Json(Confirm2FaSetupResponse {
                ok: false,
                recovery_codes: None,
                message: Some("No pending 2FA setup. Call /setup first.".to_string()),
            }));
        }
    };

    // Verify the code
    let manager = TotpManager::new(&pending.secret, &pending.account)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !manager.verify(&req.code) {
        return Ok(Json(Confirm2FaSetupResponse {
            ok: false,
            recovery_codes: None,
            message: Some("Invalid code. Please try again.".to_string()),
        }));
    }

    // Save configuration
    let config = TwoFactorConfig::new(
        &pending.account,
        Some(300), // Default 5 minutes
    );
    // Use the secret from pending, not the one generated in new()
    let mut config = config;
    config.totp_secret = pending.secret.clone();
    config.recovery_codes = pending.recovery_codes.clone();

    let storage = TwoFactorStorage::new().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    storage
        .save(&config)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Clear pending setup
    *PENDING_2FA_SETUP.lock().unwrap() = None;

    tracing::info!("2FA setup completed successfully");

    Ok(Json(Confirm2FaSetupResponse {
        ok: true,
        recovery_codes: Some(pending.recovery_codes),
        message: None,
    }))
}

#[cfg(not(feature = "vault-2fa"))]
async fn confirm_2fa_setup(
    _req: Json<Confirm2FaSetupRequest>,
) -> Result<Json<Confirm2FaSetupResponse>, StatusCode> {
    Ok(Json(Confirm2FaSetupResponse {
        ok: false,
        recovery_codes: None,
        message: Some("2FA feature not enabled in this build".to_string()),
    }))
}

#[cfg(feature = "vault-2fa")]
async fn verify_2fa(
    Json(req): Json<Verify2FaRequest>,
) -> Result<Json<Verify2FaResponse>, StatusCode> {
    use crate::security::{global_session_manager, TotpManager, TwoFactorStorage};

    let storage = TwoFactorStorage::new().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut config = storage
        .load()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !config.enabled {
        return Ok(Json(Verify2FaResponse {
            ok: false,
            session_id: None,
            expires_in_secs: None,
            message: Some("2FA is not enabled".to_string()),
        }));
    }

    // Check lockout
    if config.is_locked_out() {
        return Ok(Json(Verify2FaResponse {
            ok: false,
            session_id: None,
            expires_in_secs: None,
            message: Some("Too many failed attempts. Please wait.".to_string()),
        }));
    }

    // Verify code
    let manager = TotpManager::new(&config.totp_secret, &config.account).map_err(|e| {
        tracing::error!("Failed to create TotpManager: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::debug!(
        code = %req.code,
        secret_len = config.totp_secret.len(),
        "Verifying 2FA code"
    );

    if manager.verify(&req.code) {
        // Create session
        let session_manager = global_session_manager();
        let session_id = session_manager.create_session().await;

        // Reset failed attempts
        config.reset_failed_attempts();
        storage
            .save(&config)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(Json(Verify2FaResponse {
            ok: true,
            session_id: Some(session_id),
            expires_in_secs: Some(config.session_timeout_secs),
            message: None,
        }))
    } else {
        // Record failed attempt
        config.record_failed_attempt();
        storage
            .save(&config)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        Ok(Json(Verify2FaResponse {
            ok: false,
            session_id: None,
            expires_in_secs: None,
            message: Some(format!(
                "Invalid code. {} attempts remaining.",
                5u32.saturating_sub(config.failed_attempts)
            )),
        }))
    }
}

#[cfg(not(feature = "vault-2fa"))]
async fn verify_2fa(_req: Json<Verify2FaRequest>) -> Result<Json<Verify2FaResponse>, StatusCode> {
    Ok(Json(Verify2FaResponse {
        ok: false,
        session_id: None,
        expires_in_secs: None,
        message: Some("2FA feature not enabled in this build".to_string()),
    }))
}

#[cfg(feature = "vault-2fa")]
async fn disable_2fa(Json(req): Json<Disable2FaRequest>) -> Result<Json<OkResponse>, StatusCode> {
    use crate::security::{TotpManager, TwoFactorStorage};

    let storage = TwoFactorStorage::new().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut config = storage
        .load()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !config.enabled {
        return Ok(Json(OkResponse {
            ok: false,
            message: Some("2FA is not enabled".to_string()),
        }));
    }

    // Check lockout
    if config.is_locked_out() {
        return Ok(Json(OkResponse {
            ok: false,
            message: Some("Too many failed attempts. Please wait.".to_string()),
        }));
    }

    // Verify code
    let manager = TotpManager::new(&config.totp_secret, &config.account)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !manager.verify(&req.code) {
        config.record_failed_attempt();
        storage
            .save(&config)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        return Ok(Json(OkResponse {
            ok: false,
            message: Some("Invalid code".to_string()),
        }));
    }

    // Disable 2FA
    config.enabled = false;
    config.totp_secret = String::new();
    config.recovery_codes = Vec::new();
    storage
        .save(&config)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!("2FA disabled");

    Ok(Json(OkResponse {
        ok: true,
        message: None,
    }))
}

#[cfg(not(feature = "vault-2fa"))]
async fn disable_2fa(_req: Json<Disable2FaRequest>) -> Result<Json<OkResponse>, StatusCode> {
    Ok(Json(OkResponse {
        ok: false,
        message: Some("2FA feature not enabled in this build".to_string()),
    }))
}

#[cfg(feature = "vault-2fa")]
async fn get_recovery_codes(
    Json(req): Json<RecoveryCodesRequest>,
) -> Result<Json<RecoveryCodesResponse>, StatusCode> {
    use crate::security::{global_session_manager, TwoFactorStorage};

    // Verify session
    let session_manager = global_session_manager();
    if !session_manager.verify_session(&req.session_id).await {
        return Ok(Json(RecoveryCodesResponse {
            ok: false,
            codes: None,
            message: Some("Invalid or expired session. Please authenticate first.".to_string()),
        }));
    }

    let storage = TwoFactorStorage::new().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let config = storage
        .load()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !config.enabled {
        return Ok(Json(RecoveryCodesResponse {
            ok: false,
            codes: None,
            message: Some("2FA is not enabled".to_string()),
        }));
    }

    Ok(Json(RecoveryCodesResponse {
        ok: true,
        codes: Some(config.recovery_codes),
        message: None,
    }))
}

#[cfg(not(feature = "vault-2fa"))]
async fn get_recovery_codes(
    _req: Json<RecoveryCodesRequest>,
) -> Result<Json<RecoveryCodesResponse>, StatusCode> {
    Ok(Json(RecoveryCodesResponse {
        ok: false,
        codes: None,
        message: Some("2FA feature not enabled in this build".to_string()),
    }))
}

#[cfg(feature = "vault-2fa")]
async fn update_2fa_settings(
    Json(req): Json<Update2FaSettingsRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    use crate::security::{global_session_manager, TwoFactorStorage};

    // Verify session
    let session_manager = global_session_manager();
    if !session_manager.verify_session(&req.session_id).await {
        return Ok(Json(OkResponse {
            ok: false,
            message: Some("Invalid or expired session. Please authenticate first.".to_string()),
        }));
    }

    let storage = TwoFactorStorage::new().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut config = storage
        .load()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !config.enabled {
        return Ok(Json(OkResponse {
            ok: false,
            message: Some("2FA is not enabled".to_string()),
        }));
    }

    // Update settings
    if let Some(timeout) = req.session_timeout_secs {
        // Validate: between 1 minute and 1 hour
        if !(60..=3600).contains(&timeout) {
            return Ok(Json(OkResponse {
                ok: false,
                message: Some("session_timeout_secs must be between 60 and 3600".to_string()),
            }));
        }
        config.session_timeout_secs = timeout;
    }

    storage
        .save(&config)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(OkResponse {
        ok: true,
        message: None,
    }))
}

#[cfg(not(feature = "vault-2fa"))]
async fn update_2fa_settings(
    _req: Json<Update2FaSettingsRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    Ok(Json(OkResponse {
        ok: false,
        message: Some("2FA feature not enabled in this build".to_string()),
    }))
}
