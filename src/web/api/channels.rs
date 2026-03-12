use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use super::super::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/channels/{name}", get(get_channel))
        .route(
            "/v1/channels/configure",
            axum::routing::post(configure_channel),
        )
        .route(
            "/v1/channels/deactivate",
            axum::routing::post(deactivate_channel),
        )
        .route("/v1/channels/test", axum::routing::post(test_channel))
        .route("/v1/channels/whatsapp/pair", get(ws_whatsapp_pair))
}

/// Get current channel configuration (tokens masked).
async fn get_channel(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let config = state.config.read().await;

    // Helper: mask a token showing only last 4 chars
    fn mask_token(token: &str) -> String {
        if token.is_empty() || token == "***ENCRYPTED***" {
            return String::new();
        }
        if token.len() <= 4 {
            return "\u{2022}\u{2022}\u{2022}\u{2022}".to_string();
        }
        format!(
            "{}{}",
            "\u{2022}".repeat(token.len().min(20) - 4),
            &token[token.len() - 4..]
        )
    }

    // Resolve real token from encrypted storage for masking
    fn resolve_and_mask(channel_name: &str) -> String {
        if let Ok(secrets) = crate::storage::global_secrets() {
            let key = crate::storage::SecretKey::channel_token(channel_name);
            if let Ok(Some(real_token)) = secrets.get(&key) {
                return mask_token(&real_token);
            }
        }
        String::new()
    }

    let result = match name.as_str() {
        "telegram" => {
            let masked = if config.channels.telegram.token == "***ENCRYPTED***" {
                resolve_and_mask("telegram")
            } else {
                mask_token(&config.channels.telegram.token)
            };
            serde_json::json!({
                "name": "telegram",
                "enabled": config.channels.telegram.enabled,
                "configured": config.is_channel_configured("telegram"),
                "token_masked": masked,
                "has_token": !masked.is_empty(),
                "allow_from": config.channels.telegram.allow_from,
            })
        }
        "discord" => {
            let masked = if config.channels.discord.token == "***ENCRYPTED***" {
                resolve_and_mask("discord")
            } else {
                mask_token(&config.channels.discord.token)
            };
            serde_json::json!({
                "name": "discord",
                "enabled": config.channels.discord.enabled,
                "configured": config.is_channel_configured("discord"),
                "token_masked": masked,
                "has_token": !masked.is_empty(),
                "allow_from": config.channels.discord.allow_from,
                "default_channel_id": config.channels.discord.default_channel_id,
            })
        }
        "slack" => {
            let masked = if config.channels.slack.token == "***ENCRYPTED***" {
                resolve_and_mask("slack")
            } else {
                mask_token(&config.channels.slack.token)
            };
            serde_json::json!({
                "name": "slack",
                "enabled": config.channels.slack.enabled,
                "configured": config.is_channel_configured("slack"),
                "token_masked": masked,
                "has_token": !masked.is_empty(),
                "allow_from": config.channels.slack.allow_from,
                "channel_id": config.channels.slack.channel_id,
            })
        }
        "whatsapp" => {
            serde_json::json!({
                "name": "whatsapp",
                "enabled": config.channels.whatsapp.enabled,
                "configured": config.is_channel_configured("whatsapp"),
                "phone_number": config.channels.whatsapp.phone_number,
                "allow_from": config.channels.whatsapp.allow_from,
            })
        }
        "web" => {
            serde_json::json!({
                "name": "web",
                "enabled": config.channels.web.enabled,
                "configured": true,
                "host": config.channels.web.host,
                "port": config.channels.web.port,
            })
        }
        "email" => {
            // Check if password is encrypted
            let has_password = if config.channels.email.password == "***ENCRYPTED***" {
                if let Ok(secrets) = crate::storage::global_secrets() {
                    let key = crate::storage::SecretKey::channel_token("email");
                    matches!(secrets.get(&key), Ok(Some(_)))
                } else {
                    false
                }
            } else {
                !config.channels.email.password.is_empty()
            };
            // Read mode/notify/trigger from emails.default
            let default_acc = config.channels.emails.get("default");
            let email_mode = default_acc
                .map(|a| match a.mode {
                    crate::config::EmailMode::Automatic => "automatic",
                    crate::config::EmailMode::OnDemand => "on_demand",
                    crate::config::EmailMode::Assisted => "assisted",
                })
                .unwrap_or("assisted");
            let notify_channel = default_acc
                .and_then(|a| a.notify_channel.as_deref())
                .unwrap_or("");
            let notify_chat_id = default_acc
                .and_then(|a| a.notify_chat_id.as_deref())
                .unwrap_or("");
            // Resolve trigger word: config -> vault -> auto-generate (on_demand only)
            let trigger_word = default_acc
                .and_then(|a| a.trigger_word.as_deref().filter(|s| !s.is_empty()))
                .map(|s| s.to_string())
                .or_else(|| {
                    // Check vault (always -- user may have generated one previously)
                    let secrets = crate::storage::global_secrets().ok()?;
                    let key = crate::storage::SecretKey::custom("email.default.trigger_word");
                    secrets.get(&key).ok().flatten()
                })
                .unwrap_or_default();
            serde_json::json!({
                "name": "email",
                "enabled": config.channels.email.enabled,
                "configured": config.is_channel_configured("email"),
                "imap_host": config.channels.email.imap_host,
                "imap_port": config.channels.email.imap_port,
                "imap_folder": config.channels.email.imap_folder,
                "smtp_host": config.channels.email.smtp_host,
                "smtp_port": config.channels.email.smtp_port,
                "smtp_tls": config.channels.email.smtp_tls,
                "username": config.channels.email.username,
                "has_password": has_password,
                "from_address": config.channels.email.from_address,
                "idle_timeout_secs": config.channels.email.idle_timeout_secs,
                "allow_from": config.channels.email.allow_from,
                "email_mode": email_mode,
                "email_notify_channel": notify_channel,
                "email_notify_chat_id": notify_chat_id,
                "email_trigger_word": trigger_word,
            })
        }
        _ => return Err(StatusCode::NOT_FOUND),
    };

    Ok(Json(result))
}

