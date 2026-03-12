use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use super::super::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/tools", get(list_tools))
        .route("/v1/providers", get(list_providers))
        .route(
            "/v1/providers/configure",
            axum::routing::post(configure_provider),
        )
        .route(
            "/v1/providers/activate",
            axum::routing::post(activate_provider),
        )
        .route(
            "/v1/providers/deactivate",
            axum::routing::post(deactivate_provider),
        )
        .route("/v1/providers/test", axum::routing::post(test_provider))
        .route("/v1/providers/health", get(providers_health))
        .route("/v1/providers/models", get(list_all_models))
        .route(
            "/v1/providers/model-capabilities",
            axum::routing::post(resolve_model_capabilities),
        )
        .route("/v1/providers/ollama/models", get(list_ollama_models))
        .route(
            "/v1/providers/ollama-cloud/models",
            get(list_ollama_cloud_models),
        )
}

#[derive(Serialize)]
struct ProviderView {
    name: String,
    configured: bool,
    active: bool,
}

// --- Provider Health ---

async fn providers_health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    match state.health_tracker.as_ref() {
        Some(tracker) => {
            let snapshots = tracker.snapshots();
            Json(serde_json::json!({ "providers": snapshots }))
        }
        None => Json(serde_json::json!({ "providers": [] })),
    }
}

/// List all registered tools and their availability status.
async fn list_tools(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let mut tools = Vec::new();

    // Built-in tools from registry (include descriptions + param schema for UI)
    if let Some(ref registry) = state.tool_registry {
        let guard = registry.read().await;
        for (name, description, parameters) in guard.tool_info() {
            tools.push(serde_json::json!({
                "name": name,
                "description": description,
                "parameters": parameters,
                "source": "builtin",
                "available": true,
            }));
        }
    }

    // Check what's missing and why
    let config = state.config.read().await;
    let mut missing = Vec::new();

    if config.tools.web_search.api_key.is_empty() {
        missing.push(serde_json::json!({
            "name": "web_search",
            "reason": "No Brave Search API key configured. Set [tools.web_search] api_key in config or get a free key at https://brave.com/search/api/",
        }));
    }
    if !config.browser.enabled {
        missing.push(serde_json::json!({
            "name": "browser",
            "reason": "Browser disabled in config. Set [browser] enabled = true",
        }));
    } else {
        // Browser enabled in config but check if it actually registered
        let has_browser = tools
            .iter()
            .any(|t| t.get("name").and_then(|n| n.as_str()) == Some("browser"));
        if !has_browser {
            missing.push(serde_json::json!({
                "name": "browser",
                "reason": "Browser enabled in config but MCP bridge not connected. \
                           Check that @playwright/mcp is installed (npx @playwright/mcp --help). \
                           Browser tools register ~23s after gateway startup.",
            }));
        }
    }

    Json(serde_json::json!({
        "ok": true,
        "tools": tools,
        "missing": missing,
        "total": tools.len(),
    }))
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
                        let result: std::result::Result<Option<String>, anyhow::Error> =
                            s.get(&key);
                        matches!(result, Ok(Some(_)))
                    }
                    None => false,
                };

                // Provider is configured if:
                // 1. Has encrypted API key, OR
                // 2. Has custom base URL, OR
                // 3. Is a no-key provider (ollama, vllm, custom) AND is currently active
                let is_no_key_provider = matches!(name, "ollama" | "vllm" | "custom");
                let is_active = current_active == Some(name);
                let configured =
                    has_encrypted_key || pc.api_base.is_some() || (is_no_key_provider && is_active);

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
        let secrets =
            crate::storage::global_secrets().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let secret_key = crate::storage::SecretKey::provider_api_key(&req.name);
        secrets
            .set(&secret_key, key)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Store a marker in config (not the actual key)
        provider.api_key = if key.is_empty() {
            String::new()
        } else {
            "***ENCRYPTED***".to_string()
        };
    }

    // Update base URL in regular config (not sensitive)
    if let Some(base) = &req.api_base {
        provider.api_base = if base.is_empty() {
            None
        } else {
            Some(base.clone())
        };
    }

    state
        .save_config(config)
        .await
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
    state
        .save_config(config)
        .await
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

    // Clear the provider config completely
    if let Some(pc) = config.providers.get_mut(&req.name) {
        pc.api_key = String::new();
        pc.api_base = None; // Clear base URL for ALL providers
    }

    // If this was the active provider, clear the model to force re-selection
    let current_provider = config
        .resolve_provider(&config.agent.model)
        .map(|(n, _)| n.to_string());
    if current_provider.as_deref() == Some(req.name.as_str()) {
        config.agent.model = String::new();
    }

    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(DeactivateResponse {
        ok: true,
        message: format!(
            "Provider '{}' deactivated and credentials removed",
            req.name
        ),
    }))
}

