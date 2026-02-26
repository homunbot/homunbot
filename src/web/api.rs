use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use super::server::AppState;

pub fn router() -> Router<Arc<AppState>> {
    let api_router = Router::new()
        .route("/health", get(health))
        .route("/v1/status", get(status))
        .route("/v1/config", get(get_config))
        .route("/v1/config", axum::routing::patch(patch_config))
        .route("/v1/skills", get(list_skills))
        .route("/v1/skills/search", get(search_skills))
        .route("/v1/skills/install", axum::routing::post(install_skill))
        .route(
            "/v1/skills/{name}",
            get(get_skill_detail).delete(delete_skill),
        )
        .route("/v1/skills/catalog/status", get(catalog_status))
        .route("/v1/skills/catalog/counts", get(catalog_counts))
        .route(
            "/v1/skills/catalog/refresh",
            axum::routing::post(catalog_refresh),
        )
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
        .route("/v1/providers/models", get(list_all_models))
        .route("/v1/providers/ollama/models", get(list_ollama_models))
        .route(
            "/v1/providers/ollama-cloud/models",
            get(list_ollama_cloud_models),
        )
        // --- Channels ---
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
        // --- Webhook Ingress ---
        .route("/v1/webhook/{token}", axum::routing::post(webhook_ingress))
        // --- Account ---
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
        // --- Memory ---
        .route("/v1/memory/stats", get(memory_stats))
        .route(
            "/v1/memory/content",
            get(get_memory_file).put(put_memory_file),
        )
        .route("/v1/memory/search", get(search_memory))
        .route("/v1/memory/history", get(get_memory_history))
        .route(
            "/v1/memory/instructions",
            get(get_instructions).put(put_instructions),
        )
        .route("/v1/memory/daily", get(list_daily_files))
        .route("/v1/memory/daily/{date}", get(get_daily_file))
        .route(
            "/v1/memory/cleanup",
            axum::routing::post(run_memory_cleanup),
        )
        // --- Chat ---
        .route(
            "/v1/chat/history",
            get(chat_history).delete(clear_chat_history),
        )
        .route("/v1/chat/compact", axum::routing::post(compact_chat))
        // --- Vault ---
        .route("/v1/vault", get(list_vault_keys).post(set_vault_secret))
        .route(
            "/v1/vault/{key}/reveal",
            axum::routing::post(reveal_vault_secret),
        )
        .route(
            "/v1/vault/{key}",
            axum::routing::delete(delete_vault_secret),
        )
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
        // --- Permissions ---
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
        // --- Approvals (P0-4) ---
        .route("/v1/approvals", get(list_approvals))
        .route("/v1/approvals/pending", get(list_pending_approvals))
        .route("/v1/approvals/audit", get(get_approval_audit_log))
        .route(
            "/v1/approvals/{id}/approve",
            axum::routing::post(approve_request),
        )
        .route("/v1/approvals/{id}/deny", axum::routing::post(deny_request))
        .route(
            "/v1/approvals/config",
            get(get_approval_config).put(put_approval_config),
        );

    // --- Browser (optional) ---
    #[cfg(feature = "browser")]
    let api_router = api_router.route("/v1/browser/test", axum::routing::post(test_browser));

    api_router
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
    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        serde_json::json!({"ok": true, "key": patch.key, "value": patch.value}),
    ))
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
    } else if path.join(".openskills-source").exists() {
        "openskills".to_string()
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
    let result = if let Some(slug) = req.source.strip_prefix("clawhub:") {
        let hub = crate::skills::ClawHubInstaller::new();
        hub.install(slug).await
    } else if let Some(dir_name) = req.source.strip_prefix("openskills:") {
        let source = crate::skills::OpenSkillsSource::new();
        source.install(dir_name).await
    } else {
        let installer = crate::skills::SkillInstaller::new();
        installer.install(&req.source).await
    };

    match result {
        Ok(r) => Ok(Json(InstallResponse {
            ok: true,
            name: r.name,
            message: r.description,
        })),
        Err(e) => Ok(Json(InstallResponse {
            ok: false,
            name: String::new(),
            message: e.to_string(),
        })),
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

async fn search_skills(Query(params): Query<SkillSearchQuery>) -> Json<Vec<SkillSearchResultView>> {
    let query = params.q.trim().to_string();
    if query.len() < 2 {
        return Json(Vec::new());
    }

    let query_ch = query.clone();
    let query_gh = query.clone();
    let query_os = query.clone();

    // Search ClawHub, GitHub, and Open Skills in parallel
    let (ch_result, gh_result, os_result) = tokio::join!(
        async {
            let installer = crate::skills::ClawHubInstaller::new();
            installer.search(&query_ch, 10).await
        },
        async {
            let searcher = crate::skills::search::SkillSearcher::new();
            searcher.search(&query_gh, 10).await
        },
        async {
            let source = crate::skills::OpenSkillsSource::new();
            source.search(&query_os, 10).await
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

    // Open Skills results (community curated)
    match os_result {
        Ok(items) => {
            results.extend(items.into_iter().map(|r| SkillSearchResultView {
                name: r.source,
                description: r.description,
                source: "openskills".to_string(),
                downloads: 0,
                stars: 0,
            }));
        }
        Err(e) => {
            tracing::warn!(error = %e, "Open Skills search failed, skipping");
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

// --- Catalog cache ---

#[derive(Serialize)]
struct CatalogStatusResponse {
    cached: bool,
    stale: bool,
    skill_count: usize,
    age_secs: u64,
}

async fn catalog_status() -> Json<CatalogStatusResponse> {
    let cache_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".homun")
        .join("clawhub-catalog.json");

    if !cache_path.exists() {
        return Json(CatalogStatusResponse {
            cached: false,
            stale: true,
            skill_count: 0,
            age_secs: 0,
        });
    }

    // Read and parse the cache to get metadata
    match tokio::fs::read_to_string(&cache_path).await {
        Ok(content) => {
            #[derive(Deserialize)]
            struct Cache {
                fetched_at: u64,
                entries: Vec<serde_json::Value>,
            }
            match serde_json::from_str::<Cache>(&content) {
                Ok(cache) => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let age = now.saturating_sub(cache.fetched_at);
                    Json(CatalogStatusResponse {
                        cached: true,
                        stale: age > 6 * 3600,
                        skill_count: cache.entries.len(),
                        age_secs: age,
                    })
                }
                Err(_) => Json(CatalogStatusResponse {
                    cached: false,
                    stale: true,
                    skill_count: 0,
                    age_secs: 0,
                }),
            }
        }
        Err(_) => Json(CatalogStatusResponse {
            cached: false,
            stale: true,
            skill_count: 0,
            age_secs: 0,
        }),
    }
}

#[derive(Serialize)]
struct CatalogRefreshResponse {
    ok: bool,
    skill_count: usize,
    message: String,
}

async fn catalog_refresh() -> Json<CatalogRefreshResponse> {
    let installer = crate::skills::ClawHubInstaller::new();
    match installer.refresh_catalog_cache().await {
        Ok(()) => {
            // Read back the count from cache
            let cache_path = dirs::home_dir()
                .unwrap_or_default()
                .join(".homun")
                .join("clawhub-catalog.json");
            let count = tokio::fs::read_to_string(&cache_path)
                .await
                .ok()
                .and_then(|c| {
                    #[derive(Deserialize)]
                    struct Cache {
                        entries: Vec<serde_json::Value>,
                    }
                    serde_json::from_str::<Cache>(&c).ok()
                })
                .map(|c| c.entries.len())
                .unwrap_or(0);

            Json(CatalogRefreshResponse {
                ok: true,
                skill_count: count,
                message: format!("{} skills cached", count),
            })
        }
        Err(e) => Json(CatalogRefreshResponse {
            ok: false,
            skill_count: 0,
            message: e.to_string(),
        }),
    }
}

// --- Catalog counts (all sources) ---

#[derive(Serialize)]
struct CatalogCountsResponse {
    clawhub: usize,
    github: usize,
    openskills: usize,
}

async fn catalog_counts() -> Json<CatalogCountsResponse> {
    let home = dirs::home_dir().unwrap_or_default().join(".homun");

    // ClawHub count from catalog cache
    let clawhub = tokio::fs::read_to_string(home.join("clawhub-catalog.json"))
        .await
        .ok()
        .and_then(|c| {
            #[derive(Deserialize)]
            struct Cache {
                entries: Vec<serde_json::Value>,
            }
            serde_json::from_str::<Cache>(&c).ok()
        })
        .map(|c| c.entries.len())
        .unwrap_or(0);

    // Open Skills count from their cache
    let openskills = tokio::fs::read_to_string(home.join("openskills-catalog.json"))
        .await
        .ok()
        .and_then(|c| {
            #[derive(Deserialize)]
            struct Cache {
                entries: Vec<serde_json::Value>,
            }
            serde_json::from_str::<Cache>(&c).ok()
        })
        .map(|c| c.entries.len())
        .unwrap_or(0);

    // GitHub: no catalog cache, just show a generic number
    // (GitHub has millions of repos, we indicate "∞" on the client)
    let github = 0; // client will show "GitHub" without a number

    Json(CatalogCountsResponse {
        clawhub,
        github,
        openskills,
    })
}

// --- Skill detail ---

#[derive(Serialize)]
struct SkillDetailView {
    name: String,
    description: String,
    path: String,
    source: String,
    /// SKILL.md rendered to HTML via pulldown-cmark
    content_html: String,
    scripts: Vec<String>,
}

/// Strip YAML frontmatter from a SKILL.md string, returning just the body.
fn strip_frontmatter(md: &str) -> &str {
    if let Some(rest) = md.strip_prefix("---\n") {
        if let Some((_fm, body)) = rest.split_once("\n---") {
            // Skip the closing "---" line and any leading newline
            return body.strip_prefix('\n').unwrap_or(body);
        }
    }
    md
}

/// Render markdown to HTML using pulldown-cmark.
fn render_md_to_html(md: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(md, opts);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

async fn get_skill_detail(Path(name): Path<String>) -> Result<Json<SkillDetailView>, StatusCode> {
    let skills_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".homun")
        .join("skills");
    let skill_dir = skills_dir.join(&name);

    if !skill_dir.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Read SKILL.md
    let skill_md_path = skill_dir.join("SKILL.md");
    let content = tokio::fs::read_to_string(&skill_md_path)
        .await
        .unwrap_or_default();

    // Parse frontmatter for description (handles YAML multiline | and > blocks)
    let description = content
        .strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---"))
        .and_then(|(fm, _)| {
            let lines: Vec<&str> = fm.lines().collect();
            let desc_idx = lines.iter().position(|l| l.starts_with("description:"))?;
            let after_colon = lines[desc_idx].trim_start_matches("description:").trim();

            if after_colon == "|"
                || after_colon == ">"
                || after_colon == "|+"
                || after_colon == ">-"
            {
                // YAML multiline block scalar: collect indented continuation lines
                let mut parts = Vec::new();
                for line in &lines[desc_idx + 1..] {
                    if line.starts_with("  ") || line.starts_with("\t") {
                        parts.push(line.trim());
                    } else {
                        break;
                    }
                }
                let sep = if after_colon.starts_with('>') {
                    " "
                } else {
                    "\n"
                };
                Some(parts.join(sep))
            } else {
                // Inline value
                Some(after_colon.trim_matches('"').to_string())
            }
        })
        .unwrap_or_default();

    // Render markdown body (without frontmatter) to HTML
    let body = strip_frontmatter(&content);
    let content_html = render_md_to_html(body);

    // List scripts
    let scripts_dir = skill_dir.join("scripts");
    let scripts = if scripts_dir.exists() {
        let mut entries = Vec::new();
        if let Ok(mut rd) = tokio::fs::read_dir(&scripts_dir).await {
            while let Ok(Some(entry)) = rd.next_entry().await {
                if let Some(fname) = entry.file_name().to_str() {
                    entries.push(fname.to_string());
                }
            }
        }
        entries.sort();
        entries
    } else {
        Vec::new()
    };

    let source = detect_skill_source(&skill_dir);

    Ok(Json(SkillDetailView {
        name,
        description,
        path: skill_dir.display().to_string(),
        source,
        content_html,
        scripts,
    }))
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

fn display_name_for(provider: &str) -> &'static str {
    match provider {
        // Primary
        "anthropic" => "Anthropic",
        "openai" => "OpenAI",
        "openrouter" => "OpenRouter",
        "gemini" => "Gemini",
        // Cloud
        "deepseek" => "DeepSeek",
        "groq" => "Groq",
        "mistral" => "Mistral",
        "xai" => "xAI",
        "together" => "Together",
        "fireworks" => "Fireworks",
        "perplexity" => "Perplexity",
        "cohere" => "Cohere",
        "venice" => "Venice",
        // Gateways
        "aihubmix" => "AiHubMix",
        "vercel" => "Vercel",
        "cloudflare" => "Cloudflare",
        "copilot" => "Copilot",
        "bedrock" => "Bedrock",
        "ollama_cloud" => "Ollama Cloud",
        // Chinese
        "moonshot" => "Moonshot",
        "zhipu" => "Zhipu",
        "dashscope" => "DashScope",
        "minimax" => "MiniMax",
        // Local
        "ollama" => "Ollama",
        "vllm" => "vLLM",
        "custom" => "Custom",
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
    vision_model: String,
    ollama_configured: bool,
    ollama_cloud_configured: bool,
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

        let display = display_name_for(name);
        for model_id in cloud_models_for(name) {
            let short = model_id.split('/').next_back().unwrap_or(model_id);
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
        vision_model,
        ollama_configured,
        ollama_cloud_configured,
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
        format!(
            "{}{}",
            "•".repeat(token.len().min(20) - 4),
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
    // Note: wa-rs now handles stale sessions internally — when with_pair_code() is set,
    // it clears the device identity so the handshake uses registration instead of login.
    // Bridge events from wa-rs callback → mpsc → WebSocket
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

// ═══════════════════════════════════════════════════════════════════
// MEMORY API
// ═══════════════════════════════════════════════════════════════════

#[derive(Serialize)]
struct MemoryStatsResponse {
    chunk_count: i64,
    daily_count: usize,
    has_memory_md: bool,
    has_history_md: bool,
    has_instructions_md: bool,
}

async fn memory_stats(State(state): State<Arc<AppState>>) -> Json<MemoryStatsResponse> {
    let data_dir = crate::config::Config::data_dir();

    let chunk_count = match state.db.as_ref() {
        Some(db) => db.count_memory_chunks().await.unwrap_or(0),
        None => 0,
    };

    let daily_count = std::fs::read_dir(data_dir.join("memory"))
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
                .count()
        })
        .unwrap_or(0);

    Json(MemoryStatsResponse {
        chunk_count,
        daily_count,
        has_memory_md: data_dir.join("MEMORY.md").exists(),
        has_history_md: data_dir.join("HISTORY.md").exists(),
        has_instructions_md: data_dir.join("brain").join("INSTRUCTIONS.md").exists()
            || data_dir.join("INSTRUCTIONS.md").exists(),
    })
}

/// Run memory cleanup based on retention policies.
/// POST /api/v1/memory/cleanup
#[derive(Deserialize)]
struct MemoryCleanupRequest {
    /// Override conversation retention days (optional)
    conversation_retention_days: Option<u32>,
    /// Override history retention days (optional)
    history_retention_days: Option<u32>,
}

#[derive(Serialize)]
struct MemoryCleanupResponse {
    ok: bool,
    messages_deleted: u64,
    chunks_deleted: u64,
    message: String,
}

async fn run_memory_cleanup(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MemoryCleanupRequest>,
) -> Result<Json<MemoryCleanupResponse>, StatusCode> {
    let db = match state.db.as_ref() {
        Some(db) => db,
        None => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };

    // Use request overrides or config defaults
    let config = state.config.read().await;
    let mem_config = &config.memory;
    let conv_days = req
        .conversation_retention_days
        .unwrap_or(mem_config.conversation_retention_days);
    let hist_days = req
        .history_retention_days
        .unwrap_or(mem_config.history_retention_days);
    drop(config); // Release lock before DB operation

    match db.run_memory_cleanup(conv_days, hist_days).await {
        Ok(result) => Ok(Json(MemoryCleanupResponse {
            ok: true,
            messages_deleted: result.messages_deleted,
            chunks_deleted: result.chunks_deleted,
            message: format!(
                "Cleaned up {} old messages and {} old history chunks",
                result.messages_deleted, result.chunks_deleted
            ),
        })),
        Err(e) => {
            tracing::error!(error = %e, "Memory cleanup failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
struct MemoryFileQuery {
    file: String,
}

#[derive(Serialize)]
struct MemoryFileResponse {
    ok: bool,
    content: String,
}

async fn get_memory_file(
    Query(q): Query<MemoryFileQuery>,
) -> Result<Json<MemoryFileResponse>, StatusCode> {
    let data_dir = crate::config::Config::data_dir();
    let brain_dir = data_dir.join("brain");
    let path = match q.file.as_str() {
        "memory" => data_dir.join("MEMORY.md"),
        "history" => data_dir.join("HISTORY.md"),
        "instructions" => {
            // Prefer brain/ location, fall back to legacy data_dir
            let new_path = brain_dir.join("INSTRUCTIONS.md");
            if new_path.exists() {
                new_path
            } else {
                data_dir.join("INSTRUCTIONS.md")
            }
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    Ok(Json(MemoryFileResponse { ok: true, content }))
}

#[derive(Deserialize)]
struct PutMemoryFileRequest {
    file: String,
    content: String,
}

#[derive(Serialize)]
struct OkResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

async fn put_memory_file(
    Json(req): Json<PutMemoryFileRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let data_dir = crate::config::Config::data_dir();
    let brain_dir = data_dir.join("brain");
    let path = match req.file.as_str() {
        "memory" => data_dir.join("MEMORY.md"),
        "instructions" => brain_dir.join("INSTRUCTIONS.md"),
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    tokio::fs::write(&path, &req.content)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(OkResponse {
        ok: true,
        message: None,
    }))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
}

fn default_search_limit() -> usize {
    20
}

#[derive(Serialize)]
struct SearchResponse {
    chunks: Vec<ChunkView>,
}

#[derive(Serialize)]
struct ChunkView {
    id: i64,
    date: String,
    source: String,
    heading: String,
    content: String,
    memory_type: String,
    created_at: String,
}

impl From<crate::storage::MemoryChunkRow> for ChunkView {
    fn from(row: crate::storage::MemoryChunkRow) -> Self {
        Self {
            id: row.id,
            date: row.date,
            source: row.source,
            heading: row.heading,
            content: row.content,
            memory_type: row.memory_type,
            created_at: row.created_at,
        }
    }
}

async fn search_memory(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    if q.q.trim().is_empty() {
        return Ok(Json(SearchResponse { chunks: vec![] }));
    }

    let fts_results = db
        .fts5_search(&q.q, q.limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if fts_results.is_empty() {
        return Ok(Json(SearchResponse { chunks: vec![] }));
    }

    let ids: Vec<i64> = fts_results.iter().map(|&(id, _)| id).collect();
    let rows = db
        .load_chunks_by_ids(&ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Preserve FTS5 ranking order
    let mut id_order: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for (i, &(id, _)) in fts_results.iter().enumerate() {
        id_order.insert(id, i);
    }
    let mut chunks: Vec<ChunkView> = rows.into_iter().map(ChunkView::from).collect();
    chunks.sort_by_key(|c| id_order.get(&c.id).copied().unwrap_or(usize::MAX));

    Ok(Json(SearchResponse { chunks }))
}

#[derive(Deserialize)]
struct HistoryQuery {
    #[serde(default = "default_history_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_history_limit() -> i64 {
    20
}

async fn get_memory_history(
    State(state): State<Arc<AppState>>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<SearchResponse>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let rows: Vec<crate::storage::MemoryChunkRow> = sqlx::query_as(
        "SELECT id, date, source, heading, content, memory_type, created_at \
         FROM memory_chunks WHERE memory_type = 'history' \
         ORDER BY created_at DESC LIMIT ? OFFSET ?",
    )
    .bind(q.limit)
    .bind(q.offset)
    .fetch_all(db.pool())
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let chunks = rows.into_iter().map(ChunkView::from).collect();
    Ok(Json(SearchResponse { chunks }))
}

#[derive(Serialize)]
struct InstructionsResponse {
    ok: bool,
    instructions: Vec<String>,
}

async fn get_instructions() -> Json<InstructionsResponse> {
    let data_dir = crate::config::Config::data_dir();
    let brain_path = data_dir.join("brain").join("INSTRUCTIONS.md");
    let legacy_path = data_dir.join("INSTRUCTIONS.md");
    let path = if brain_path.exists() {
        brain_path
    } else {
        legacy_path
    };

    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let instructions: Vec<String> = content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .map(|s| s.to_string())
        })
        .filter(|s| !s.is_empty())
        .collect();

    Json(InstructionsResponse {
        ok: true,
        instructions,
    })
}

#[derive(Deserialize)]
struct PutInstructionsRequest {
    instructions: Vec<String>,
}

async fn put_instructions(
    Json(req): Json<PutInstructionsRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let data_dir = crate::config::Config::data_dir();
    let brain_dir = data_dir.join("brain");
    let path = brain_dir.join("INSTRUCTIONS.md");

    tokio::fs::create_dir_all(&brain_dir)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let content = req
        .instructions
        .iter()
        .map(|i| format!("- {i}"))
        .collect::<Vec<_>>()
        .join("\n");

    tokio::fs::write(
        &path,
        if content.is_empty() {
            String::new()
        } else {
            format!("{content}\n")
        },
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(OkResponse {
        ok: true,
        message: None,
    }))
}

#[derive(Serialize)]
struct DailyListResponse {
    dates: Vec<String>,
}

async fn list_daily_files() -> Json<DailyListResponse> {
    let memory_dir = crate::config::Config::data_dir().join("memory");

    let mut dates: Vec<String> = std::fs::read_dir(&memory_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    name.strip_suffix(".md").map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    dates.sort_unstable_by(|a, b| b.cmp(a)); // newest first
    Json(DailyListResponse { dates })
}

#[derive(Serialize)]
struct DailyFileResponse {
    ok: bool,
    date: String,
    content: String,
}

async fn get_daily_file(Path(date): Path<String>) -> Result<Json<DailyFileResponse>, StatusCode> {
    // Validate date format to prevent path traversal
    if !date.chars().all(|c| c.is_ascii_digit() || c == '-') || date.len() != 10 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let path = crate::config::Config::data_dir()
        .join("memory")
        .join(format!("{date}.md"));

    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(Json(DailyFileResponse {
        ok: true,
        date,
        content,
    }))
}

// ═══════════════════════════════════════════════════════════════════
// VAULT API
// ═══════════════════════════════════════════════════════════════════

#[derive(Serialize)]
struct VaultKeysResponse {
    keys: Vec<String>,
}

async fn list_vault_keys() -> Result<Json<VaultKeysResponse>, StatusCode> {
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
    Json(req): Json<SetVaultRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
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

async fn reveal_vault_secret(
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
            return do_reveal_secret(&key).await;
        }
    };

    let config = match storage.load() {
        Ok(c) => c,
        Err(_) => {
            tracing::warn!("Could not load 2FA config, allowing vault access");
            return do_reveal_secret(&key).await;
        }
    };

    if !config.enabled {
        // 2FA not enabled, allow access
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

async fn delete_vault_secret(Path(key): Path<String>) -> Result<Json<OkResponse>, StatusCode> {
    let secrets =
        crate::storage::global_secrets().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let secret_key = crate::storage::SecretKey::custom(&format!("vault.{key}"));

    secrets
        .delete(&secret_key)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(OkResponse {
        ok: true,
        message: None,
    }))
}

// ─── Vault 2FA ──────────────────────────────────────────────────

/// Pending 2FA setup data (stored in memory until confirmed)
static PENDING_2FA_SETUP: std::sync::Mutex<Option<Pending2FaSetup>> = std::sync::Mutex::new(None);

#[derive(Clone)]
struct Pending2FaSetup {
    secret: String,
    account: String,
    recovery_codes: Vec<String>,
    qr_image: String,
    qr_url: String,
}

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

#[derive(Serialize)]
struct TwoFaSetupResponse {
    qr_image: String,
    secret: String,
    uri: String,
}

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

#[derive(Deserialize)]
struct Disable2FaRequest {
    code: String,
}

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

#[derive(Deserialize)]
struct Update2FaSettingsRequest {
    session_id: String,
    session_timeout_secs: Option<u64>,
}

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

// ─── Chat History ──────────────────────────────────────────────

#[derive(Deserialize)]
struct ChatHistoryQuery {
    limit: Option<u32>,
}

#[derive(Serialize)]
struct ChatHistoryMessage {
    role: String,
    content: String,
    tools_used: Vec<String>,
    timestamp: String,
}

async fn chat_history(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ChatHistoryQuery>,
) -> Result<Json<Vec<ChatHistoryMessage>>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let limit = q.limit.unwrap_or(50);

    let rows = db
        .load_messages("web:default", limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let messages: Vec<ChatHistoryMessage> = rows
        .into_iter()
        .filter(|r| r.role == "user" || r.role == "assistant")
        .map(|r| {
            let tools: Vec<String> = serde_json::from_str(&r.tools_used).unwrap_or_default();
            ChatHistoryMessage {
                role: r.role,
                content: r.content,
                tools_used: tools,
                timestamp: r.timestamp,
            }
        })
        .collect();

    Ok(Json(messages))
}

/// Clear chat history for the web session
async fn clear_chat_history(
    State(state): State<Arc<AppState>>,
) -> Result<Json<OkResponse>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    db.clear_messages("web:default")
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(OkResponse {
        ok: true,
        message: Some("Chat history cleared".to_string()),
    }))
}

/// Compact chat conversation (trigger memory consolidation)
async fn compact_chat(State(state): State<Arc<AppState>>) -> Result<Json<OkResponse>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    // Check if there are enough messages to consolidate
    let count = db
        .count_messages("web:default")
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if count < 10 {
        return Ok(Json(OkResponse {
            ok: false,
            message: Some("Not enough messages to compact (need at least 10)".to_string()),
        }));
    }

    // Trigger consolidation by resetting the last_consolidated counter
    // The agent loop will handle the actual consolidation on next message
    db.upsert_session("web:default", 0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(OkResponse {
        ok: true,
        message: Some("Conversation will be compacted on next message".to_string()),
    }))
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

// ═══════════════════════════════════════════════════════════════
// WEBHOOK INGRESS
// ═══════════════════════════════════════════════════════════════

/// Request body for webhook ingress
#[derive(Debug, Deserialize)]
struct WebhookRequest {
    /// The message to send to the agent
    message: String,
    /// Optional: conversation ID for threading (defaults to "webhook")
    #[serde(default)]
    conversation_id: Option<String>,
}

/// Response for webhook ingress
#[derive(Debug, Serialize)]
struct WebhookResponse {
    status: &'static str,
    user: String,
    conversation_id: String,
}

/// Error response for webhook ingress
#[derive(Debug, Serialize)]
struct WebhookError {
    error: &'static str,
    message: String,
}

/// Handle incoming webhook requests.
///
/// Validates the webhook token, resolves the user, and forwards the message
/// to the agent for processing.
///
/// POST /api/v1/webhook/{token}
/// Body: { "message": "...", "conversation_id": "optional" }
async fn webhook_ingress(
    Path(token): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<WebhookRequest>,
) -> Result<Json<WebhookResponse>, (StatusCode, Json<WebhookError>)> {
    // Validate token format (should start with "wh_")
    if !token.starts_with("wh_") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(WebhookError {
                error: "invalid_token",
                message: "Token must start with 'wh_'".to_string(),
            }),
        ));
    }

    // Look up the user by webhook token
    let db = match &state.db {
        Some(db) => db,
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(WebhookError {
                    error: "no_database",
                    message: "Database not available".to_string(),
                }),
            ));
        }
    };

    let user = db.lookup_user_by_webhook_token(&token).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(WebhookError {
                error: "database_error",
                message: e.to_string(),
            }),
        )
    })?;

    let user = match user {
        Some(u) => u,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(WebhookError {
                    error: "invalid_token",
                    message: "Unknown or disabled webhook token".to_string(),
                }),
            ));
        }
    };

    // Update token last_used
    let _ = db.touch_webhook_token(&token).await;

    // Build session key: webhook:{conversation_id}
    let conversation_id = body
        .conversation_id
        .unwrap_or_else(|| "default".to_string());
    let session_key = format!("webhook:{}", conversation_id);

    // Create inbound message
    let inbound = crate::bus::InboundMessage {
        channel: "webhook".to_string(),
        sender_id: user.id.clone(),
        chat_id: session_key.clone(),
        content: body.message,
        timestamp: chrono::Utc::now(),
    };

    // Send to agent
    let inbound_tx = match &state.inbound_tx {
        Some(tx) => tx,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(WebhookError {
                    error: "agent_not_running",
                    message: "Agent is not running. Start the gateway first.".to_string(),
                }),
            ));
        }
    };

    inbound_tx.send(inbound).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(WebhookError {
                error: "send_failed",
                message: format!("Failed to send message to agent: {}", e),
            }),
        )
    })?;

    tracing::info!(
        token = &token[..12.min(token.len())],
        user = %user.username,
        session = %session_key,
        "Webhook message received"
    );

    Ok(Json(WebhookResponse {
        status: "queued",
        user: user.username,
        conversation_id,
    }))
}