#[derive(Deserialize)]
struct ChannelConfigRequest {
    name: String,
    token: Option<String>,
    phone_number: Option<String>,
    #[serde(default)]
    allow_from: Option<Vec<String>>,
    host: Option<String>,
    port: Option<u16>,
    auth_token: Option<String>,
    default_channel_id: Option<String>,
    // Email-specific fields
    imap_host: Option<String>,
    imap_port: Option<u16>,
    imap_folder: Option<String>,
    smtp_host: Option<String>,
    smtp_port: Option<u16>,
    smtp_tls: Option<bool>,
    username: Option<String>,
    password: Option<String>,
    from_address: Option<String>,
    idle_timeout_secs: Option<u64>,
    // Email mode/notify fields (write to channels.emails.default)
    email_mode: Option<String>,
    email_notify_channel: Option<String>,
    email_notify_chat_id: Option<String>,
    email_trigger_word: Option<String>,
}

#[derive(Serialize)]
struct ChannelConfigResponse {
    ok: bool,
    message: String,
}

async fn configure_channel(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChannelConfigRequest>,
) -> Result<Json<ChannelConfigResponse>, StatusCode> {
    let mut config = state.config.read().await.clone();

    match req.name.as_str() {
        "telegram" => {
            // Store token in encrypted storage
            if let Some(token) = &req.token {
                if !token.is_empty() {
                    let secrets = crate::storage::global_secrets()
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    let key = crate::storage::SecretKey::channel_token("telegram");
                    secrets
                        .set(&key, token)
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    config.channels.telegram.token = "***ENCRYPTED***".to_string();
                }
            }
            if let Some(allow_from) = &req.allow_from {
                config.channels.telegram.allow_from = allow_from.clone();
            }
            config.channels.telegram.enabled = true;
        }
        "discord" => {
            if let Some(token) = &req.token {
                if !token.is_empty() {
                    let secrets = crate::storage::global_secrets()
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    let key = crate::storage::SecretKey::channel_token("discord");
                    secrets
                        .set(&key, token)
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    config.channels.discord.token = "***ENCRYPTED***".to_string();
                }
            }
            if let Some(allow_from) = &req.allow_from {
                config.channels.discord.allow_from = allow_from.clone();
            }
            if let Some(channel_id) = &req.default_channel_id {
                config.channels.discord.default_channel_id = channel_id.clone();
            }
            config.channels.discord.enabled = true;
        }
        "slack" => {
            if let Some(token) = &req.token {
                if !token.is_empty() {
                    let secrets = crate::storage::global_secrets()
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    let key = crate::storage::SecretKey::channel_token("slack");
                    secrets
                        .set(&key, token)
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    config.channels.slack.token = "***ENCRYPTED***".to_string();
                }
            }
            if let Some(allow_from) = &req.allow_from {
                config.channels.slack.allow_from = allow_from.clone();
            }
            if let Some(channel_id) = &req.default_channel_id {
                config.channels.slack.channel_id = channel_id.clone();
            }
            config.channels.slack.enabled = true;
        }
        "whatsapp" => {
            if let Some(phone) = &req.phone_number {
                config.channels.whatsapp.phone_number = phone.clone();
            }
            if let Some(allow_from) = &req.allow_from {
                config.channels.whatsapp.allow_from = allow_from.clone();
            }
            // Don't set enabled here -- WhatsApp needs pairing first
        }
        "web" => {
            if let Some(host) = &req.host {
                config.channels.web.host = host.clone();
            }
            if let Some(port) = req.port {
                config.channels.web.port = port;
            }
            if let Some(auth_token) = &req.auth_token {
                config.channels.web.auth_token = auth_token.clone();
            }
            // Web is always enabled
        }
        "email" => {
            // IMAP settings
            if let Some(imap_host) = &req.imap_host {
                config.channels.email.imap_host = imap_host.clone();
            }
            if let Some(imap_port) = req.imap_port {
                config.channels.email.imap_port = imap_port;
            }
            if let Some(imap_folder) = &req.imap_folder {
                config.channels.email.imap_folder = imap_folder.clone();
            }
            // SMTP settings
            if let Some(smtp_host) = &req.smtp_host {
                config.channels.email.smtp_host = smtp_host.clone();
            }
            if let Some(smtp_port) = req.smtp_port {
                config.channels.email.smtp_port = smtp_port;
            }
            if let Some(smtp_tls) = req.smtp_tls {
                config.channels.email.smtp_tls = smtp_tls;
            }
            // Credentials
            if let Some(username) = &req.username {
                config.channels.email.username = username.clone();
            }
            if let Some(password) = &req.password {
                if !password.is_empty() {
                    // Store password in encrypted storage (both legacy + new key)
                    let secrets = crate::storage::global_secrets()
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    let legacy_key = crate::storage::SecretKey::channel_token("email");
                    secrets
                        .set(&legacy_key, password)
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    // Also store with multi-account key so EmailChannel resolves it directly
                    let new_key = crate::storage::SecretKey::custom("email.default.password");
                    secrets
                        .set(&new_key, password)
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                    config.channels.email.password = "***ENCRYPTED***".to_string();
                }
            }
            if let Some(from_address) = &req.from_address {
                config.channels.email.from_address = from_address.clone();
            }
            if let Some(idle_timeout_secs) = req.idle_timeout_secs {
                config.channels.email.idle_timeout_secs = idle_timeout_secs;
            }
            if let Some(allow_from) = &req.allow_from {
                config.channels.email.allow_from = allow_from.clone();
            }
            config.channels.email.enabled = true;

            // Sync to channels.emails["default"] for the multi-account system
            let default_acc = config
                .channels
                .emails
                .entry("default".to_string())
                .or_insert_with(|| crate::config::EmailAccountConfig {
                    enabled: true,
                    ..Default::default()
                });
            // Mirror IMAP/SMTP/credentials from legacy config
            default_acc.enabled = true;
            default_acc.imap_host = config.channels.email.imap_host.clone();
            default_acc.imap_port = config.channels.email.imap_port;
            default_acc.imap_folder = config.channels.email.imap_folder.clone();
            default_acc.smtp_host = config.channels.email.smtp_host.clone();
            default_acc.smtp_port = config.channels.email.smtp_port;
            default_acc.smtp_tls = config.channels.email.smtp_tls;
            default_acc.username = config.channels.email.username.clone();
            default_acc.password = config.channels.email.password.clone();
            default_acc.from_address = config.channels.email.from_address.clone();
            default_acc.idle_timeout_secs = config.channels.email.idle_timeout_secs;
            default_acc.allow_from = config.channels.email.allow_from.clone();
            // Apply mode/notify/trigger from request
            if let Some(mode_str) = &req.email_mode {
                default_acc.mode = match mode_str.as_str() {
                    "automatic" => crate::config::EmailMode::Automatic,
                    "on_demand" => crate::config::EmailMode::OnDemand,
                    _ => crate::config::EmailMode::Assisted,
                };
            }
            if let Some(nc) = &req.email_notify_channel {
                default_acc.notify_channel = if nc.is_empty() {
                    None
                } else {
                    Some(nc.clone())
                };
            }
            if let Some(ncid) = &req.email_notify_chat_id {
                default_acc.notify_chat_id = if ncid.is_empty() {
                    None
                } else {
                    Some(ncid.clone())
                };
            }
            if let Some(tw) = &req.email_trigger_word {
                default_acc.trigger_word = if tw.is_empty() {
                    None
                } else {
                    Some(tw.clone())
                };
            }
            // Auto-generate trigger word for on_demand mode if not set
            if default_acc.mode == crate::config::EmailMode::OnDemand
                && default_acc
                    .trigger_word
                    .as_ref()
                    .is_none_or(|t| t.is_empty())
            {
                if let Ok(secrets) = crate::storage::global_secrets() {
                    let key = crate::storage::SecretKey::custom("email.default.trigger_word");
                    let existing = secrets.get(&key).ok().flatten();
                    if existing.is_none() {
                        let tw = super::email_accounts::generate_email_trigger_word();
                        let _ = secrets.set(&key, &tw);
                        tracing::info!(trigger_word = %tw, "Auto-generated trigger word on configure");
                    }
                }
            }
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    }

    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ChannelConfigResponse {
        ok: true,
        message: format!("Channel '{}' configured", req.name),
    }))
}

