use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::Serialize;

use super::super::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/status", get(status))
        .route("/v1/config", get(get_config))
        .route("/v1/config", axum::routing::patch(patch_config))
}

// --- Status ---

#[derive(Serialize)]
struct StatusResponse {
    version: &'static str,
    model: String,
    provider: String,
    uptime_secs: u64,
    channels: Vec<ChannelStatus>,
    skills_count: usize,
}

#[derive(Serialize)]
struct ChannelStatus {
    name: String,
    enabled: bool,
}

async fn status(State(state): State<Arc<AppState>>) -> Json<StatusResponse> {
    let config = state.config.read().await;
    let provider = config
        .resolve_provider(&config.agent.model)
        .map(|(n, _)| n.to_string())
        .unwrap_or_else(|| "none".to_string());

    let channels = vec![
        ChannelStatus {
            name: "telegram".into(),
            enabled: config.channels.telegram.enabled,
        },
        ChannelStatus {
            name: "discord".into(),
            enabled: config.channels.discord.enabled,
        },
        ChannelStatus {
            name: "slack".into(),
            enabled: config.channels.slack.enabled,
        },
        ChannelStatus {
            name: "whatsapp".into(),
            enabled: config.channels.whatsapp.enabled,
        },
        ChannelStatus {
            name: "email".into(),
            enabled: config.channels.email.enabled,
        },
        ChannelStatus {
            name: "web".into(),
            enabled: config.channels.web.enabled,
        },
    ];

    // Count installed skills
    let skills_count = crate::skills::SkillInstaller::list_installed()
        .await
        .map(|s| s.len())
        .unwrap_or(0);

    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION"),
        model: config.agent.model.clone(),
        provider,
        uptime_secs: state.started_at.elapsed().as_secs(),
        channels,
        skills_count,
    })
}

// --- Config ---

#[derive(Serialize)]
struct ConfigResponse {
    agent: AgentConfigView,
    channels: ChannelsConfigView,
    has_provider: bool,
    provider_name: String,
}

#[derive(Serialize)]
struct AgentConfigView {
    model: String,
    max_tokens: u32,
    temperature: f32,
    max_iterations: u32,
}

#[derive(Serialize)]
struct ChannelsConfigView {
    telegram_enabled: bool,
    discord_enabled: bool,
    slack_enabled: bool,
    whatsapp_enabled: bool,
    email_enabled: bool,
    web_enabled: bool,
}

async fn get_config(State(state): State<Arc<AppState>>) -> Json<ConfigResponse> {
    let config = state.config.read().await;
    let (provider_name, _) = config
        .resolve_provider(&config.agent.model)
        .unwrap_or(("none", &crate::config::ProviderConfig::default()));

    Json(ConfigResponse {
        agent: AgentConfigView {
            model: config.agent.model.clone(),
            max_tokens: config.agent.max_tokens,
            temperature: config.agent.temperature,
            max_iterations: config.agent.max_iterations,
        },
        channels: ChannelsConfigView {
            telegram_enabled: config.channels.telegram.enabled,
            discord_enabled: config.channels.discord.enabled,
            slack_enabled: config.channels.slack.enabled,
            whatsapp_enabled: config.channels.whatsapp.enabled,
            email_enabled: config.channels.email.enabled,
            web_enabled: config.channels.web.enabled,
        },
        has_provider: provider_name != "none",
        provider_name: provider_name.to_string(),
    })
}

// --- Config patch ---

#[derive(serde::Deserialize)]
struct ConfigPatch {
    key: String,
    value: serde_json::Value,
}

async fn patch_config(
    State(state): State<Arc<AppState>>,
    Json(patch): Json<ConfigPatch>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut config = state.config.read().await.clone();

    // For string values, use coerce_value (backwards compatible: "8192" → number).
    // For arrays, objects, bools, numbers — use directly.
    match &patch.value {
        serde_json::Value::String(s) => {
            crate::config::dotpath::config_set(&mut config, &patch.key, s)
        }
        other => crate::config::dotpath::config_set_value(&mut config, &patch.key, other.clone()),
    }
    .map_err(|_| StatusCode::BAD_REQUEST)?;

    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"ok": true, "key": patch.key})))
}
