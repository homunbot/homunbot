use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use serde::Serialize;

use super::super::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/onboarding/status", get(onboarding_status))
        .route("/v1/onboarding/complete", post(onboarding_complete))
}

#[derive(Serialize)]
struct OnboardingStatus {
    completed: bool,
    has_provider: bool,
    has_model: bool,
    user_name: String,
    language: String,
    timezone: String,
    channels_configured: Vec<String>,
}

async fn onboarding_status(State(state): State<Arc<AppState>>) -> Json<OnboardingStatus> {
    let config = state.config.read().await;

    let has_provider = config
        .resolve_provider(&config.agent.model)
        .map(|(n, _)| n != "none")
        .unwrap_or(false);

    let mut channels_configured = Vec::new();
    if config.is_channel_configured("telegram") {
        channels_configured.push("telegram".into());
    }
    if config.is_channel_configured("discord") {
        channels_configured.push("discord".into());
    }
    if config.is_channel_configured("slack") {
        channels_configured.push("slack".into());
    }
    if config.is_channel_configured("whatsapp") {
        channels_configured.push("whatsapp".into());
    }
    if config.is_channel_configured("email") {
        channels_configured.push("email".into());
    }

    Json(OnboardingStatus {
        completed: config.ui.onboarding_completed,
        has_provider,
        has_model: !config.agent.model.is_empty(),
        user_name: config.agent.user_name.clone(),
        language: config.ui.language.clone(),
        timezone: config.agent.timezone.clone(),
        channels_configured,
    })
}

async fn onboarding_complete(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut config = state.config.read().await.clone();
    config.ui.onboarding_completed = true;
    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"ok": true})))
}