#[derive(Deserialize)]
struct ChannelDeactivateRequest {
    name: String,
}

async fn deactivate_channel(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChannelDeactivateRequest>,
) -> Result<Json<ChannelConfigResponse>, StatusCode> {
    // Remove token from encrypted storage
    if matches!(
        req.name.as_str(),
        "telegram" | "discord" | "slack" | "email"
    ) {
        if let Ok(secrets) = crate::storage::global_secrets() {
            let key = crate::storage::SecretKey::channel_token(&req.name);
            let _ = secrets.delete(&key);
        }
    }

    let mut config = state.config.read().await.clone();

    match req.name.as_str() {
        "telegram" => {
            config.channels.telegram.enabled = false;
            config.channels.telegram.token = String::new();
            config.channels.telegram.allow_from.clear();
        }
        "discord" => {
            config.channels.discord.enabled = false;
            config.channels.discord.token = String::new();
            config.channels.discord.allow_from.clear();
            config.channels.discord.default_channel_id = String::new();
        }
        "whatsapp" => {
            config.channels.whatsapp.enabled = false;
            // Keep phone_number and db_path -- user might re-pair
        }
        "web" => {
            // Web cannot be fully deactivated
            return Ok(Json(ChannelConfigResponse {
                ok: false,
                message: "Web UI cannot be deactivated".to_string(),
            }));
        }
        "slack" => {
            config.channels.slack.enabled = false;
            config.channels.slack.token = String::new();
            config.channels.slack.channel_id = String::new();
            config.channels.slack.allow_from.clear();
        }
        "email" => {
            config.channels.email.enabled = false;
            config.channels.email.password = String::new();
            config.channels.email.allow_from.clear();
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    }

    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ChannelConfigResponse {
        ok: true,
        message: format!("Channel '{}' deactivated", req.name),
    }))
}