// ═══════════════════════════════════════════════════════════════
// ACCOUNT API
// ═══════════════════════════════════════════════════════════════

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

    db.create_webhook_token(&token, &owner.id, &body.name)
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
        last_used: current.and_then(|t| t.last_used.clone()),
        created_at: current.map(|t| t.created_at.clone()).unwrap_or_default(),
    }))
}

// ─── Browser ─────────────────────────────────────────────────────

#[derive(Serialize)]
struct BrowserTestResponse {
    success: bool,
    message: String,
}

/// Test if browser can be launched
#[cfg(feature = "browser")]
async fn test_browser(State(state): State<Arc<AppState>>) -> Json<BrowserTestResponse> {
    let config = state.config.read().await;

    if !config.browser.enabled {
        return Json(BrowserTestResponse {
            success: false,
            message: "Browser automation is disabled in configuration".to_string(),
        });
    }

    // Try to get the browser manager and test it
    let manager = crate::browser::global_browser_manager();

    match manager.test_connection().await {
        Ok(()) => Json(BrowserTestResponse {
            success: true,
            message: "Browser launched successfully".to_string(),
        }),
        Err(e) => Json(BrowserTestResponse {
            success: false,
            message: format!("Failed to launch browser: {}", e),
        }),
    }
}

// ═══════════════════════════════════════════════════════════════
// APPROVALS (P0-4)
// ═══════════════════════════════════════════════════════════════