#[derive(Deserialize)]
struct ProviderTestRequest {
    name: String,
    model: Option<String>,
    api_key: Option<String>,
    api_base: Option<String>,
    timeout_secs: Option<u64>,
}

#[derive(Serialize)]
struct ProviderTestResponse {
    ok: bool,
    message: String,
    provider: String,
    model: String,
    latency_ms: Option<u128>,
}

fn default_model_for_provider(provider: &str) -> String {
    match provider {
        "anthropic" => "anthropic/claude-sonnet-4-20250514".to_string(),
        "openai" => "openai/gpt-4o-mini".to_string(),
        "openrouter" => "openrouter/anthropic/claude-sonnet-4".to_string(),
        "ollama" => "ollama/llama3:8b".to_string(),
        "ollama_cloud" => "ollama_cloud/llama3.3".to_string(),
        "gemini" => "gemini/gemini-2.0-flash".to_string(),
        "deepseek" => "deepseek/deepseek-chat".to_string(),
        "groq" => "groq/llama-3.1-8b-instant".to_string(),
        "mistral" => "mistral/mistral-small-latest".to_string(),
        "xai" => "xai/grok-beta".to_string(),
        "together" => "together/meta-llama/Llama-3.3-70B-Instruct-Turbo".to_string(),
        "fireworks" => "fireworks/accounts/fireworks/models/llama-v3p3-70b-instruct".to_string(),
        "perplexity" => "perplexity/sonar-pro".to_string(),
        "cohere" => "cohere/command-r".to_string(),
        "venice" => "venice/llama-3.3-70b".to_string(),
        "aihubmix" => "aihubmix/claude-sonnet-4".to_string(),
        "vercel" => "vercel/claude-3-5-sonnet".to_string(),
        "cloudflare" => "cloudflare/@cf/meta/llama-3.3-70b-instruct".to_string(),
        "copilot" => "copilot/gpt-4o".to_string(),
        "bedrock" => "bedrock/anthropic.claude-3-sonnet".to_string(),
        "moonshot" => "moonshot/moonshot-v1-8k".to_string(),
        "zhipu" => "zhipu/glm-4".to_string(),
        "dashscope" => "dashscope/qwen-plus".to_string(),
        "minimax" => "minimax/MiniMax-M2".to_string(),
        "vllm" => "vllm/default".to_string(),
        "custom" => "custom/default".to_string(),
        _ => format!("{provider}/default"),
    }
}