#[derive(Deserialize)]
struct ChannelTestRequest {
    name: String,
    token: Option<String>,
}

#[derive(Serialize)]
struct ChannelTestResponse {
    ok: bool,
    message: String,
}

async fn test_channel(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChannelTestRequest>,
) -> Json<ChannelTestResponse> {
    match req.name.as_str() {
        "telegram" => {
            // Get token: use provided one, or fall back to stored one
            let token = if let Some(t) = &req.token {
                if !t.is_empty() {
                    Some(t.clone())
                } else {
                    None
                }
            } else {
                None
            };
            let token = token.or_else(|| {
                crate::storage::global_secrets().ok().and_then(|s| {
                    let key = crate::storage::SecretKey::channel_token("telegram");
                    s.get(&key).ok().flatten()
                })
            });

            let Some(token) = token else {
                return Json(ChannelTestResponse {
                    ok: false,
                    message: "No token provided or stored".to_string(),
                });
            };

            // Call Telegram getMe API
            let url = format!("https://api.telegram.org/bot{}/getMe", token);
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default();

            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    #[derive(Deserialize)]
                    struct TgResponse {
                        ok: bool,
                        result: Option<TgUser>,
                    }
                    #[derive(Deserialize)]
                    struct TgUser {
                        username: Option<String>,
                        first_name: Option<String>,
                    }

                    match resp.json::<TgResponse>().await {
                        Ok(tg) if tg.ok => {
                            let name = tg
                                .result
                                .map(|u| {
                                    u.username
                                        .unwrap_or_else(|| u.first_name.unwrap_or_default())
                                })
                                .unwrap_or_default();
                            Json(ChannelTestResponse {
                                ok: true,
                                message: format!("Connected as @{}", name),
                            })
                        }
                        _ => Json(ChannelTestResponse {
                            ok: false,
                            message: "Invalid response from Telegram".to_string(),
                        }),
                    }
                }
                Ok(resp) => Json(ChannelTestResponse {
                    ok: false,
                    message: format!("Telegram returned {}", resp.status()),
                }),
                Err(e) => Json(ChannelTestResponse {
                    ok: false,
                    message: format!("Connection failed: {}", e),
                }),
            }
        }
        "discord" => {
            // For Discord, just validate token format (starts with "MT" or similar)
            let token = req.token.or_else(|| {
                crate::storage::global_secrets().ok().and_then(|s| {
                    let key = crate::storage::SecretKey::channel_token("discord");
                    s.get(&key).ok().flatten()
                })
            });

            match token {
                Some(t) if t.len() > 20 => Json(ChannelTestResponse {
                    ok: true,
                    message: "Token format looks valid".to_string(),
                }),
                Some(_) => Json(ChannelTestResponse {
                    ok: false,
                    message: "Token too short -- check your Discord bot token".to_string(),
                }),
                None => Json(ChannelTestResponse {
                    ok: false,
                    message: "No token provided or stored".to_string(),
                }),
            }
        }
        "whatsapp" => {
            let config = state.config.read().await;
            let db_exists = config.channels.whatsapp.resolved_db_path().exists();
            let has_phone = !config.channels.whatsapp.phone_number.is_empty();
            drop(config);

            if db_exists && has_phone {
                Json(ChannelTestResponse {
                    ok: true,
                    message: "Session exists -- WhatsApp is paired".to_string(),
                })
            } else if has_phone {
                Json(ChannelTestResponse {
                    ok: false,
                    message: "Phone configured but not paired yet".to_string(),
                })
            } else {
                Json(ChannelTestResponse {
                    ok: false,
                    message: "Not configured -- enter phone number and pair".to_string(),
                })
            }
        }
        "slack" => {
            let token = req.token.or_else(|| {
                crate::storage::global_secrets().ok().and_then(|s| {
                    let key = crate::storage::SecretKey::channel_token("slack");
                    s.get(&key).ok().flatten()
                })
            });

            let Some(token) = token else {
                return Json(ChannelTestResponse {
                    ok: false,
                    message: "No token provided or stored".to_string(),
                });
            };

            // Call Slack auth.test API
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default();

            match client
                .get("https://slack.com/api/auth.test")
                .bearer_auth(&token)
                .send()
                .await
            {
                Ok(resp) => {
                    #[derive(Deserialize)]
                    struct SlackAuth {
                        ok: bool,
                        user: Option<String>,
                        team: Option<String>,
                        error: Option<String>,
                    }
                    match resp.json::<SlackAuth>().await {
                        Ok(auth) if auth.ok => {
                            let user = auth.user.unwrap_or_default();
                            let team = auth.team.unwrap_or_default();
                            Json(ChannelTestResponse {
                                ok: true,
                                message: format!("Connected as {} in {}", user, team),
                            })
                        }
                        Ok(auth) => Json(ChannelTestResponse {
                            ok: false,
                            message: format!(
                                "Slack auth failed: {}",
                                auth.error.unwrap_or_else(|| "unknown error".into())
                            ),
                        }),
                        Err(e) => Json(ChannelTestResponse {
                            ok: false,
                            message: format!("Failed to parse Slack response: {}", e),
                        }),
                    }
                }
                Err(e) => Json(ChannelTestResponse {
                    ok: false,
                    message: format!("Connection failed: {}", e),
                }),
            }
        }
        "email" => {
            let config = state.config.read().await;
            let imap_host = config.channels.email.imap_host.clone();
            let imap_port = config.channels.email.imap_port;
            let smtp_host = config.channels.email.smtp_host.clone();
            let username = config.channels.email.username.clone();
            let password_stored = config.channels.email.password.clone();
            drop(config);

            if imap_host.is_empty() {
                return Json(ChannelTestResponse {
                    ok: false,
                    message: "IMAP host is required".to_string(),
                });
            }
            if username.is_empty() {
                return Json(ChannelTestResponse {
                    ok: false,
                    message: "Username is required".to_string(),
                });
            }

            // Resolve password from vault if encrypted
            let has_password = if password_stored == "***ENCRYPTED***" {
                crate::storage::global_secrets()
                    .ok()
                    .and_then(|s| {
                        let key = crate::storage::SecretKey::channel_token("email");
                        s.get(&key).ok().flatten()
                    })
                    .is_some()
            } else {
                !password_stored.is_empty()
            };

            if !has_password {
                return Json(ChannelTestResponse {
                    ok: false,
                    message: "No password configured".to_string(),
                });
            }

            // Test TCP connection to IMAP server
            match tokio::time::timeout(
                std::time::Duration::from_secs(10),
                tokio::net::TcpStream::connect(format!("{}:{}", imap_host, imap_port)),
            )
            .await
            {
                Ok(Ok(_)) => {
                    let smtp_status = if !smtp_host.is_empty() {
                        " SMTP configured."
                    } else {
                        " SMTP not configured (send disabled)."
                    };
                    Json(ChannelTestResponse {
                        ok: true,
                        message: format!(
                            "IMAP reachable at {}:{}.{}",
                            imap_host, imap_port, smtp_status
                        ),
                    })
                }
                Ok(Err(e)) => Json(ChannelTestResponse {
                    ok: false,
                    message: format!("Cannot reach {}:{} -- {}", imap_host, imap_port, e),
                }),
                Err(_) => Json(ChannelTestResponse {
                    ok: false,
                    message: format!("Connection to {}:{} timed out (10s)", imap_host, imap_port),
                }),
            }
        }
        "web" => Json(ChannelTestResponse {
            ok: true,
            message: "Web UI is running".to_string(),
        }),
        _ => Json(ChannelTestResponse {
            ok: false,
            message: "Unknown channel".to_string(),
        }),
    }
}

