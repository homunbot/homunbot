use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use super::server::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health))
        .route("/v1/status", get(status))
        .route("/v1/config", get(get_config))
        .route("/v1/config", axum::routing::patch(patch_config))
        .route("/v1/skills", get(list_skills))
        .route("/v1/skills/search", get(search_skills))
        .route("/v1/skills/install", axum::routing::post(install_skill))
        .route("/v1/skills/{name}", axum::routing::delete(delete_skill))
        .route("/v1/providers", get(list_providers))
        .route("/v1/providers/configure", axum::routing::post(configure_provider))
        .route("/v1/providers/activate", axum::routing::post(activate_provider))
        .route("/v1/providers/deactivate", axum::routing::post(deactivate_provider))
        .route("/v1/providers/models", get(list_all_models))
        .route("/v1/providers/ollama/models", get(list_ollama_models))
        // --- Channels ---
        .route("/v1/channels/{name}", get(get_channel))
        .route("/v1/channels/configure", axum::routing::post(configure_channel))
        .route("/v1/channels/deactivate", axum::routing::post(deactivate_channel))
        .route("/v1/channels/test", axum::routing::post(test_channel))
        .route("/v1/channels/whatsapp/pair", get(ws_whatsapp_pair))
}

// --- Health check ---

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
    uptime_secs: u64,
}

async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        uptime_secs: state.started_at.elapsed().as_secs(),
    })
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
            name: "whatsapp".into(),
            enabled: config.channels.whatsapp.enabled,
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
    whatsapp_enabled: bool,
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
            whatsapp_enabled: config.channels.whatsapp.enabled,
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
    value: String,
}

async fn patch_config(
    State(state): State<Arc<AppState>>,
    Json(patch): Json<ConfigPatch>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut config = state.config.read().await.clone();
    crate::config::dotpath::config_set(&mut config, &patch.key, &patch.value)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    state.save_config(config).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"ok": true, "key": patch.key, "value": patch.value})))
}

// --- Skills ---

#[derive(Serialize)]
struct SkillView {
    name: String,
    description: String,
    path: String,
    source: String,
}

/// Detect the source of an installed skill by checking marker files.
fn detect_skill_source(path: &std::path::Path) -> String {
    if path.join(".clawhub-source").exists() {
        "clawhub".to_string()
    } else {
        "github".to_string()
    }
}

async fn list_skills() -> Json<Vec<SkillView>> {
    let skills = crate::skills::SkillInstaller::list_installed()
        .await
        .unwrap_or_default();

    Json(
        skills
            .into_iter()
            .map(|s| {
                let source = detect_skill_source(&s.path);
                SkillView {
                    name: s.name,
                    description: s.description,
                    path: s.path.display().to_string(),
                    source,
                }
            })
            .collect(),
    )
}

#[derive(serde::Deserialize)]
struct InstallRequest {
    source: String,
}

#[derive(Serialize)]
struct InstallResponse {
    ok: bool,
    name: String,
    message: String,
}

async fn install_skill(
    Json(req): Json<InstallRequest>,
) -> Result<Json<InstallResponse>, StatusCode> {
    if let Some(slug) = req.source.strip_prefix("clawhub:") {
        let hub = crate::skills::ClawHubInstaller::new();
        match hub.install(slug).await {
            Ok(result) => Ok(Json(InstallResponse {
                ok: true,
                name: result.name,
                message: result.description,
            })),
            Err(e) => Ok(Json(InstallResponse {
                ok: false,
                name: String::new(),
                message: e.to_string(),
            })),
        }
    } else {
        let installer = crate::skills::SkillInstaller::new();
        match installer.install(&req.source).await {
            Ok(result) => Ok(Json(InstallResponse {
                ok: true,
                name: result.name,
                message: result.description,
            })),
            Err(e) => Ok(Json(InstallResponse {
                ok: false,
                name: String::new(),
                message: e.to_string(),
            })),
        }
    }
}

