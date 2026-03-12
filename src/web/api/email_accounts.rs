use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use super::super::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/email-accounts", get(list_email_accounts))
        .route(
            "/v1/email-accounts/configure",
            axum::routing::post(configure_email_account),
        )
        .route(
            "/v1/email-accounts/deactivate",
            axum::routing::post(deactivate_email_account),
        )
        .route(
            "/v1/email-accounts/test",
            axum::routing::post(test_email_account),
        )
        .route(
            "/v1/email-accounts/{name}",
            get(get_email_account).delete(delete_email_account),
        )
        .route(
            "/v1/channels/email/trigger-word",
            axum::routing::post(generate_or_get_trigger_word),
        )
}

/// List all email accounts (passwords masked).
async fn list_email_accounts(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let config = state.config.read().await;
    let accounts: Vec<serde_json::Value> = config
        .channels
        .emails
        .iter()
        .map(|(name, acc)| {
            let has_password = if acc.password == "***ENCRYPTED***" {
                crate::storage::global_secrets()
                    .ok()
                    .and_then(|s| {
                        let key =
                            crate::storage::SecretKey::custom(&format!("email.{name}.password"));
                        s.get(&key).ok().flatten()
                    })
                    .is_some()
            } else {
                !acc.password.is_empty()
            };
            serde_json::json!({
                "name": name,
                "enabled": acc.enabled,
                "configured": acc.is_configured(),
                "imap_host": acc.imap_host,
                "imap_port": acc.imap_port,
                "imap_folder": acc.imap_folder,
                "smtp_host": acc.smtp_host,
                "smtp_port": acc.smtp_port,
                "smtp_tls": acc.smtp_tls,
                "username": acc.username,
                "has_password": has_password,
                "from_address": acc.from_address,
                "idle_timeout_secs": acc.idle_timeout_secs,
                "allow_from": acc.allow_from,
                "mode": acc.mode,
                "notify_channel": acc.notify_channel,
                "notify_chat_id": acc.notify_chat_id,
                "trigger_word": acc.trigger_word,
                "batch_threshold": acc.batch_threshold,
                "batch_window_secs": acc.batch_window_secs,
                "send_delay_secs": acc.send_delay_secs,
            })
        })
        .collect();

    Json(serde_json::json!({ "accounts": accounts }))
}

/// Get a single email account by name (password masked).
async fn get_email_account(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let config = state.config.read().await;
    let acc = config
        .channels
        .emails
        .get(&name)
        .ok_or(StatusCode::NOT_FOUND)?;

    let has_password = if acc.password == "***ENCRYPTED***" {
        crate::storage::global_secrets()
            .ok()
            .and_then(|s| {
                let key = crate::storage::SecretKey::custom(&format!("email.{name}.password"));
                s.get(&key).ok().flatten()
            })
            .is_some()
    } else {
        !acc.password.is_empty()
    };

    Ok(Json(serde_json::json!({
        "name": name,
        "enabled": acc.enabled,
        "configured": acc.is_configured(),
        "imap_host": acc.imap_host,
        "imap_port": acc.imap_port,
        "imap_folder": acc.imap_folder,
        "smtp_host": acc.smtp_host,
        "smtp_port": acc.smtp_port,
        "smtp_tls": acc.smtp_tls,
        "username": acc.username,
        "has_password": has_password,
        "from_address": acc.from_address,
        "idle_timeout_secs": acc.idle_timeout_secs,
        "allow_from": acc.allow_from,
        "mode": acc.mode,
        "notify_channel": acc.notify_channel,
        "notify_chat_id": acc.notify_chat_id,
        "trigger_word": acc.trigger_word,
        "batch_threshold": acc.batch_threshold,
        "batch_window_secs": acc.batch_window_secs,
        "send_delay_secs": acc.send_delay_secs,
    })))
}