// ═══════════════════════════════════════════════════
// WhatsApp Pairing -- WebSocket endpoint
// ═══════════════════════════════════════════════════

/// WebSocket upgrade for WhatsApp pairing flow.
///
/// Protocol:
/// 1. Client sends `{ "phone": "393331234567" }`
/// 2. Server starts wa-rs bot with `with_pair_code()`
/// 3. Server sends events:
///    - `{ "type": "pairing_code", "code": "ABCD-EFGH", "timeout": 60 }`
///    - `{ "type": "paired" }`
///    - `{ "type": "connected" }`
///    - `{ "type": "error", "message": "..." }`
async fn ws_whatsapp_pair(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_whatsapp_pairing(socket, state))
}

async fn handle_whatsapp_pairing(socket: WebSocket, state: Arc<AppState>) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Step 1: wait for { "phone": "..." } from client
    let phone = loop {
        match ws_receiver.next().await {
            Some(Ok(Message::Text(text))) => {
                let text = text.to_string();
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(phone) = parsed.get("phone").and_then(|v| v.as_str()) {
                        if !phone.is_empty() {
                            break phone.to_string();
                        }
                    }
                }
                let err =
                    serde_json::json!({"type": "error", "message": "Send {\"phone\": \"number\"}"});
                let _ = ws_sender.send(Message::Text(err.to_string().into())).await;
            }
            Some(Ok(Message::Close(_))) | None => return,
            _ => continue,
        }
    };

    tracing::info!(phone = %phone, "WhatsApp pairing started via WebSocket");

    // Step 2: resolve DB path from config
    let db_path = {
        let config = state.config.read().await;
        config.channels.whatsapp.resolved_db_path()
    };

    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            let msg = serde_json::json!({"type": "error", "message": format!("Cannot create directory: {e}")});
            let _ = ws_sender.send(Message::Text(msg.to_string().into())).await;
            return;
        }
    }

    // Step 3: start wa-rs bot with pair code
    // Note: wa-rs now handles stale sessions internally -- when with_pair_code() is set,
    // it clears the device identity so the handshake uses registration instead of login.
    // Bridge events from wa-rs callback -> mpsc -> WebSocket
    let (_event_tx, mut event_rx) = tokio::sync::mpsc::channel::<serde_json::Value>(16);

    #[cfg(feature = "channel-whatsapp")]
    let bot_handle = {
        let pair_phone = phone.clone();
        let db_path_str = db_path.to_string_lossy().to_string();
        let event_tx = _event_tx; // Use the sender
        tokio::spawn(async move { run_whatsapp_pair_bot(pair_phone, db_path_str, event_tx).await })
    };

    #[cfg(not(feature = "channel-whatsapp"))]
    let bot_handle = tokio::spawn(async {
        // WhatsApp not available without channel-whatsapp feature
    });

    // Step 4: Forward events to WebSocket
    let state_for_save = state.clone();
    let phone_for_save = phone.clone();

    loop {
        tokio::select! {
            // Event from wa-rs bot
            event = event_rx.recv() => {
                match event {
                    Some(msg) => {
                        let is_done = msg.get("type").and_then(|v| v.as_str()) == Some("paired")
                            || msg.get("type").and_then(|v| v.as_str()) == Some("connected");

                        if ws_sender.send(Message::Text(msg.to_string().into())).await.is_err() {
                            break; // WebSocket closed
                        }

                        // On successful pairing, update config
                        if msg.get("type").and_then(|v| v.as_str()) == Some("paired") {
                            let mut config = state_for_save.config.read().await.clone();
                            config.channels.whatsapp.phone_number = phone_for_save.clone();
                            config.channels.whatsapp.enabled = true;
                            let _ = state_for_save.save_config(config).await;

                            // Send done message
                            let done = serde_json::json!({"type": "done"});
                            let _ = ws_sender.send(Message::Text(done.to_string().into())).await;
                        }

                        if is_done {
                            // Give the client a moment to process, then close
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                            break;
                        }
                    }
                    None => break, // Channel closed (bot finished or errored)
                }
            }
            // Client message (close or cancel)
            client_msg = ws_receiver.next() => {
                match client_msg {
                    Some(Ok(Message::Close(_))) | None => {
                        bot_handle.abort();
                        break;
                    }
                    _ => {} // Ignore other client messages during pairing
                }
            }
        }
    }

    // Cleanup: abort bot if still running
    bot_handle.abort();
    tracing::info!(phone = %phone, "WhatsApp pairing WebSocket closed");
}