// --- Search skills ---

#[derive(Deserialize)]
struct SkillSearchQuery {
    q: String,
}

#[derive(Serialize)]
struct SkillSearchResultView {
    name: String,
    description: String,
    source: String,
    downloads: u64,
    stars: u64,
}

async fn search_skills(
    Query(params): Query<SkillSearchQuery>,
) -> Json<Vec<SkillSearchResultView>> {
    let query = params.q.trim().to_string();
    if query.len() < 2 {
        return Json(Vec::new());
    }

    let query_ch = query.clone();
    let query_gh = query.clone();

    // Search ClawHub and GitHub in parallel (same pattern as TUI app.rs:1337)
    let (ch_result, gh_result) = tokio::join!(
        async {
            let installer = crate::skills::ClawHubInstaller::new();
            installer.search(&query_ch, 10).await
        },
        async {
            let searcher = crate::skills::search::SkillSearcher::new();
            searcher.search(&query_gh, 10).await
        }
    );

    let mut results: Vec<SkillSearchResultView> = Vec::new();

    // ClawHub results first (curated registry)
    match ch_result {
        Ok(items) => {
            results.extend(items.into_iter().map(|r| SkillSearchResultView {
                name: format!("clawhub:{}", r.slug),
                description: r.description,
                source: "clawhub".to_string(),
                downloads: r.downloads,
                stars: r.stars,
            }));
        }
        Err(e) => {
            tracing::warn!(error = %e, "ClawHub search failed, skipping");
        }
    }

    // GitHub results
    match gh_result {
        Ok(items) => {
            results.extend(items.into_iter().map(|r| SkillSearchResultView {
                name: r.full_name,
                description: r.description,
                source: "github".to_string(),
                downloads: 0,
                stars: r.stars as u64,
            }));
        }
        Err(e) => {
            tracing::warn!(error = %e, "GitHub skill search failed, skipping");
        }
    }

    Json(results)
}

// --- Delete skill ---

#[derive(Serialize)]
struct DeleteSkillResponse {
    ok: bool,
    message: String,
}

async fn delete_skill(Path(name): Path<String>) -> Json<DeleteSkillResponse> {
    match crate::skills::SkillInstaller::remove(&name).await {
        Ok(()) => Json(DeleteSkillResponse {
            ok: true,
            message: format!("Skill '{}' removed", name),
        }),
        Err(e) => Json(DeleteSkillResponse {
            ok: false,
            message: e.to_string(),
        }),
    }
}

// --- Providers ---

#[derive(Serialize)]
struct ProviderView {
    name: String,
    configured: bool,
    active: bool,
}

async fn list_providers(State(state): State<Arc<AppState>>) -> Json<Vec<ProviderView>> {
    let config = state.config.read().await;

    // Check secure storage for encrypted keys
    let secrets = crate::storage::global_secrets().ok();

    // Get current active provider
    let current_active = config.resolve_provider(&config.agent.model).map(|(n, _)| n);

    Json(
        config
            .providers
            .iter()
            .map(|(name, pc)| {
                // Check if API key exists in secure storage
                let has_encrypted_key = match &secrets {
                    Some(s) => {
                        let key = crate::storage::SecretKey::provider_api_key(name);
                        let result: std::result::Result<Option<String>, anyhow::Error> = s.get(&key);
                        matches!(result, Ok(Some(_)))
                    }
                    None => false,
                };

                // Provider is configured if:
                // 1. Has encrypted API key, OR
                // 2. Has custom base URL, OR
                // 3. Is a no-key provider (ollama, vllm, custom) AND is currently active
                let is_no_key_provider = matches!(name, "ollama" | "vllm" | "custom");
                let is_active = current_active.as_deref() == Some(name);
                let configured = has_encrypted_key
                    || pc.api_base.is_some()
                    || (is_no_key_provider && is_active);

                ProviderView {
                    name: name.to_string(),
                    configured,
                    active: is_active,
                }
            })
            .collect(),
    )
}