#[derive(Deserialize)]
struct EmailAccountRequest {
    name: String,
    // IMAP
    imap_host: Option<String>,
    imap_port: Option<u16>,
    imap_folder: Option<String>,
    // SMTP
    smtp_host: Option<String>,
    smtp_port: Option<u16>,
    smtp_tls: Option<bool>,
    // Credentials
    username: Option<String>,
    password: Option<String>,
    from_address: Option<String>,
    // Behaviour
    idle_timeout_secs: Option<u64>,
    #[serde(default)]
    allow_from: Option<Vec<String>>,
    mode: Option<crate::config::EmailMode>,
    notify_channel: Option<String>,
    notify_chat_id: Option<String>,
    trigger_word: Option<String>,
    // Batching
    batch_threshold: Option<u32>,
    batch_window_secs: Option<u64>,
    send_delay_secs: Option<u64>,
}

/// Create or update an email account.
async fn configure_email_account(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EmailAccountRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut config = state.config.read().await.clone();

    let acc = config
        .channels
        .emails
        .entry(req.name.clone())
        .or_insert_with(crate::config::EmailAccountConfig::default);

    // IMAP
    if let Some(v) = &req.imap_host {
        acc.imap_host = v.clone();
    }
    if let Some(v) = req.imap_port {
        acc.imap_port = v;
    }
    if let Some(v) = &req.imap_folder {
        acc.imap_folder = v.clone();
    }
    // SMTP
    if let Some(v) = &req.smtp_host {
        acc.smtp_host = v.clone();
    }
    if let Some(v) = req.smtp_port {
        acc.smtp_port = v;
    }
    if let Some(v) = req.smtp_tls {
        acc.smtp_tls = v;
    }
    // Credentials
    if let Some(v) = &req.username {
        acc.username = v.clone();
    }
    if let Some(password) = &req.password {
        if !password.is_empty() {
            let secrets =
                crate::storage::global_secrets().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let key = crate::storage::SecretKey::custom(&format!("email.{}.password", req.name));
            secrets
                .set(&key, password)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            acc.password = "***ENCRYPTED***".to_string();
        }
    }
    if let Some(v) = &req.from_address {
        acc.from_address = v.clone();
    }
    // Behaviour
    if let Some(v) = req.idle_timeout_secs {
        acc.idle_timeout_secs = v;
    }
    if let Some(v) = &req.allow_from {
        acc.allow_from = v.clone();
    }
    if let Some(v) = &req.mode {
        acc.mode = v.clone();
    }
    if let Some(v) = &req.notify_channel {
        acc.notify_channel = Some(v.clone());
    }
    if let Some(v) = &req.notify_chat_id {
        acc.notify_chat_id = Some(v.clone());
    }
    if let Some(v) = &req.trigger_word {
        acc.trigger_word = Some(v.clone());
    }
    // Batching
    if let Some(v) = req.batch_threshold {
        acc.batch_threshold = v;
    }
    if let Some(v) = req.batch_window_secs {
        acc.batch_window_secs = v;
    }
    if let Some(v) = req.send_delay_secs {
        acc.send_delay_secs = v;
    }

    acc.enabled = true;

    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "message": format!("Email account '{}' configured", req.name),
    })))
}

#[derive(Deserialize)]
struct EmailAccountNameRequest {
    name: String,
}

/// Deactivate an email account (keeps config, sets enabled=false).
async fn deactivate_email_account(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EmailAccountNameRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut config = state.config.read().await.clone();

    let acc = config
        .channels
        .emails
        .get_mut(&req.name)
        .ok_or(StatusCode::NOT_FOUND)?;

    acc.enabled = false;

    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "message": format!("Email account '{}' deactivated", req.name),
    })))
}

/// Delete an email account entirely (removes config + vault password).
async fn delete_email_account(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut config = state.config.read().await.clone();

    if config.channels.emails.remove(&name).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Clean up vault password
    if let Ok(secrets) = crate::storage::global_secrets() {
        let key = crate::storage::SecretKey::custom(&format!("email.{name}.password"));
        let _ = secrets.delete(&key);
        let trigger_key = crate::storage::SecretKey::custom(&format!("email.{name}.trigger_word"));
        let _ = secrets.delete(&trigger_key);
    }

    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "message": format!("Email account '{}' deleted", name),
    })))
}