async fn test_provider(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ProviderTestRequest>,
) -> Json<ProviderTestResponse> {
    let mut config = state.config.read().await.clone();

    // Provider must exist in config.
    let Some(provider_cfg) = config.providers.get_mut(&req.name) else {
        return Json(ProviderTestResponse {
            ok: false,
            message: format!("Unknown provider '{}'", req.name),
            provider: req.name,
            model: String::new(),
            latency_ms: None,
        });
    };

    // Apply temporary overrides from the form (without persisting them).
    if let Some(api_key) = req.api_key.as_ref() {
        provider_cfg.api_key = api_key.clone();
    }
    if let Some(api_base) = req.api_base.as_ref() {
        provider_cfg.api_base = if api_base.trim().is_empty() {
            None
        } else {
            Some(api_base.trim().to_string())
        };
    }

    let model = match req.model.as_deref() {
        Some(m) if !m.trim().is_empty() => {
            let m = m.trim();
            let prefix = format!("{}/", req.name);
            if m.starts_with(&prefix) {
                m.to_string()
            } else {
                format!("{prefix}{m}")
            }
        }
        _ => {
            let current = config.agent.model.clone();
            let expected_prefix = format!("{}/", req.name);
            if current.starts_with(&expected_prefix) {
                current
            } else {
                default_model_for_provider(&req.name)
            }
        }
    };

    let provider = match crate::provider::create_single_provider(&config, &model) {
        Ok((_, p)) => p,
        Err(e) => {
            return Json(ProviderTestResponse {
                ok: false,
                message: format!("Provider setup failed: {e}"),
                provider: req.name,
                model,
                latency_ms: None,
            });
        }
    };

    let timeout_secs = req.timeout_secs.unwrap_or(20).clamp(3, 60);
    let started = Instant::now();
    let chat_req = crate::provider::ChatRequest {
        messages: vec![
            crate::provider::ChatMessage::system(
                "Reply with exactly 'ok'. No extra text, no tools.",
            ),
            crate::provider::ChatMessage::user("connection test"),
        ],
        tools: vec![],
        model: model.clone(),
        max_tokens: 12,
        temperature: 0.0,
        // Disable thinking for connection test — reasoning models would consume
        // the entire 12-token budget on thinking blocks, returning no text.
        think: Some(false),
    };

    let result =
        tokio::time::timeout(Duration::from_secs(timeout_secs), provider.chat(chat_req)).await;

    match result {
        Ok(Ok(resp)) => {
            let latency_ms = started.elapsed().as_millis();
            let preview = resp
                .content
                .unwrap_or_default()
                .trim()
                .chars()
                .take(80)
                .collect::<String>();
            Json(ProviderTestResponse {
                ok: true,
                message: if preview.is_empty() {
                    format!(
                        "Connection OK (no text content, finish={})",
                        resp.finish_reason
                    )
                } else {
                    format!("Connection OK: {}", preview)
                },
                provider: req.name,
                model,
                latency_ms: Some(latency_ms),
            })
        }
        Ok(Err(e)) => Json(ProviderTestResponse {
            ok: false,
            message: format!("Connection failed: {e}"),
            provider: req.name,
            model,
            latency_ms: None,
        }),
        Err(_) => Json(ProviderTestResponse {
            ok: false,
            message: format!("Connection timed out after {timeout_secs}s"),
            provider: req.name,
            model,
            latency_ms: None,
        }),
    }
}

// --- All Models (aggregated from configured providers) ---