// --- Provider Configuration ---

#[derive(Deserialize)]
struct ProviderConfigRequest {
    name: String,
    api_key: Option<String>,
    api_base: Option<String>,
}

#[derive(Serialize)]
struct ProviderConfigResponse {
    ok: bool,
    message: String,
}

async fn configure_provider(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ProviderConfigRequest>,
) -> Result<Json<ProviderConfigResponse>, StatusCode> {
    let mut config = state.config.read().await.clone();

    // Get the provider config
    let provider = config
        .providers
        .get_mut(&req.name)
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Update API key in SECURE STORAGE (encrypted)
    if let Some(key) = &req.api_key {
        // Store API key in encrypted secrets storage
        let secrets = crate::storage::global_secrets()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let secret_key = crate::storage::SecretKey::provider_api_key(&req.name);
        secrets.set(&secret_key, key)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Store a marker in config (not the actual key)
        provider.api_key = if key.is_empty() { String::new() } else { "***ENCRYPTED***".to_string() };
    }

    // Update base URL in regular config (not sensitive)
    if let Some(base) = &req.api_base {
        provider.api_base = if base.is_empty() { None } else { Some(base.clone()) };
    }

    state.save_config(config).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ProviderConfigResponse {
        ok: true,
        message: format!("Provider '{}' configured (API key encrypted)", req.name),
    }))
}

#[derive(Deserialize)]
struct ActivateProviderRequest {
    name: String,
    model: Option<String>,
}

#[derive(Serialize)]
struct ActivateProviderResponse {
    ok: bool,
    message: String,
    model: String,
}

async fn activate_provider(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ActivateProviderRequest>,
) -> Result<Json<ActivateProviderResponse>, StatusCode> {
    let mut config = state.config.read().await.clone();

    // Check provider exists
    let provider = config
        .providers
        .get(&req.name)
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Ollama, vLLM, and custom don't require API key, only base URL (optional with defaults)
    let needs_api_key = !matches!(req.name.as_str(), "ollama" | "vllm" | "custom");

    // Check if API key exists in secure storage
    let has_encrypted_key = if needs_api_key {
        match crate::storage::global_secrets() {
            Ok(secrets) => {
                let key = crate::storage::SecretKey::provider_api_key(&req.name);
                let result: std::result::Result<Option<String>, anyhow::Error> = secrets.get(&key);
                matches!(result, Ok(Some(_)))
            }
            Err(_) => false,
        }
    } else {
        true // Doesn't need key
    };

    if needs_api_key && !has_encrypted_key && provider.api_base.is_none() {
        return Ok(Json(ActivateProviderResponse {
            ok: false,
            message: "Provider not configured. Set API key first.".to_string(),
            model: config.agent.model.clone(),
        }));
    }

    // Build the model string — ensure it has the provider prefix
    let model = match req.model {
        Some(m) if !m.is_empty() => {
            // Ensure the model has the provider prefix (e.g. "ollama/llama3:8b")
            let prefix = format!("{}/", req.name);
            if m.starts_with(&prefix) {
                m
            } else {
                format!("{}{}", prefix, m)
            }
        }
        _ => {
            // Default models per provider
            match req.name.as_str() {
                "anthropic" => "anthropic/claude-sonnet-4-20250514".to_string(),
                "openai" => "openai/gpt-4o".to_string(),
                "openrouter" => "openrouter/anthropic/claude-sonnet-4".to_string(),
                "ollama" => "ollama/llama3:8b".to_string(),
                "gemini" => "gemini/gemini-2.0-flash".to_string(),
                "deepseek" => "deepseek/deepseek-chat".to_string(),
                "groq" => "groq/llama-3.1-8b-instant".to_string(),
                _ => format!("{}/default", req.name),
            }
        }
    };

    // For local providers, ensure api_base is set with a sensible default
    if matches!(req.name.as_str(), "ollama" | "vllm" | "custom") {
        if let Some(pc) = config.providers.get_mut(&req.name) {
            if pc.api_base.is_none() {
                let default_base = match req.name.as_str() {
                    "ollama" => "http://localhost:11434/v1",
                    "vllm" => "http://localhost:8000/v1",
                    _ => "http://localhost:8080/v1",
                };
                pc.api_base = Some(default_base.to_string());
            }
        }
    }

    config.agent.model = model.clone();
    state.save_config(config).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ActivateProviderResponse {
        ok: true,
        message: format!("Provider '{}' activated", req.name),
        model,
    }))
}