/// Test IMAP/SMTP connection for an email account.
async fn test_email_account(
    State(state): State<Arc<AppState>>,
    Json(req): Json<EmailAccountNameRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let config = state.config.read().await;
    let acc = config
        .channels
        .emails
        .get(&req.name)
        .ok_or(StatusCode::NOT_FOUND)?;

    if !acc.is_configured() {
        return Ok(Json(serde_json::json!({
            "ok": false,
            "message": "Account is not fully configured (missing IMAP/SMTP/credentials).",
        })));
    }

    // Resolve password from vault
    let password = if acc.password == "***ENCRYPTED***" {
        let secrets =
            crate::storage::global_secrets().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let key = crate::storage::SecretKey::custom(&format!("email.{}.password", req.name));
        secrets
            .get(&key)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        acc.password.clone()
    };

    // Test IMAP connection
    let imap_result =
        test_imap_connection(&acc.imap_host, acc.imap_port, &acc.username, &password).await;

    match imap_result {
        Ok(()) => Ok(Json(serde_json::json!({
            "ok": true,
            "message": format!("IMAP connection to {}:{} successful", acc.imap_host, acc.imap_port),
        }))),
        Err(e) => Ok(Json(serde_json::json!({
            "ok": false,
            "message": format!("IMAP connection failed: {e}"),
        }))),
    }
}

/// Test IMAP connection by attempting a login.
#[cfg(feature = "channel-email")]
async fn test_imap_connection(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<(), String> {
    use std::sync::Arc as StdArc;
    use tokio::net::TcpStream;
    use tokio_rustls::rustls::{ClientConfig, RootCertStore};
    use tokio_rustls::TlsConnector;

    let addr = format!("{host}:{port}");
    let tcp = TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("TCP connect failed: {e}"))?;

    let certs = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.into(),
    };
    let rustls_config = ClientConfig::builder()
        .with_root_certificates(certs)
        .with_no_client_auth();
    let tls_connector: TlsConnector = StdArc::new(rustls_config).into();
    let sni: rustls_pki_types::DnsName = host
        .to_string()
        .try_into()
        .map_err(|_| format!("Invalid hostname: {host}"))?;
    let tls = tls_connector
        .connect(sni.into(), tcp)
        .await
        .map_err(|e| format!("TLS handshake failed: {e}"))?;

    let client = async_imap::Client::new(tls);
    let mut session = client
        .login(username, password)
        .await
        .map_err(|e| format!("IMAP login failed: {}", e.0))?;

    let _ = session.logout().await;
    Ok(())
}

#[cfg(not(feature = "channel-email"))]
async fn test_imap_connection(
    _host: &str,
    _port: u16,
    _username: &str,
    _password: &str,
) -> Result<(), String> {
    Err("Email feature not enabled (compile with --features channel-email)".to_string())
}

/// Generate or retrieve trigger word for an email account.
/// POST /api/v1/channels/email/trigger-word
/// Body: { "account": "default" }  (optional, defaults to "default")
async fn generate_or_get_trigger_word(
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let account = req
        .get("account")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let vault_key = format!("email.{account}.trigger_word");

    let secrets =
        crate::storage::global_secrets().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let key = crate::storage::SecretKey::custom(&vault_key);

    // Return existing or generate new
    let trigger_word = match secrets.get(&key) {
        Ok(Some(tw)) => tw,
        _ => {
            let tw = generate_email_trigger_word();
            secrets
                .set(&key, &tw)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            tracing::info!(account, trigger_word = %tw, "Generated trigger word via API");
            tw
        }
    };

    Ok(Json(serde_json::json!({ "trigger_word": trigger_word })))
}

pub(super) fn generate_email_trigger_word() -> String {
    use std::time::SystemTime;
    let seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
    let mut result = String::from("hm-");
    let mut state = seed;
    for _ in 0..6 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let idx = (state >> 33) as usize % chars.len();
        result.push(chars[idx]);
    }
    result
}