/// Response for pending approvals list
#[derive(Debug, Serialize)]
struct PendingApprovalsResponse {
    pending: Vec<crate::tools::PendingApproval>,
    count: usize,
}

/// Response for approval audit log
#[derive(Debug, Serialize)]
struct ApprovalAuditResponse {
    log: Vec<crate::tools::ApprovalLogEntry>,
    count: usize,
}

/// Request for approving/denying
#[derive(Debug, Deserialize)]
struct ApprovalActionRequest {
    /// If true, add to session allowlist (approve all future same commands)
    #[serde(default)]
    always: bool,
}

/// Response for approval action
#[derive(Debug, Serialize)]
struct ApprovalActionResponse {
    success: bool,
    message: String,
}

/// Response for approval config
#[derive(Debug, Serialize)]
struct ApprovalConfigResponse {
    level: String,
    auto_approve: Vec<String>,
    always_ask: Vec<String>,
    pending_count: usize,
}

/// Request for updating approval config
#[derive(Debug, Deserialize)]
struct ApprovalConfigRequest {
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    auto_approve: Option<Vec<String>>,
    #[serde(default)]
    always_ask: Option<Vec<String>>,
}

/// List all approvals (pending + summary)
/// GET /api/v1/approvals
async fn list_approvals(State(_state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let mgr = crate::tools::global_approval_manager();

    match mgr {
        Some(m) => {
            let pending = m.get_pending();
            let audit = m.audit_log();
            Json(serde_json::json!({
                "pending": pending,
                "pending_count": pending.len(),
                "audit_count": audit.len(),
                "autonomy_level": format!("{:?}", m.autonomy_level()),
                "session_allowlist": m.session_allowlist().into_iter().collect::<Vec<_>>(),
            }))
        }
        None => Json(serde_json::json!({
            "error": "Approval manager not initialized",
            "pending": [],
            "pending_count": 0,
        })),
    }
}

/// List pending approvals
/// GET /api/v1/approvals/pending
async fn list_pending_approvals(
    State(_state): State<Arc<AppState>>,
) -> Json<PendingApprovalsResponse> {
    let mgr = crate::tools::global_approval_manager();

    let pending = mgr.map(|m| m.get_pending()).unwrap_or_default();
    let count = pending.len();

    Json(PendingApprovalsResponse { pending, count })
}

/// Get approval audit log
/// GET /api/v1/approvals/audit
async fn get_approval_audit_log(
    State(_state): State<Arc<AppState>>,
) -> Json<ApprovalAuditResponse> {
    let mgr = crate::tools::global_approval_manager();

    let log = mgr.map(|m| m.audit_log()).unwrap_or_default();
    let count = log.len();

    Json(ApprovalAuditResponse { log, count })
}

/// Approve a pending request
/// POST /api/v1/approvals/{id}/approve
async fn approve_request(
    Path(id): Path<String>,
    State(_state): State<Arc<AppState>>,
    Json(body): Json<ApprovalActionRequest>,
) -> Result<Json<ApprovalActionResponse>, (StatusCode, Json<ApprovalActionResponse>)> {
    let mgr = crate::tools::global_approval_manager();

    match mgr {
        Some(m) => match m.approve(&id, body.always) {
            Ok(()) => Ok(Json(ApprovalActionResponse {
                success: true,
                message: if body.always {
                    "Approved and added to session allowlist".to_string()
                } else {
                    "Approved for this session".to_string()
                },
            })),
            Err(e) => Err((
                StatusCode::NOT_FOUND,
                Json(ApprovalActionResponse {
                    success: false,
                    message: e,
                }),
            )),
        },
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApprovalActionResponse {
                success: false,
                message: "Approval manager not initialized".to_string(),
            }),
        )),
    }
}