#[derive(Deserialize)]
struct DeactivateRequest {
    name: String,
}

#[derive(Serialize)]
struct DeactivateResponse {
    ok: bool,
    message: String,
}

async fn deactivate_provider(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeactivateRequest>,
) -> Result<Json<DeactivateResponse>, StatusCode> {
    // Remove API key from encrypted storage
    if let Ok(secrets) = crate::storage::global_secrets() {
        let key = crate::storage::SecretKey::provider_api_key(&req.name);
        let _ = secrets.delete(&key);
    }

    let mut config = state.config.read().await.clone();

    // Clear the provider config
    if let Some(pc) = config.providers.get_mut(&req.name) {
        pc.api_key = String::new();
        // For local providers (ollama/vllm/custom), also clear base URL
        if matches!(req.name.as_str(), "ollama" | "vllm" | "custom") {
            pc.api_base = None;
        }
    }

    // If this was the active provider, clear the model to force re-selection
    let current_provider = config.resolve_provider(&config.agent.model)
        .map(|(n, _)| n.to_string());
    if current_provider.as_deref() == Some(req.name.as_str()) {
        config.agent.model = String::new();
    }

    state.save_config(config).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(DeactivateResponse {
        ok: true,
        message: format!("Provider '{}' deactivated and credentials removed", req.name),
    }))
}

// --- All Models (aggregated from configured providers) ---

/// Hardcoded popular models per cloud provider
fn cloud_models_for(provider: &str) -> &'static [&'static str] {
    match provider {
        "anthropic" => &[
            "anthropic/claude-sonnet-4-20250514",
            "anthropic/claude-opus-4-20250514",
            "anthropic/claude-haiku-4-20250514",
            "anthropic/claude-3-5-sonnet-20241022",
        ],
        "openai" => &[
            "openai/gpt-4o",
            "openai/gpt-4o-mini",
            "openai/o1",
            "openai/o3-mini",
        ],
        "gemini" => &[
            "gemini/gemini-2.0-flash",
            "gemini/gemini-2.0-pro",
            "gemini/gemini-1.5-pro",
        ],
        "deepseek" => &[
            "deepseek/deepseek-chat",
            "deepseek/deepseek-r1",
        ],
        "groq" => &[
            "groq/llama-3.3-70b-versatile",
            "groq/llama-3.1-8b-instant",
            "groq/mixtral-8x7b-32768",
        ],
        "openrouter" => &[
            "openrouter/anthropic/claude-sonnet-4",
            "openrouter/openai/gpt-4o",
            "openrouter/google/gemini-2.0-flash",
            "openrouter/meta-llama/llama-3.3-70b-instruct",
        ],
        "moonshot" => &["moonshot/moonshot-v1-8k"],
        "zhipu" => &["zhipu/glm-4"],
        "dashscope" => &["dashscope/qwen-plus"],
        "minimax" => &["minimax/MiniMax-M2"],
        "aihubmix" => &["aihubmix/claude-sonnet-4"],
        _ => &[],
    }
}