/// Run the wa-rs bot for pairing, sending events back via the channel.
/// This mirrors the TUI's `run_whatsapp_pairing()` logic.
#[cfg(feature = "channel-whatsapp")]
async fn run_whatsapp_pair_bot(
    phone: String,
    db_path: String,
    event_tx: tokio::sync::mpsc::Sender<serde_json::Value>,
) {
    use wa_rs::bot::Bot;
    use wa_rs::store::SqliteStore;
    use wa_rs_core::types::events::Event as WaEvent;
    use wa_rs_proto::whatsapp as wa;
    use wa_rs_tokio_transport::TokioWebSocketTransportFactory;
    use wa_rs_ureq_http::UreqHttpClient;

    let backend = match SqliteStore::new(&db_path).await {
        Ok(store) => Arc::new(store),
        Err(e) => {
            let msg = serde_json::json!({"type": "error", "message": format!("WhatsApp store error: {e}")});
            let _ = event_tx.send(msg).await;
            return;
        }
    };

    let transport_factory = TokioWebSocketTransportFactory::new();
    let http_client = UreqHttpClient::new();
    let tx = event_tx.clone();

    let bot = Bot::builder()
        .with_backend(backend)
        .with_transport_factory(transport_factory)
        .with_http_client(http_client)
        .with_device_props(
            Some("Linux".to_string()),
            None,
            Some(wa::device_props::PlatformType::Chrome),
        )
        .with_pair_code(wa_rs::pair_code::PairCodeOptions {
            phone_number: phone,
            ..Default::default()
        })
        .skip_history_sync()
        .on_event(move |event, _client| {
            let tx = tx.clone();
            async move {
                let msg = match event {
                    WaEvent::PairingCode { code, timeout } => Some(serde_json::json!({
                        "type": "pairing_code",
                        "code": code,
                        "timeout": timeout.as_secs()
                    })),
                    WaEvent::PairSuccess(_) => Some(serde_json::json!({"type": "paired"})),
                    WaEvent::PairError(err) => Some(serde_json::json!({
                        "type": "error",
                        "message": format!("{}", err.error)
                    })),
                    WaEvent::Connected(_) => Some(serde_json::json!({"type": "connected"})),
                    WaEvent::LoggedOut(_) => Some(serde_json::json!({
                        "type": "error",
                        "message": "Logged out"
                    })),
                    _ => None,
                };
                if let Some(msg) = msg {
                    let _ = tx.send(msg).await;
                }
            }
        })
        .build()
        .await;

    let mut bot = match bot {
        Ok(b) => b,
        Err(e) => {
            let msg = serde_json::json!({"type": "error", "message": format!("Failed to build WhatsApp bot: {e}")});
            let _ = event_tx.send(msg).await;
            return;
        }
    };

    match bot.run().await {
        Ok(handle) => {
            let _ = handle.await;
        }
        Err(e) => {
            let msg = serde_json::json!({"type": "error", "message": format!("Failed to start WhatsApp bot: {e}")});
            let _ = event_tx.send(msg).await;
        }
    }
}