/// Deny a pending request
/// POST /api/v1/approvals/{id}/deny
async fn deny_request(
    Path(id): Path<String>,
    State(_state): State<Arc<AppState>>,
) -> Result<Json<ApprovalActionResponse>, (StatusCode, Json<ApprovalActionResponse>)> {
    let mgr = crate::tools::global_approval_manager();

    match mgr {
        Some(m) => match m.deny(&id) {
            Ok(()) => Ok(Json(ApprovalActionResponse {
                success: true,
                message: "Request denied".to_string(),
            })),
            Err(e) => Err((
                StatusCode::NOT_FOUND,
                Json(ApprovalActionResponse {
                    success: false,
                    message: e,
                }),
            )),
        },
        None => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApprovalActionResponse {
                success: false,
                message: "Approval manager not initialized".to_string(),
            }),
        )),
    }
}

/// Get approval configuration
/// GET /api/v1/approvals/config
async fn get_approval_config(State(state): State<Arc<AppState>>) -> Json<ApprovalConfigResponse> {
    let config = state.config.read().await;
    let mgr = crate::tools::global_approval_manager();

    Json(ApprovalConfigResponse {
        level: format!("{:?}", config.permissions.approval.level).to_lowercase(),
        auto_approve: config.permissions.approval.auto_approve.clone(),
        always_ask: config.permissions.approval.always_ask.clone(),
        pending_count: mgr.map(|m| m.get_pending().len()).unwrap_or(0),
    })
}