fn display_name_for(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "Anthropic",
        "openai" => "OpenAI",
        "openrouter" => "OpenRouter",
        "gemini" => "Gemini",
        "deepseek" => "DeepSeek",
        "groq" => "Groq",
        "moonshot" => "Moonshot",
        "zhipu" => "Zhipu",
        "dashscope" => "DashScope",
        "minimax" => "MiniMax",
        "aihubmix" => "AiHubMix",
        _ => "Unknown",
    }
}

#[derive(Serialize)]
struct ModelEntry {
    provider: String,
    model: String,
    label: String,
}

#[derive(Serialize)]
struct AllModelsResponse {
    ok: bool,
    models: Vec<ModelEntry>,
    current: String,
    ollama_configured: bool,
}

async fn list_all_models(State(state): State<Arc<AppState>>) -> Json<AllModelsResponse> {
    let config = state.config.read().await;
    let current_model = config.agent.model.clone();
    let secrets = crate::storage::global_secrets().ok();

    let mut models = Vec::new();
    let mut ollama_configured = false;

    for (name, pc) in config.providers.iter() {
        // Check if configured
        let has_key = match &secrets {
            Some(s) => {
                let key = crate::storage::SecretKey::provider_api_key(name);
                matches!(s.get(&key), Ok(Some(_)))
            }
            None => false,
        };
        let configured = has_key || !pc.api_key.is_empty() || pc.api_base.is_some();

        if !configured {
            continue;
        }

        // Local providers: skip from hardcoded list (JS fetches them live)
        if name == "ollama" {
            ollama_configured = true;
            continue;
        }
        if name == "vllm" || name == "custom" {
            continue;
        }

        let display = display_name_for(name);
        for model_id in cloud_models_for(name) {
            let short = model_id.split('/').last().unwrap_or(model_id);
            models.push(ModelEntry {
                provider: name.to_string(),
                model: model_id.to_string(),
                label: format!("{} / {}", display, short),
            });
        }
    }

    Json(AllModelsResponse {
        ok: true,
        models,
        current: current_model,
        ollama_configured,
    })
}

// --- Ollama Models ---

#[derive(Serialize)]
struct OllamaModel {
    name: String,
    size: String,
    modified: String,
}

#[derive(Serialize)]
struct OllamaModelsResponse {
    ok: bool,
    models: Vec<OllamaModel>,
    error: Option<String>,
}

async fn list_ollama_models(
    State(state): State<Arc<AppState>>,
) -> Json<OllamaModelsResponse> {
    let config = state.config.read().await;

    // Get Ollama base URL — strip /v1 suffix since native API doesn't use it
    let raw_base = config
        .providers
        .get("ollama")
        .and_then(|p| p.api_base.as_deref())
        .unwrap_or("http://localhost:11434");
    let base_url = raw_base.trim_end_matches('/').trim_end_matches("/v1");

    let url = format!("{}/api/tags", base_url);
    // Drop config lock before doing network I/O
    drop(config);

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return Json(OllamaModelsResponse {
                ok: false,
                models: vec![],
                error: Some("Failed to create HTTP client".to_string()),
            });
        }
    };

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            #[derive(Deserialize)]
            struct OllamaApiResponse {
                models: Vec<OllamaApiModel>,
            }
            #[derive(Deserialize)]
            struct OllamaApiModel {
                name: String,
                size: Option<u64>,
                modified_at: Option<String>,
            }

            match resp.json::<OllamaApiResponse>().await {
                Ok(data) => {
                    let models = data
                        .models
                        .into_iter()
                        .map(|m| OllamaModel {
                            name: m.name,
                            size: m
                                .size
                                .map(|s| format!("{:.1} GB", s as f64 / 1e9))
                                .unwrap_or_else(|| "Unknown".to_string()),
                            modified: m.modified_at.unwrap_or_else(|| "Unknown".to_string()),
                        })
                        .collect();

                    Json(OllamaModelsResponse {
                        ok: true,
                        models,
                        error: None,
                    })
                }
                Err(e) => Json(OllamaModelsResponse {
                    ok: false,
                    models: vec![],
                    error: Some(format!("Failed to parse Ollama response: {}", e)),
                }),
            }
        }
        Ok(resp) => Json(OllamaModelsResponse {
            ok: false,
            models: vec![],
            error: Some(format!("Ollama returned status {}", resp.status())),
        }),
        Err(e) => Json(OllamaModelsResponse {
            ok: false,
            models: vec![],
            error: Some(format!("Cannot connect to Ollama: {}. Is Ollama running?", e)),
        }),
    }
}