/// Hardcoded popular models per cloud provider
fn cloud_models_for(provider: &str) -> &'static [&'static str] {
    match provider {
        // Primary providers
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
        "openrouter" => &[
            "openrouter/anthropic/claude-sonnet-4",
            "openrouter/openai/gpt-4o",
            "openrouter/google/gemini-2.0-flash",
            "openrouter/meta-llama/llama-3.3-70b-instruct",
        ],
        // Cloud providers
        "deepseek" => &["deepseek/deepseek-chat", "deepseek/deepseek-reasoner"],
        "groq" => &[
            "groq/llama-3.3-70b-versatile",
            "groq/llama-3.1-8b-instant",
            "groq/mixtral-8x7b-32768",
        ],
        "mistral" => &[
            "mistral/mistral-large-latest",
            "mistral/mistral-small-latest",
            "mistral/codestral-latest",
        ],
        "xai" => &["xai/grok-beta"],
        "together" => &[
            "together/meta-llama/Llama-3.3-70B-Instruct-Turbo",
            "together/mistralai/Mixtral-8x7B-Instruct-v0.1",
        ],
        "fireworks" => &[
            "fireworks/accounts/fireworks/models/llama-v3p3-70b-instruct",
            "fireworks/accounts/fireworks/models/qwen2p5-72b-instruct",
        ],
        "perplexity" => &["perplexity/sonar-pro", "perplexity/sonar-reasoning-pro"],
        "cohere" => &["cohere/command-r-plus", "cohere/command-r"],
        "venice" => &["venice/llama-3.3-70b"],
        // Gateways
        "aihubmix" => &["aihubmix/claude-sonnet-4"],
        "vercel" => &["vercel/claude-3-5-sonnet"],
        "cloudflare" => &["cloudflare/@cf/meta/llama-3.3-70b-instruct"],
        "copilot" => &["copilot/gpt-4o"],
        "bedrock" => &["bedrock/anthropic.claude-3-sonnet"],
        "ollama_cloud" => &["ollama_cloud/llama3.3", "ollama_cloud/mistral"],
        // Chinese providers
        "moonshot" => &["moonshot/moonshot-v1-8k"],
        "zhipu" => &["zhipu/glm-4"],
        "dashscope" => &["dashscope/qwen-plus"],
        "minimax" => &["minimax/MiniMax-M2"],
        _ => &[],
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
    vision_model: String,
    ollama_configured: bool,
    ollama_cloud_configured: bool,
    hidden_models: HashMap<String, Vec<String>>,
    model_overrides: HashMap<String, crate::config::ModelOverrides>,
    model_capabilities: HashMap<String, crate::config::ModelCapabilities>,
    effective_model_capabilities: HashMap<String, crate::config::ModelCapabilities>,
}

async fn list_all_models(State(state): State<Arc<AppState>>) -> Json<AllModelsResponse> {
    let config = state.config.read().await;
    let current_model = config.agent.model.clone();
    let vision_model = config.agent.vision_model.clone();
    let secrets = crate::storage::global_secrets().ok();

    let mut models = Vec::new();
    let mut ollama_configured = false;
    let mut ollama_cloud_configured = false;

    for (name, pc) in config.providers.iter() {
        // Special handling for providers that don't require API keys
        if name == "ollama" {
            // Ollama local is always potentially available (runs at localhost:11434)
            // Mark as configured so JS can try to fetch models
            ollama_configured = true;
            continue;
        }

        // Check if configured (has API key or base URL)
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

        // Local/cloud providers with dynamic model lists: skip hardcoded, JS fetches live
        if name == "ollama_cloud" {
            ollama_cloud_configured = true;
            continue;
        }
        if name == "vllm" || name == "custom" {
            continue;
        }

        for model_id in cloud_models_for(name) {
            // Skip models hidden by the user
            if pc.hidden_models.contains(&model_id.to_string()) {
                continue;
            }
            // Strip provider prefix: "openrouter/anthropic/claude-sonnet-4" -> "anthropic/claude-sonnet-4"
            let label = model_id
                .strip_prefix(name)
                .and_then(|s| s.strip_prefix('/'))
                .unwrap_or(model_id)
                .to_string();
            models.push(ModelEntry {
                provider: name.to_string(),
                model: model_id.to_string(),
                label,
            });
        }
    }

    // Collect hidden models per configured provider
    let mut hidden_models = HashMap::new();
    for (name, pc) in config.providers.iter() {
        if !pc.hidden_models.is_empty() {
            hidden_models.insert(name.to_string(), pc.hidden_models.clone());
        }
    }

    let mut capability_models = HashSet::new();
    for entry in &models {
        capability_models.insert(entry.model.clone());
    }
    if !current_model.is_empty() {
        capability_models.insert(current_model.clone());
    }
    if !vision_model.is_empty() {
        capability_models.insert(vision_model.clone());
    }
    for model in &config.agent.fallback_models {
        capability_models.insert(model.clone());
    }
    for model in config.agent.model_overrides.keys() {
        capability_models.insert(model.clone());
    }
    let model_capabilities =
        build_model_capabilities_map(&config, capability_models.clone().into_iter(), false);
    let effective_model_capabilities =
        build_model_capabilities_map(&config, capability_models.into_iter(), true);

    Json(AllModelsResponse {
        ok: true,
        models,
        current: current_model,
        vision_model,
        ollama_configured,
        ollama_cloud_configured,
        hidden_models,
        model_overrides: config.agent.model_overrides.clone(),
        model_capabilities,
        effective_model_capabilities,
    })
}

#[derive(Deserialize)]
struct ModelCapabilitiesRequest {
    models: Vec<String>,
    #[serde(default)]
    apply_overrides: bool,
}

#[derive(Serialize)]
struct ModelCapabilitiesResponse {
    ok: bool,
    model_capabilities: HashMap<String, crate::config::ModelCapabilities>,
}

fn build_model_capabilities_map<I>(
    config: &crate::config::Config,
    models: I,
    apply_overrides: bool,
) -> HashMap<String, crate::config::ModelCapabilities>
where
    I: IntoIterator<Item = String>,
{
    let mut result = HashMap::new();
    for model in models {
        if model.trim().is_empty() {
            continue;
        }
        let provider_name = config
            .resolve_provider(&model)
            .map(|(name, _)| name)
            .unwrap_or("unknown");
        result.insert(
            model.clone(),
            if apply_overrides {
                config
                    .agent
                    .effective_model_capabilities(provider_name, &model)
            } else {
                crate::provider::capabilities::detect_model_capabilities(provider_name, &model)
            },
        );
    }
    result
}

async fn resolve_model_capabilities(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ModelCapabilitiesRequest>,
) -> Json<ModelCapabilitiesResponse> {
    let config = state.config.read().await;
    Json(ModelCapabilitiesResponse {
        ok: true,
        model_capabilities: build_model_capabilities_map(
            &config,
            payload.models.into_iter(),
            payload.apply_overrides,
        ),
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

async fn list_ollama_models(State(state): State<Arc<AppState>>) -> Json<OllamaModelsResponse> {
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
            error: Some(format!(
                "Cannot connect to Ollama: {}. Is Ollama running?",
                e
            )),
        }),
    }
}

/// List available models from Ollama Cloud (uses same /api/tags format as local Ollama)
#[derive(Serialize)]
struct OllamaCloudModelsResponse {
    ok: bool,
    models: Vec<OllamaCloudModel>,
    error: Option<String>,
}

#[derive(Serialize)]
struct OllamaCloudModel {
    id: String,
    owned_by: String,
}

async fn list_ollama_cloud_models(
    State(state): State<Arc<AppState>>,
) -> Json<OllamaCloudModelsResponse> {
    let config = state.config.read().await;

    // Get Ollama Cloud API key and base URL
    let api_key = if let Some(pc) = config.providers.get("ollama_cloud") {
        if pc.api_key == "***ENCRYPTED***" {
            // Retrieve from secure storage
            let secrets = match crate::storage::global_secrets() {
                Ok(s) => s,
                Err(_) => {
                    return Json(OllamaCloudModelsResponse {
                        ok: false,
                        models: vec![],
                        error: Some("Failed to access secure storage".to_string()),
                    });
                }
            };
            let key = crate::storage::SecretKey::provider_api_key("ollama_cloud");
            match secrets.get(&key) {
                Ok(Some(k)) => k,
                _ => {
                    return Json(OllamaCloudModelsResponse {
                        ok: false,
                        models: vec![],
                        error: Some("Ollama Cloud API key not found in vault".to_string()),
                    });
                }
            }
        } else if pc.api_key.is_empty() {
            return Json(OllamaCloudModelsResponse {
                ok: false,
                models: vec![],
                error: Some("Ollama Cloud API key not configured".to_string()),
            });
        } else {
            pc.api_key.clone()
        }
    } else {
        return Json(OllamaCloudModelsResponse {
            ok: false,
            models: vec![],
            error: Some("Ollama Cloud provider not configured".to_string()),
        });
    };

    // Ollama Cloud uses https://ollama.com with /api/tags (same as local)
    let base_url = config
        .providers
        .get("ollama_cloud")
        .and_then(|p| p.api_base.as_deref())
        .unwrap_or("https://ollama.com")
        .trim_end_matches('/')
        .to_string();

    drop(config);

    // Use /api/tags endpoint (same format as local Ollama)
    let url = format!("{}/api/tags", base_url);

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return Json(OllamaCloudModelsResponse {
                ok: false,
                models: vec![],
                error: Some("Failed to create HTTP client".to_string()),
            });
        }
    };

    match client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            // Parse same format as local Ollama
            #[derive(Deserialize)]
            struct OllamaApiResponse {
                models: Vec<OllamaApiModel>,
            }
            #[derive(Deserialize)]
            struct OllamaApiModel {
                name: String,
            }

            match resp.json::<OllamaApiResponse>().await {
                Ok(data) => {
                    let models = data
                        .models
                        .into_iter()
                        .map(|m| OllamaCloudModel {
                            id: m.name,
                            owned_by: "ollama".to_string(),
                        })
                        .collect();

                    Json(OllamaCloudModelsResponse {
                        ok: true,
                        models,
                        error: None,
                    })
                }
                Err(e) => Json(OllamaCloudModelsResponse {
                    ok: false,
                    models: vec![],
                    error: Some(format!("Failed to parse response: {}", e)),
                }),
            }
        }
        Ok(resp) => Json(OllamaCloudModelsResponse {
            ok: false,
            models: vec![],
            error: Some(format!("API returned status {}", resp.status())),
        }),
        Err(e) => Json(OllamaCloudModelsResponse {
            ok: false,
            models: vec![],
            error: Some(format!("Connection failed: {}", e)),
        }),
    }
}