/// Update approval configuration
/// PUT /api/v1/approvals/config
async fn put_approval_config(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ApprovalConfigRequest>,
) -> Result<Json<ApprovalConfigResponse>, (StatusCode, String)> {
    use crate::config::AutonomyLevel;

    let mut config = state.config.write().await;

    if let Some(level) = body.level {
        let level = match level.to_lowercase().as_str() {
            "full" => AutonomyLevel::Full,
            "supervised" => AutonomyLevel::Supervised,
            "readonly" | "read_only" => AutonomyLevel::ReadOnly,
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("Invalid autonomy level: {}", level),
                ))
            }
        };
        config.permissions.approval.level = level;
    }

    if let Some(auto_approve) = body.auto_approve {
        config.permissions.approval.auto_approve = auto_approve;
    }

    if let Some(always_ask) = body.always_ask {
        config.permissions.approval.always_ask = always_ask;
    }

    // Save config
    if let Err(e) = config.save() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save config: {}", e),
        ));
    }

    let mgr = crate::tools::global_approval_manager();

    Ok(Json(ApprovalConfigResponse {
        level: format!("{:?}", config.permissions.approval.level).to_lowercase(),
        auto_approve: config.permissions.approval.auto_approve.clone(),
        always_ask: config.permissions.approval.always_ask.clone(),
        pending_count: mgr.map(|m| m.get_pending().len()).unwrap_or(0),
    }))
}