// ═══════════════════════════════════════════════════
// Channels — get, configure, deactivate, test
// ═══════════════════════════════════════════════════

/// Get current channel configuration (tokens masked).
async fn get_channel(
    axum::extract::Path(name): axum::extract::Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let config = state.config.read().await;

    // Helper: mask a token showing only last 4 chars
    fn mask_token(token: &str) -> String {
        if token.is_empty() || token == "***ENCRYPTED***" {
            return String::new();
        }
        if token.len() <= 4 {
            return "••••".to_string();
        }
        format!("{}{}",
            "•".repeat(token.len().min(20) - 4),
            &token[token.len() - 4..])
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
                    secrets.set(&key, token)
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
                    secrets.set(&key, token)
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
        "whatsapp" => {
            if let Some(phone) = &req.phone_number {
                config.channels.whatsapp.phone_number = phone.clone();
            }
            if let Some(allow_from) = &req.allow_from {
                config.channels.whatsapp.allow_from = allow_from.clone();
            }
            // Don't set enabled here — WhatsApp needs pairing first
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
        _ => return Err(StatusCode::BAD_REQUEST),
    }

    state.save_config(config).await
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
    if matches!(req.name.as_str(), "telegram" | "discord") {
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
            // Keep phone_number and db_path — user might re-pair
        }
        "web" => {
            // Web cannot be fully deactivated
            return Ok(Json(ChannelConfigResponse {
                ok: false,
                message: "Web UI cannot be deactivated".to_string(),
            }));
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    }

    state.save_config(config).await
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
                if !t.is_empty() { Some(t.clone()) } else { None }
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
                            let name = tg.result
                                .map(|u| u.username.unwrap_or_else(|| u.first_name.unwrap_or_default()))
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
                    message: "Token too short — check your Discord bot token".to_string(),
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
                    message: "Session exists — WhatsApp is paired".to_string(),
                })
            } else if has_phone {
                Json(ChannelTestResponse {
                    ok: false,
                    message: "Phone configured but not paired yet".to_string(),
                })
            } else {
                Json(ChannelTestResponse {
                    ok: false,
                    message: "Not configured — enter phone number and pair".to_string(),
                })
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
// WhatsApp Pairing — WebSocket endpoint
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
                let err = serde_json::json!({"type": "error", "message": "Send {\"phone\": \"number\"}"});
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
    // Note: wa-rs now handles stale sessions internally — when with_pair_code() is set,
    // it clears the device identity so the handshake uses registration instead of login.
    // Bridge events from wa-rs callback → mpsc → WebSocket
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<serde_json::Value>(16);

    let pair_phone = phone.clone();
    let db_path_str = db_path.to_string_lossy().to_string();

    let bot_handle = tokio::spawn(async move {
        run_whatsapp_pair_bot(pair_phone, db_path_str, event_tx).await
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
async fn run_whatsapp_pair_bot(
    phone: String,
    db_path: String,
    event_tx: tokio::sync::mpsc::Sender<serde_json::Value>,
) {
    use wa_rs::bot::Bot;
    use wa_rs::store::SqliteStore;
    use wa_rs_tokio_transport::TokioWebSocketTransportFactory;
    use wa_rs_ureq_http::UreqHttpClient;
    use wa_rs_core::types::events::Event as WaEvent;
    use wa_rs_proto::whatsapp as wa;

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
