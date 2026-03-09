use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::path::{Component, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Multipart, Path, Query, State, WebSocketUpgrade};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};

use super::server::AppState;

pub fn router() -> Router<Arc<AppState>> {
    let api_router = Router::new()
        .route("/health", get(health))
        .route("/v1/logs/stream", get(stream_logs))
        .route("/v1/logs/recent", get(recent_logs))
        .route("/v1/status", get(status))
        .route("/v1/config", get(get_config))
        .route("/v1/config", axum::routing::patch(patch_config))
        .route("/v1/skills", get(list_skills))
        .route("/v1/skills/search", get(search_skills))
        .route("/v1/skills/install", axum::routing::post(install_skill))
        .route(
            "/v1/skills/create",
            axum::routing::post(create_skill_api),
        )
        .route(
            "/v1/skills/{name}",
            get(get_skill_detail).delete(delete_skill),
        )
        .route(
            "/v1/skills/{name}/scan",
            axum::routing::post(scan_skill_api),
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
        // --- MCP ---
        .route("/v1/mcp/catalog", get(list_mcp_catalog))
        .route("/v1/mcp/suggest", get(suggest_mcp_catalog))
        .route("/v1/mcp/search", get(search_mcp_catalog))
        .route(
            "/v1/mcp/install-guide",
            axum::routing::post(mcp_install_guide),
        )
        .route(
            "/v1/mcp/oauth/google/start",
            axum::routing::post(start_google_mcp_oauth),
        )
        .route(
            "/v1/mcp/oauth/google/exchange",
            axum::routing::post(exchange_google_mcp_oauth_code),
        )
        .route(
            "/v1/mcp/oauth/github/start",
            axum::routing::post(start_github_mcp_oauth),
        )
        .route(
            "/v1/mcp/oauth/github/exchange",
            axum::routing::post(exchange_github_mcp_oauth_code),
        )
        .route(
            "/v1/mcp/servers",
            get(list_mcp_servers).post(upsert_mcp_server),
        )
        .route("/v1/mcp/setup", axum::routing::post(setup_mcp_server))
        .route(
            "/v1/mcp/servers/{name}/toggle",
            axum::routing::post(toggle_mcp_server),
        )
        .route(
            "/v1/mcp/servers/{name}/test",
            axum::routing::post(test_mcp_server),
        )
        .route(
            "/v1/mcp/servers/{name}",
            axum::routing::delete(delete_mcp_server),
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
        .route(
            "/v1/channels/email/trigger-word",
            axum::routing::post(generate_or_get_trigger_word),
        )
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
            "/v1/chat/conversations",
            get(list_chat_conversations).post(create_chat_conversation),
        )
        .route(
            "/v1/chat/conversations/{conversation_id}",
            axum::routing::patch(update_chat_conversation).delete(delete_chat_conversation),
        )
        .route(
            "/v1/chat/history",
            get(chat_history).delete(clear_chat_history),
        )
        .route(
            "/v1/chat/uploads",
            axum::routing::post(upload_chat_attachment),
        )
        .route(
            "/v1/chat/uploads/{conversation_id}/{file_name}",
            get(get_chat_uploaded_file),
        )
        .route("/v1/chat/run", get(current_chat_run))
        .route("/v1/chat/compact", axum::routing::post(compact_chat))
        .route("/v1/chat/stop", axum::routing::post(stop_chat_run))
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
        .route(
            "/v1/security/sandbox",
            get(get_execution_sandbox).put(put_execution_sandbox),
        )
        .route(
            "/v1/security/sandbox/status",
            get(get_execution_sandbox_status),
        )
        .route(
            "/v1/security/sandbox/presets",
            get(get_execution_sandbox_presets),
        )
        .route(
            "/v1/security/sandbox/image",
            get(get_execution_sandbox_image_status),
        )
        .route(
            "/v1/security/sandbox/image/pull",
            axum::routing::post(pull_execution_sandbox_image),
        )
        .route(
            "/v1/security/sandbox/events",
            get(get_execution_sandbox_events),
        )
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
        )
        // --- Email Accounts (multi-account) ---
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
        // --- Automations ---
        .route(
            "/v1/automations",
            get(list_automations).post(create_automation),
        )
        .route("/v1/automations/targets", get(list_automation_targets))
        .route(
            "/v1/automations/{id}",
            axum::routing::patch(patch_automation).delete(delete_automation),
        )
        .route("/v1/automations/{id}/history", get(get_automation_history))
        .route(
            "/v1/automations/{id}/run",
            axum::routing::post(run_automation_now),
        )
        // --- Usage ---
        .route("/v1/usage", get(get_usage));

    // --- Knowledge Base (RAG) ---
    #[cfg(feature = "local-embeddings")]
    let api_router = api_router
        .route("/v1/knowledge/stats", get(knowledge_stats))
        .route(
            "/v1/knowledge/sources",
            get(list_knowledge_sources).delete(delete_knowledge_source),
        )
        .route("/v1/knowledge/search", get(search_knowledge))
        .route(
            "/v1/knowledge/ingest",
            axum::routing::post(ingest_knowledge),
        )
        .route(
            "/v1/knowledge/ingest-directory",
            axum::routing::post(ingest_knowledge_directory),
        )
        .route(
            "/v1/knowledge/reveal",
            axum::routing::post(reveal_knowledge_chunk),
        );

    // --- Browser (optional) ---
    #[cfg(feature = "browser")]
    let api_router = api_router.route("/v1/browser/test", axum::routing::post(test_browser));

    // --- Emergency Stop ---
    let api_router = api_router
        .route(
            "/v1/emergency-stop",
            axum::routing::post(emergency_stop_handler),
        )
        .route("/v1/resume", axum::routing::post(resume_handler));

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

/// GET /api/v1/logs/stream
async fn stream_logs() -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let rx = crate::logs::subscribe();

    let stream = futures::stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Ok(record) => {
                let payload =
                    serde_json::to_string(&record).unwrap_or_else(|_| "{}".to_string());
                let event = Event::default().event("log").data(payload);
                Some((Ok(event), rx))
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                let lag_record = crate::logs::LogRecord {
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    level: "warn".to_string(),
                    target: "homun.logs".to_string(),
                    message: format!(
                        "Log stream dropped {skipped} events because the client was too slow"
                    ),
                    module_path: None,
                    file: None,
                    line: None,
                    fields: Vec::new(),
                };
                let payload =
                    serde_json::to_string(&lag_record).unwrap_or_else(|_| "{}".to_string());
                let event = Event::default().event("log").data(payload);
                Some((Ok(event), rx))
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
        }
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    )
}

#[derive(Deserialize)]
struct RecentLogsQuery {
    limit: Option<usize>,
}

/// GET /api/v1/logs/recent
async fn recent_logs(Query(query): Query<RecentLogsQuery>) -> Json<Vec<crate::logs::LogRecord>> {
    let limit = query.limit.unwrap_or(250).clamp(1, 1000);
    Json(crate::logs::recent(limit))
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
    #[serde(default)]
    force: bool,
}

#[derive(Serialize)]
struct InstallResponse {
    ok: bool,
    name: String,
    message: String,
    security_report: Option<InstallSecurityReportView>,
}

#[derive(Serialize)]
struct InstallSecurityReportView {
    risk_score: u8,
    blocked: bool,
    warnings: usize,
    scanned_files: usize,
    summary: String,
}

fn install_security_view(
    report: Option<&crate::skills::SecurityReport>,
) -> Option<InstallSecurityReportView> {
    report.map(|report| InstallSecurityReportView {
        risk_score: report.risk_score,
        blocked: report.blocked,
        warnings: report.warnings.len(),
        scanned_files: report.scanned_files,
        summary: report.summary(),
    })
}

async fn install_skill(
    Json(req): Json<InstallRequest>,
) -> Result<Json<InstallResponse>, StatusCode> {
    let security_options = crate::skills::InstallSecurityOptions { force: req.force };
    let result = if let Some(slug) = req.source.strip_prefix("clawhub:") {
        let hub = crate::skills::ClawHubInstaller::new();
        hub.install_with_options(slug, security_options.clone())
            .await
    } else if let Some(dir_name) = req.source.strip_prefix("openskills:") {
        let source = crate::skills::OpenSkillsSource::new();
        source
            .install_with_options(dir_name, security_options.clone())
            .await
    } else {
        let installer = crate::skills::SkillInstaller::new();
        installer
            .install_with_options(&req.source, security_options)
            .await
    };

    match result {
        Ok(r) => Ok(Json(InstallResponse {
            ok: true,
            name: r.name,
            message: r.description,
            security_report: install_security_view(r.security_report.as_ref()),
        })),
        Err(e) => Ok(Json(InstallResponse {
            ok: false,
            name: String::new(),
            message: e.to_string(),
            security_report: None,
        })),
    }
}

// --- Create skill ---

#[derive(Deserialize)]
struct CreateSkillRequest {
    prompt: String,
    name: Option<String>,
    language: Option<String>,
    #[serde(default)]
    overwrite: bool,
}

#[derive(Serialize)]
struct CreateSkillResponse {
    ok: bool,
    name: String,
    path: String,
    language: String,
    reused_skills: Vec<String>,
    smoke_test_passed: bool,
    validation_notes: Vec<String>,
    message: String,
    security_report: Option<SecurityReportDetailView>,
}

#[derive(Serialize)]
struct SecurityReportDetailView {
    risk_score: u8,
    blocked: bool,
    scanned_files: usize,
    summary: String,
    warnings: Vec<SecurityWarningView>,
}

#[derive(Serialize)]
struct SecurityWarningView {
    severity: String,
    category: String,
    description: String,
    file: Option<String>,
    line: Option<usize>,
}

fn security_detail_view(report: &crate::skills::SecurityReport) -> SecurityReportDetailView {
    SecurityReportDetailView {
        risk_score: report.risk_score,
        blocked: report.blocked,
        scanned_files: report.scanned_files,
        summary: report.summary(),
        warnings: report
            .warnings
            .iter()
            .map(|w| SecurityWarningView {
                severity: format!("{:?}", w.severity),
                category: format!("{:?}", w.category),
                description: w.description.clone(),
                file: w.file.clone(),
                line: w.line,
            })
            .collect(),
    }
}

async fn create_skill_api(
    Json(req): Json<CreateSkillRequest>,
) -> Result<Json<CreateSkillResponse>, StatusCode> {
    let request = crate::skills::SkillCreationRequest {
        prompt: req.prompt,
        name: req.name,
        language: req.language,
        overwrite: req.overwrite,
    };

    match crate::skills::create_skill(request).await {
        Ok(result) => Ok(Json(CreateSkillResponse {
            ok: true,
            name: result.name,
            path: result.path.display().to_string(),
            language: result.script_language,
            reused_skills: result.reused_skills,
            smoke_test_passed: result.smoke_test_passed,
            validation_notes: result.validation_notes,
            message: String::new(),
            security_report: Some(security_detail_view(&result.security_report)),
        })),
        Err(e) => Ok(Json(CreateSkillResponse {
            ok: false,
            name: String::new(),
            path: String::new(),
            language: String::new(),
            reused_skills: vec![],
            smoke_test_passed: false,
            validation_notes: vec![],
            message: e.to_string(),
            security_report: None,
        })),
    }
}

// --- Scan skill ---

async fn scan_skill_api(
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let skills_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".homun")
        .join("skills");
    let skill_dir = skills_dir.join(&name);

    if !skill_dir.exists() {
        return Ok(Json(serde_json::json!({
            "ok": false,
            "message": format!("Skill '{}' not found", name),
        })));
    }

    match crate::skills::scan_skill_package(&skill_dir).await {
        Ok(report) => Ok(Json(serde_json::json!({
            "ok": true,
            "report": security_detail_view(&report),
        }))),
        Err(e) => Ok(Json(serde_json::json!({
            "ok": false,
            "message": e.to_string(),
        }))),
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
    recommended: bool,
    recommended_reason: Option<String>,
    decision_tags: Vec<String>,
    why_choose: Option<String>,
    tradeoff: Option<String>,
}

fn skill_query_terms(query: &str) -> Vec<String> {
    query
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| part.len() >= 2)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
}

fn skill_searchable_text(skill: &SkillSearchResultView) -> String {
    format!("{} {}", skill.name, skill.description).to_ascii_lowercase()
}

fn skill_recommendation_score(skill: &SkillSearchResultView, query: &str) -> i64 {
    let query_lower = query.trim().to_ascii_lowercase();
    let searchable = skill_searchable_text(skill);
    let mut score = 0i64;

    if !query_lower.is_empty() && searchable.contains(&query_lower) {
        score += 80;
    }
    for term in skill_query_terms(query) {
        if searchable.contains(&term) {
            score += 18;
        }
    }

    score += match skill.source.as_str() {
        "clawhub" => 40,
        "openskills" => 24,
        "github" => 12,
        _ => 0,
    };
    score += (skill.stars.min(5000) / 50) as i64;
    score += (skill.downloads.min(500_000) / 5_000) as i64;
    if skill.description.to_ascii_lowercase().contains("agent")
        || skill.description.to_ascii_lowercase().contains("workflow")
    {
        score += 6;
    }

    score
}

fn skill_recommended_reason(skill: &SkillSearchResultView) -> String {
    let mut reasons = Vec::new();
    match skill.source.as_str() {
        "clawhub" => reasons.push("curated in ClawHub".to_string()),
        "openskills" => reasons.push("community-curated in Open Skills".to_string()),
        "github" => reasons.push("direct GitHub source".to_string()),
        _ => {}
    }
    if skill.downloads > 0 {
        reasons.push(format!("{} downloads", skill.downloads));
    }
    if skill.stars > 0 {
        reasons.push(format!("{} GitHub stars", skill.stars));
    }
    if reasons.is_empty() {
        "best overall match for this search".to_string()
    } else {
        reasons.truncate(3);
        reasons.join(", ")
    }
}

fn skill_decision_tags(skill: &SkillSearchResultView) -> Vec<String> {
    let mut tags = Vec::new();
    match skill.source.as_str() {
        "clawhub" => tags.push("Curated".to_string()),
        "openskills" => tags.push("Open Skills".to_string()),
        "github" => tags.push("GitHub".to_string()),
        _ => {}
    }
    if skill.downloads >= 1_000 {
        tags.push("Popular".to_string());
    }
    if skill.stars >= 100 {
        tags.push("High signal".to_string());
    }
    tags.truncate(4);
    tags
}

fn skill_why_choose(skill: &SkillSearchResultView) -> String {
    match skill.source.as_str() {
        "clawhub" => {
            "Choose this if you want the safest default pick from a curated catalog.".to_string()
        }
        "openskills" => {
            "Choose this if you want a community-curated option with a cleaner install path."
                .to_string()
        }
        "github" => {
            "Choose this if you want the original repository or the broadest ecosystem coverage."
                .to_string()
        }
        _ => "Choose this if it matches your use case better than the default option.".to_string(),
    }
}

fn skill_tradeoff(skill: &SkillSearchResultView) -> String {
    match skill.source.as_str() {
        "clawhub" => {
            "Tradeoff: more opinionated curation, so niche variants may be missing.".to_string()
        }
        "openskills" => {
            "Tradeoff: quality varies by contributor and popularity signals may be weaker."
                .to_string()
        }
        "github" => {
            "Tradeoff: less curated, so install quality and maintenance can vary more.".to_string()
        }
        _ => "Tradeoff: not the clearest default option for a non-technical user.".to_string(),
    }
}

fn annotate_skill_search_results(results: &mut [SkillSearchResultView], query: &str) {
    if results.is_empty() {
        return;
    }

    for item in results.iter_mut() {
        item.recommended = false;
        item.recommended_reason = None;
        item.decision_tags = skill_decision_tags(item);
        item.why_choose = Some(skill_why_choose(item));
        item.tradeoff = Some(skill_tradeoff(item));
    }

    results.sort_by(|a, b| {
        skill_recommendation_score(b, query)
            .cmp(&skill_recommendation_score(a, query))
            .then_with(|| b.downloads.cmp(&a.downloads))
            .then_with(|| b.stars.cmp(&a.stars))
            .then_with(|| a.name.cmp(&b.name))
    });

    if let Some(first) = results.first_mut() {
        first.recommended = true;
        first.recommended_reason = Some(skill_recommended_reason(first));
        if !first.decision_tags.iter().any(|tag| tag == "Recommended") {
            first.decision_tags.insert(0, "Recommended".to_string());
        }
    }
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
                recommended: false,
                recommended_reason: None,
                decision_tags: vec![],
                why_choose: None,
                tradeoff: None,
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
                recommended: false,
                recommended_reason: None,
                decision_tags: vec![],
                why_choose: None,
                tradeoff: None,
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
                recommended: false,
                recommended_reason: None,
                decision_tags: vec![],
                why_choose: None,
                tradeoff: None,
            }));
        }
        Err(e) => {
            tracing::warn!(error = %e, "GitHub skill search failed, skipping");
        }
    }

    annotate_skill_search_results(&mut results, &query);
    Json(results)
}

// --- Delete skill ---

#[derive(Serialize)]
struct DeleteSkillResponse {
    ok: bool,
    message: String,
}

async fn delete_skill(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Json<DeleteSkillResponse> {
    match crate::skills::SkillInstaller::remove(&name).await {
        Ok(()) => {
            let mut message = format!("Skill '{}' removed", name);
            if let Some(db) = &state.db {
                let reason = format!("Missing skill dependency: {name}");
                match db
                    .invalidate_automations_by_dependency("skill", &name, &reason)
                    .await
                {
                    Ok(affected) if affected > 0 => {
                        message = format!("{message}. Invalidated {affected} automation(s).");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            skill = %name,
                            "Failed to invalidate dependent automations after skill removal"
                        );
                    }
                }
            }
            Json(DeleteSkillResponse { ok: true, message })
        }
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

// --- Provider Health ---

async fn providers_health(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    match state.health_tracker.as_ref() {
        Some(tracker) => {
            let snapshots = tracker.snapshots();
            Json(serde_json::json!({ "providers": snapshots }))
        }
        None => Json(serde_json::json!({ "providers": [] })),
    }
}

// --- Emergency Stop ---

async fn emergency_stop_handler(
    State(state): State<Arc<AppState>>,
) -> Json<crate::security::EStopReport> {
    let report = crate::security::emergency_stop(&state.estop_handles).await;
    Json(report)
}

async fn resume_handler(
    State(_state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    crate::security::resume();
    Json(serde_json::json!({ "status": "resumed", "network": "online" }))
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
        think: None,
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
    hidden_models: std::collections::HashMap<String, Vec<String>>,
    model_overrides: std::collections::HashMap<String, crate::config::ModelOverrides>,
    model_capabilities: std::collections::HashMap<String, crate::config::ModelCapabilities>,
    effective_model_capabilities:
        std::collections::HashMap<String, crate::config::ModelCapabilities>,
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
            // Strip provider prefix: "openrouter/anthropic/claude-sonnet-4" → "anthropic/claude-sonnet-4"
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
    let mut hidden_models = std::collections::HashMap::new();
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
    model_capabilities: std::collections::HashMap<String, crate::config::ModelCapabilities>,
}

fn build_model_capabilities_map<I>(
    config: &crate::config::Config,
    models: I,
    apply_overrides: bool,
) -> std::collections::HashMap<String, crate::config::ModelCapabilities>
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

// ═══════════════════════════════════════════════════
// MCP — catalog, guided setup, CRUD, test
// ═══════════════════════════════════════════════════

#[derive(Clone, Serialize)]
struct McpCatalogEnvView {
    key: String,
    description: String,
    required: bool,
    secret: bool,
}

#[derive(Clone, Serialize)]
struct McpCatalogItemView {
    kind: String, // preset | npm
    source: String,
    id: String,
    display_name: String,
    description: String,
    command: String,
    args: Vec<String>,
    transport: Option<String>, // stdio | http
    url: Option<String>,       // set when transport=http
    install_supported: bool,
    package_name: Option<String>,
    downloads_monthly: Option<u64>,
    score: Option<f64>,
    popularity_rank: Option<u32>,
    popularity_value: Option<u64>,
    popularity_source: Option<String>,
    env: Vec<McpCatalogEnvView>,
    docs_url: Option<String>,
    aliases: Vec<String>,
    keywords: Vec<String>,
    recommended: bool,
    recommended_reason: Option<String>,
    decision_tags: Vec<String>,
    setup_effort: String,
    auth_profile: String,
    preflight_checks: Vec<String>,
    why_choose: Option<String>,
    tradeoff: Option<String>,
}

fn preset_to_view(preset: crate::skills::McpServerPreset) -> McpCatalogItemView {
    McpCatalogItemView {
        kind: "preset".to_string(),
        source: "curated".to_string(),
        id: preset.id,
        display_name: preset.display_name,
        description: preset.description,
        command: preset.command,
        args: preset
            .args
            .iter()
            .map(|arg| crate::mcp_setup::render_mcp_arg_template(arg))
            .collect(),
        transport: Some("stdio".to_string()),
        url: None,
        install_supported: true,
        package_name: None,
        downloads_monthly: None,
        score: None,
        popularity_rank: None,
        popularity_value: None,
        popularity_source: None,
        env: preset
            .env
            .into_iter()
            .map(|e| McpCatalogEnvView {
                key: e.key,
                description: e.description,
                required: e.required,
                secret: e.secret,
            })
            .collect(),
        docs_url: preset.docs_url,
        aliases: preset.aliases,
        keywords: preset.keywords,
        recommended: false,
        recommended_reason: None,
        decision_tags: vec![],
        setup_effort: "Moderate".to_string(),
        auth_profile: "Unknown".to_string(),
        preflight_checks: vec![],
        why_choose: None,
        tradeoff: None,
    }
}

#[derive(Clone, Debug)]
struct McpLeaderboardEntry {
    rank: u32,
    popularity: u64,
    url: String,
}

#[derive(Debug)]
struct McpMarketItem {
    rank: u32,
    name: String,
    slug: String,
    popularity: u64,
    url: String,
}

#[derive(Deserialize)]
struct McpMarketFallback {
    items: Vec<McpMarketFallbackItem>,
}

#[derive(Deserialize)]
struct McpMarketFallbackItem {
    rank: u32,
    name: String,
    slug: String,
    popularity: u64,
    url: Option<String>,
}

#[derive(Deserialize)]
struct OfficialRegistryResponse {
    servers: Option<Vec<OfficialRegistryServerEntry>>,
}

#[derive(Deserialize)]
struct OfficialRegistryServerEntry {
    server: OfficialRegistryServer,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfficialRegistryServer {
    name: String,
    title: Option<String>,
    description: Option<String>,
    website_url: Option<String>,
    repository: Option<OfficialRegistryRepository>,
    packages: Option<Vec<OfficialRegistryPackage>>,
    remotes: Option<Vec<OfficialRegistryRemote>>,
}

#[derive(Deserialize)]
struct OfficialRegistryRepository {
    url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfficialRegistryPackage {
    registry_type: Option<String>,
    identifier: Option<String>,
    version: Option<String>,
    runtime_hint: Option<String>,
    environment_variables: Option<Vec<OfficialRegistryInput>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfficialRegistryRemote {
    #[serde(rename = "type")]
    transport_type: Option<String>,
    url: Option<String>,
    headers: Option<Vec<OfficialRegistryInput>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfficialRegistryInput {
    name: Option<String>,
    description: Option<String>,
    is_required: Option<bool>,
    is_secret: Option<bool>,
}

fn normalize_mcp_lookup_key(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn trim_numeric_suffix(value: &str) -> &str {
    if let Some((base, suffix)) = value.rsplit_once('-') {
        if suffix.chars().all(|c| c.is_ascii_digit()) {
            return base;
        }
    }
    value
}

fn add_market_lookup_keys<'a>(
    map: &mut HashMap<String, McpLeaderboardEntry>,
    item: &'a McpMarketItem,
    key: &'a str,
) {
    let normalized = normalize_mcp_lookup_key(key);
    if normalized.is_empty() {
        return;
    }
    let incoming = McpLeaderboardEntry {
        rank: item.rank,
        popularity: item.popularity,
        url: item.url.clone(),
    };
    match map.get(&normalized) {
        Some(existing) if existing.rank <= incoming.rank => {}
        _ => {
            map.insert(normalized, incoming);
        }
    }
}

fn build_market_index(items: &[McpMarketItem]) -> HashMap<String, McpLeaderboardEntry> {
    let mut out = HashMap::new();
    for item in items {
        add_market_lookup_keys(&mut out, item, &item.name);
        add_market_lookup_keys(&mut out, item, &item.slug);
        add_market_lookup_keys(&mut out, item, trim_numeric_suffix(&item.slug));
    }
    out
}

fn find_market_entry(
    leaderboard: &HashMap<String, McpLeaderboardEntry>,
    display_name: &str,
    id: &str,
    package_name: Option<&str>,
) -> Option<McpLeaderboardEntry> {
    let mut keys = Vec::new();
    keys.push(display_name.to_string());
    keys.push(id.to_string());
    if let Some((_, tail)) = id.rsplit_once('/') {
        keys.push(tail.to_string());
    }
    if let Some((_, tail)) = id.rsplit_once('.') {
        keys.push(tail.to_string());
    }
    if let Some(pkg) = package_name {
        keys.push(pkg.to_string());
        if let Some((_, tail)) = pkg.rsplit_once('/') {
            keys.push(tail.to_string());
        }
    }

    for key in keys {
        let normalized = normalize_mcp_lookup_key(&key);
        if normalized.is_empty() {
            continue;
        }
        if let Some(entry) = leaderboard.get(&normalized) {
            return Some(entry.clone());
        }
    }
    None
}

fn apply_market_entry(
    item: &mut McpCatalogItemView,
    leaderboard: &HashMap<String, McpLeaderboardEntry>,
) {
    if let Some(entry) = find_market_entry(
        leaderboard,
        &item.display_name,
        &item.id,
        item.package_name.as_deref(),
    ) {
        item.popularity_rank = Some(entry.rank);
        item.popularity_value = Some(entry.popularity);
        item.popularity_source = Some("mcpmarket".to_string());
        if item.docs_url.is_none() {
            item.docs_url = Some(entry.url);
        }
    }
}

fn sort_mcp_catalog(items: &mut [McpCatalogItemView]) {
    items.sort_by(|a, b| {
        let a_rank = a.popularity_rank.unwrap_or(u32::MAX);
        let b_rank = b.popularity_rank.unwrap_or(u32::MAX);
        let a_kind = if a.kind == "preset" { 0u8 } else { 1u8 };
        let b_kind = if b.kind == "preset" { 0u8 } else { 1u8 };
        a_rank
            .cmp(&b_rank)
            .then_with(|| a_kind.cmp(&b_kind))
            .then_with(|| a.display_name.cmp(&b.display_name))
    });
}

fn mcp_query_terms(query: &str) -> Vec<String> {
    query
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| part.len() >= 2)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
}

fn mcp_searchable_text(item: &McpCatalogItemView) -> String {
    format!(
        "{} {} {} {} {} {}",
        item.id,
        item.display_name,
        item.description,
        item.aliases.join(" "),
        item.keywords.join(" "),
        item.docs_url.clone().unwrap_or_default()
    )
    .to_ascii_lowercase()
}

fn required_env_count(item: &McpCatalogItemView) -> usize {
    item.env.iter().filter(|env| env.required).count()
}

fn mcp_supports_oauth(item: &McpCatalogItemView) -> bool {
    let text = mcp_searchable_text(item);
    item.env.iter().any(|env| {
        let key = env.key.to_ascii_lowercase();
        key.contains("client_id")
            || key.contains("client_secret")
            || key.contains("refresh_token")
            || key.contains("access_token")
            || key.contains("oauth")
    }) || text.contains("oauth")
}

fn mcp_requires_remote_auth(item: &McpCatalogItemView) -> bool {
    item.transport.as_deref() == Some("http")
        && item.env.iter().any(|env| {
            let key = env.key.to_ascii_lowercase();
            key.contains("authorization")
                || key.contains("header")
                || key.contains("token")
                || key.contains("api_key")
        })
}

fn mcp_requires_token(item: &McpCatalogItemView) -> bool {
    item.env.iter().any(|env| {
        let key = env.key.to_ascii_lowercase();
        key.contains("token") || key.contains("api_key") || key.contains("secret")
    })
}

fn auth_profile(item: &McpCatalogItemView) -> &'static str {
    if mcp_supports_oauth(item) {
        "OAuth"
    } else if mcp_requires_remote_auth(item) {
        "Remote auth"
    } else if mcp_requires_token(item) {
        "API key / token"
    } else if item.env.is_empty() {
        "No credentials"
    } else {
        "Manual configuration"
    }
}

fn setup_effort_label(item: &McpCatalogItemView) -> &'static str {
    let required_env = required_env_count(item);
    if mcp_supports_oauth(item) || required_env >= 4 {
        "Advanced"
    } else if mcp_requires_remote_auth(item)
        || mcp_requires_token(item)
        || required_env >= 2
        || item.transport.as_deref() == Some("http")
    {
        "Moderate"
    } else {
        "Easy"
    }
}

fn preflight_checks(item: &McpCatalogItemView, query: &str) -> Vec<String> {
    let mut checks = Vec::new();
    let query_trimmed = query.trim();
    if !query_trimmed.is_empty() {
        checks.push(format!(
            "Confirm this server really matches your intent: {}.",
            query_trimmed
        ));
    }
    if mcp_supports_oauth(item) {
        checks.push(
            "Have access to the provider developer console to create an OAuth app/client."
                .to_string(),
        );
        checks.push(
            "Be ready to configure redirect/consent settings and approve the required scopes."
                .to_string(),
        );
    } else if mcp_requires_token(item) {
        checks.push(
            "Make sure you can generate an API key or access token in the provider dashboard."
                .to_string(),
        );
    }
    if item.transport.as_deref() == Some("http") {
        checks.push("Verify the remote MCP endpoint is already live and that you know the required headers.".to_string());
    } else if !item.command.trim().is_empty() {
        checks.push(format!(
            "Local runtime will execute: {} {}.",
            item.command,
            item.args.join(" ")
        ));
    }
    if required_env_count(item) > 0 {
        checks.push(format!(
            "Prepare {} required environment value(s) before starting the wizard.",
            required_env_count(item)
        ));
    }
    if item.docs_url.is_some() {
        checks.push("Keep the linked documentation open while filling credentials.".to_string());
    }
    checks.truncate(4);
    checks
}

fn decision_tags(item: &McpCatalogItemView) -> Vec<String> {
    let mut tags = Vec::new();
    match item.source.as_str() {
        "curated" => tags.push("Curated".to_string()),
        "official-registry" => tags.push("Official".to_string()),
        _ => {}
    }
    if let Some(rank) = item.popularity_rank {
        if rank <= 20 {
            tags.push("Popular".to_string());
        }
    }
    match setup_effort_label(item) {
        "Easy" => tags.push("Easiest setup".to_string()),
        "Advanced" => tags.push("Advanced".to_string()),
        _ => {}
    }
    match auth_profile(item) {
        "OAuth" => tags.push("Requires OAuth".to_string()),
        "Remote auth" => tags.push("Remote endpoint".to_string()),
        "API key / token" => tags.push("Needs token".to_string()),
        _ => {}
    }
    tags.truncate(4);
    tags
}

fn why_choose_reason(item: &McpCatalogItemView) -> String {
    if item.source == "curated" {
        "Choose this if you want the cleanest guided setup inside Homun.".to_string()
    } else if item.source == "official-registry" {
        "Choose this if you prefer an MCP listed in the official registry.".to_string()
    } else if item.transport.as_deref() == Some("http") {
        "Choose this if you prefer a hosted endpoint instead of installing a local runtime."
            .to_string()
    } else if item.popularity_rank.unwrap_or(u32::MAX) <= 25 {
        "Choose this if you want a widely used option with stronger community validation."
            .to_string()
    } else {
        "Choose this only if its features match your use case better than the recommended option."
            .to_string()
    }
}

fn tradeoff_reason(item: &McpCatalogItemView) -> String {
    if mcp_supports_oauth(item) {
        "Tradeoff: setup is heavier because OAuth credentials and consent flow are required."
            .to_string()
    } else if item.transport.as_deref() == Some("http") {
        "Tradeoff: depends on a remote endpoint and usually on custom authorization headers."
            .to_string()
    } else if required_env_count(item) >= 3 {
        "Tradeoff: you need several environment values before the connection can work.".to_string()
    } else if item.source == "npm" {
        "Tradeoff: package is less curated, so documentation and defaults may be rougher."
            .to_string()
    } else {
        "Tradeoff: not the simplest default starting point for a non-technical user.".to_string()
    }
}

fn annotate_query_items(items: &mut [McpCatalogItemView], query: &str) {
    for item in items.iter_mut() {
        item.setup_effort = setup_effort_label(item).to_string();
        item.auth_profile = auth_profile(item).to_string();
        item.preflight_checks = preflight_checks(item, query);
        item.decision_tags = decision_tags(item);
        item.why_choose = Some(why_choose_reason(item));
        item.tradeoff = Some(tradeoff_reason(item));
    }
}

fn recommendation_score(item: &McpCatalogItemView, query: &str) -> i64 {
    let query_lower = query.trim().to_ascii_lowercase();
    let terms = mcp_query_terms(query);
    let searchable = mcp_searchable_text(item);
    let name_text = format!("{} {}", item.display_name, item.id).to_ascii_lowercase();
    let mut score = 0i64;

    if !query_lower.is_empty() && searchable.contains(&query_lower) {
        score += 80;
    }
    if !query_lower.is_empty() && name_text.contains(&query_lower) {
        score += 70;
    }
    for term in terms {
        if name_text.contains(&term) {
            score += 24;
        } else if searchable.contains(&term) {
            score += 10;
        }
    }

    score += match item.source.as_str() {
        "curated" => 42,
        "official-registry" => 34,
        "npm" => 8,
        _ => 12,
    };
    score += if item.install_supported { 18 } else { -20 };
    score += if item.docs_url.is_some() { 12 } else { 0 };
    score += match item.transport.as_deref() {
        Some("stdio") => 8,
        Some("http") => 4,
        _ => 0,
    };
    score += (22usize.saturating_sub(required_env_count(item) * 4)) as i64;
    if item.env.len() > 5 {
        score -= ((item.env.len() - 5) as i64) * 2;
    }
    if let Some(rank) = item.popularity_rank {
        score += 120i64.saturating_sub(rank.min(120) as i64);
    }
    if item.package_name.is_some() {
        score += 4;
    }

    score
}

fn recommendation_reason(item: &McpCatalogItemView) -> String {
    let mut reasons = Vec::new();
    match item.source.as_str() {
        "curated" => reasons.push("curated by Homun for guided setup".to_string()),
        "official-registry" => reasons.push("listed in the official MCP registry".to_string()),
        _ => {}
    }
    if item.install_supported {
        reasons.push("works with the guided installer".to_string());
    }
    if let Some(rank) = item.popularity_rank {
        reasons.push(format!("ranked #{} in the MCPMarket Top 100", rank));
    }
    let required_env = required_env_count(item);
    if required_env <= 2 {
        reasons.push("requires only a small number of credentials".to_string());
    } else if required_env <= 4 {
        reasons.push("setup stays reasonably compact".to_string());
    }
    if item.docs_url.is_some() {
        reasons.push("documentation is linked for credential lookup".to_string());
    }

    if reasons.is_empty() {
        "best overall match for the requested service".to_string()
    } else {
        reasons.truncate(3);
        reasons.join(", ")
    }
}

fn apply_query_recommendation(items: &mut [McpCatalogItemView], query: &str) {
    for item in items.iter_mut() {
        item.recommended = false;
        item.recommended_reason = None;
    }
    if query.trim().is_empty() || items.is_empty() {
        return;
    }

    let best_index = items
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| {
            recommendation_score(a, query)
                .cmp(&recommendation_score(b, query))
                .then_with(|| {
                    let a_rank = a.popularity_rank.unwrap_or(u32::MAX);
                    let b_rank = b.popularity_rank.unwrap_or(u32::MAX);
                    b_rank.cmp(&a_rank)
                })
        })
        .map(|(idx, _)| idx);

    if let Some(idx) = best_index {
        items[idx].recommended = true;
        items[idx].recommended_reason = Some(recommendation_reason(&items[idx]));
        if !items[idx]
            .decision_tags
            .iter()
            .any(|tag| tag == "Recommended")
        {
            items[idx]
                .decision_tags
                .insert(0, "Recommended".to_string());
        }
    }
}

fn sort_mcp_catalog_for_query(items: &mut [McpCatalogItemView], query: &str) {
    items.sort_by(|a, b| {
        recommendation_score(b, query)
            .cmp(&recommendation_score(a, query))
            .then_with(|| {
                let a_rank = a.popularity_rank.unwrap_or(u32::MAX);
                let b_rank = b.popularity_rank.unwrap_or(u32::MAX);
                a_rank.cmp(&b_rank)
            })
            .then_with(|| a.display_name.cmp(&b.display_name))
    });
}

fn parse_market_item_list(value: &serde_json::Value, limit: usize) -> Vec<McpMarketItem> {
    let Some(item_list) = value
        .get("@type")
        .and_then(|v| v.as_str())
        .filter(|t| *t == "ItemList")
        .and_then(|_| value.get("itemListElement"))
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };

    item_list
        .iter()
        .filter_map(|entry| {
            let rank = entry
                .get("position")
                .and_then(|v| v.as_u64())
                .and_then(|n| u32::try_from(n).ok())?;
            let item = entry.get("item")?;
            let name = item.get("name").and_then(|v| v.as_str())?.to_string();
            let url = item.get("url").and_then(|v| v.as_str())?.to_string();
            let slug = url
                .split("/server/")
                .nth(1)
                .unwrap_or_default()
                .split('?')
                .next()
                .unwrap_or_default()
                .trim()
                .to_string();
            if slug.is_empty() {
                return None;
            }
            let popularity = item
                .get("interactionStatistic")
                .and_then(|v| v.get("userInteractionCount"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            Some(McpMarketItem {
                rank,
                name,
                slug,
                popularity,
                url,
            })
        })
        .take(limit)
        .collect()
}

async fn fetch_mcpmarket_live(limit: usize) -> Option<Vec<McpMarketItem>> {
    let client = reqwest::Client::builder()
        .user_agent("homun")
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;
    let response = client
        .get("https://mcpmarket.com/leaderboards")
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let html = response.text().await.ok()?;
    let script_re = regex::Regex::new(
        r#"(?s)<script[^>]*type=["']application/ld\+json["'][^>]*>(.*?)</script>"#,
    )
    .ok()?;
    for cap in script_re.captures_iter(&html) {
        let payload = cap.get(1).map(|m| m.as_str()).unwrap_or_default();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) {
            let items = parse_market_item_list(&value, limit);
            if !items.is_empty() {
                return Some(items);
            }
        }
    }
    None
}

async fn load_mcpmarket_fallback(limit: usize) -> Vec<McpMarketItem> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("static")
        .join("data")
        .join("mcpmarket-top100-fallback.json");
    let Ok(content) = tokio::fs::read_to_string(path).await else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_str::<McpMarketFallback>(&content) else {
        return Vec::new();
    };
    parsed
        .items
        .into_iter()
        .map(|i| McpMarketItem {
            rank: i.rank,
            name: i.name,
            slug: i.slug.clone(),
            popularity: i.popularity,
            url: i
                .url
                .unwrap_or_else(|| format!("https://mcpmarket.com/server/{}", i.slug)),
        })
        .take(limit)
        .collect()
}

async fn load_mcpmarket_index(limit: usize) -> HashMap<String, McpLeaderboardEntry> {
    static MCPMARKET_INDEX_CACHE: OnceLock<
        Mutex<Option<(Instant, HashMap<String, McpLeaderboardEntry>)>>,
    > = OnceLock::new();
    const CACHE_TTL: Duration = Duration::from_secs(15 * 60);

    let cache = MCPMARKET_INDEX_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((cached_at, data)) = guard.as_ref() {
            if cached_at.elapsed() < CACHE_TTL {
                return data.clone();
            }
        }
    }

    let fresh = if let Some(items) = fetch_mcpmarket_live(limit).await {
        build_market_index(&items)
    } else {
        let fallback = load_mcpmarket_fallback(limit).await;
        if fallback.is_empty() {
            tracing::warn!(
                "MCPMarket leaderboard unavailable; continuing without popularity ranking"
            );
            HashMap::new()
        } else {
            tracing::warn!(
                "Using bundled MCPMarket leaderboard fallback (live source unavailable)"
            );
            build_market_index(&fallback)
        }
    };

    if let Ok(mut guard) = cache.lock() {
        *guard = Some((Instant::now(), fresh.clone()));
    }

    fresh
}

fn package_command_and_args(package: &OfficialRegistryPackage) -> Option<(String, Vec<String>)> {
    let identifier = package.identifier.as_deref()?.trim();
    if identifier.is_empty() {
        return None;
    }
    let runtime_hint = package
        .runtime_hint
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let registry_type = package
        .registry_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let version = package
        .version
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());

    if runtime_hint == "npx" || registry_type == "npm" {
        let mut package_ref = identifier.to_string();
        if let Some(ver) = version {
            package_ref = format!("{}@{}", identifier, ver);
        }
        return Some((
            "npx".to_string(),
            vec!["-y".to_string(), package_ref.to_string()],
        ));
    }

    if runtime_hint == "uvx" || registry_type == "pypi" {
        let package_ref = if let Some(ver) = version {
            format!("{}=={}", identifier, ver)
        } else {
            identifier.to_string()
        };
        return Some(("uvx".to_string(), vec![package_ref]));
    }

    if runtime_hint == "docker" || registry_type == "oci" {
        return Some((
            "docker".to_string(),
            vec![
                "run".to_string(),
                "--rm".to_string(),
                "-i".to_string(),
                identifier.to_string(),
            ],
        ));
    }

    None
}

fn build_env_view(specs: &[OfficialRegistryInput]) -> Vec<McpCatalogEnvView> {
    specs
        .iter()
        .filter_map(|spec| {
            let key = spec.name.clone()?.trim().to_string();
            if key.is_empty() {
                return None;
            }
            Some(McpCatalogEnvView {
                key: key.clone(),
                description: spec
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("Value for {}", key)),
                required: spec.is_required.unwrap_or(false),
                secret: spec.is_secret.unwrap_or(false),
            })
        })
        .collect()
}

fn official_registry_entry_to_view(
    entry: OfficialRegistryServerEntry,
) -> Option<McpCatalogItemView> {
    let server = entry.server;
    let id = server.name.trim().to_string();
    if id.is_empty() {
        return None;
    }

    let display_name = server
        .title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| display_name_from_package(&id));

    let description = server
        .description
        .clone()
        .filter(|d| !d.trim().is_empty())
        .unwrap_or_else(|| "MCP server from official registry".to_string());

    let docs_url = server.website_url.or_else(|| {
        server
            .repository
            .as_ref()
            .and_then(|repo| repo.url.as_ref().map(ToString::to_string))
    });

    let packages = server.packages.unwrap_or_default();
    if let Some(pkg) = packages
        .iter()
        .find(|p| package_command_and_args(p).is_some())
    {
        let (command, args) = package_command_and_args(pkg)?;
        let env = build_env_view(pkg.environment_variables.as_deref().unwrap_or_default());
        return Some(McpCatalogItemView {
            kind: "registry".to_string(),
            source: "official-registry".to_string(),
            id,
            display_name,
            description,
            command,
            args,
            transport: Some("stdio".to_string()),
            url: None,
            install_supported: true,
            package_name: pkg.identifier.clone(),
            downloads_monthly: None,
            score: None,
            popularity_rank: None,
            popularity_value: None,
            popularity_source: None,
            env,
            docs_url,
            aliases: vec![],
            keywords: vec![],
            recommended: false,
            recommended_reason: None,
            decision_tags: vec![],
            setup_effort: "Moderate".to_string(),
            auth_profile: "Unknown".to_string(),
            preflight_checks: vec![],
            why_choose: None,
            tradeoff: None,
        });
    }

    let remotes = server.remotes.unwrap_or_default();
    if let Some(remote) = remotes.into_iter().find(|r| {
        matches!(
            r.transport_type
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "streamable-http" | "sse" | "http" | "https"
        ) && r.url.as_deref().map(str::trim).is_some()
    }) {
        let env = build_env_view(remote.headers.as_deref().unwrap_or_default());
        return Some(McpCatalogItemView {
            kind: "registry".to_string(),
            source: "official-registry".to_string(),
            id,
            display_name,
            description,
            command: String::new(),
            args: vec![],
            transport: Some("http".to_string()),
            url: remote.url,
            install_supported: true,
            package_name: None,
            downloads_monthly: None,
            score: None,
            popularity_rank: None,
            popularity_value: None,
            popularity_source: None,
            env,
            docs_url,
            aliases: vec![],
            keywords: vec![],
            recommended: false,
            recommended_reason: None,
            decision_tags: vec![],
            setup_effort: "Moderate".to_string(),
            auth_profile: "Unknown".to_string(),
            preflight_checks: vec![],
            why_choose: None,
            tradeoff: None,
        });
    }

    Some(McpCatalogItemView {
        kind: "registry".to_string(),
        source: "official-registry".to_string(),
        id,
        display_name,
        description,
        command: String::new(),
        args: vec![],
        transport: None,
        url: None,
        install_supported: false,
        package_name: None,
        downloads_monthly: None,
        score: None,
        popularity_rank: None,
        popularity_value: None,
        popularity_source: None,
        env: vec![],
        docs_url,
        aliases: vec![],
        keywords: vec![],
        recommended: false,
        recommended_reason: None,
        decision_tags: vec![],
        setup_effort: "Moderate".to_string(),
        auth_profile: "Unknown".to_string(),
        preflight_checks: vec![],
        why_choose: None,
        tradeoff: None,
    })
}

async fn fetch_official_registry_servers(
    search: Option<&str>,
    limit: usize,
) -> Vec<OfficialRegistryServerEntry> {
    let mut url = match reqwest::Url::parse("https://registry.modelcontextprotocol.io/v0/servers") {
        Ok(u) => u,
        Err(_) => return Vec::new(),
    };

    let safe_limit = limit.clamp(1, 100);
    url.query_pairs_mut()
        .append_pair("version", "latest")
        .append_pair("limit", &safe_limit.to_string());
    if let Some(q) = search.map(str::trim).filter(|q| !q.is_empty()) {
        url.query_pairs_mut().append_pair("search", q);
    }

    let client = match reqwest::Client::builder()
        .user_agent("homun")
        .timeout(Duration::from_secs(12))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    match client.get(url).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<OfficialRegistryResponse>().await {
                Ok(parsed) => parsed.servers.unwrap_or_default(),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to parse official MCP registry response");
                    Vec::new()
                }
            }
        }
        Ok(resp) => {
            tracing::warn!(
                status = %resp.status(),
                "Official MCP registry request returned non-success status"
            );
            Vec::new()
        }
        Err(e) => {
            tracing::warn!(error = %e, "Official MCP registry request failed");
            Vec::new()
        }
    }
}

async fn list_mcp_catalog() -> Json<Vec<McpCatalogItemView>> {
    let leaderboard = load_mcpmarket_index(100).await;
    let mut items = crate::skills::all_mcp_presets()
        .into_iter()
        .map(preset_to_view)
        .collect::<Vec<_>>();
    for item in &mut items {
        apply_market_entry(item, &leaderboard);
    }

    let mut seen = items
        .iter()
        .map(|i| i.id.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let official = fetch_official_registry_servers(None, 100).await;
    for entry in official {
        let Some(mut item) = official_registry_entry_to_view(entry) else {
            continue;
        };
        let dedupe_key = item.id.to_ascii_lowercase();
        if !seen.insert(dedupe_key) {
            continue;
        }
        apply_market_entry(&mut item, &leaderboard);
        items.push(item);
    }

    sort_mcp_catalog(&mut items);
    Json(items)
}

#[derive(Deserialize)]
struct McpSuggestQuery {
    q: String,
}

async fn suggest_mcp_catalog(
    Query(query): Query<McpSuggestQuery>,
) -> Json<Vec<McpCatalogItemView>> {
    let leaderboard = load_mcpmarket_index(100).await;
    let mut items = crate::skills::suggest_mcp_presets(&query.q)
        .into_iter()
        .map(preset_to_view)
        .collect::<Vec<_>>();
    for item in &mut items {
        apply_market_entry(item, &leaderboard);
    }
    sort_mcp_catalog(&mut items);
    Json(items)
}

async fn start_google_mcp_oauth(
    Json(req): Json<GoogleMcpOauthStartRequest>,
) -> Result<Json<GoogleMcpOauthStartResponse>, (StatusCode, Json<serde_json::Value>)> {
    if req.client_id.trim().is_empty() || req.redirect_uri.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "client_id and redirect_uri are required"
            })),
        ));
    }

    let state = uuid::Uuid::new_v4().to_string();
    let (auth_url, scopes) =
        build_google_mcp_oauth_url(&req.service, &req.client_id, &req.redirect_uri, &state)
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
            })?;

    Ok(Json(GoogleMcpOauthStartResponse {
        ok: true,
        auth_url: auth_url.to_string(),
        redirect_uri: req.redirect_uri.trim().to_string(),
        scopes,
        state,
    }))
}

async fn exchange_google_mcp_oauth_code(
    Json(req): Json<GoogleMcpOauthExchangeRequest>,
) -> Result<Json<GoogleMcpOauthExchangeResponse>, (StatusCode, Json<serde_json::Value>)> {
    if req.client_id.trim().is_empty()
        || req.client_secret.trim().is_empty()
        || req.code.trim().is_empty()
        || req.redirect_uri.trim().is_empty()
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "service, code, client_id, client_secret, and redirect_uri are required"
            })),
        ));
    }

    if google_mcp_scopes(&req.service).is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Unsupported Google MCP OAuth service: {}", req.service)
            })),
        ));
    }

    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", req.code.trim()),
            ("client_id", req.client_id.trim()),
            ("client_secret", req.client_secret.trim()),
            ("redirect_uri", req.redirect_uri.trim()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    let status = response.status();
    let body = response
        .json::<GoogleMcpOauthTokenResponse>()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    if !status.is_success() {
        let detail = body
            .error_description
            .clone()
            .or(body.error.clone())
            .unwrap_or_else(|| "Google OAuth token exchange failed".to_string());
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": detail })),
        ));
    }

    let message = if body.refresh_token.as_deref().unwrap_or_default().is_empty() {
        Some(
            "Token exchange succeeded, but Google did not return a refresh token. Retry consent with prompt=consent and offline access."
                .to_string(),
        )
    } else {
        Some("Google OAuth token exchange succeeded.".to_string())
    };

    Ok(Json(GoogleMcpOauthExchangeResponse {
        ok: true,
        access_token: body.access_token,
        refresh_token: body.refresh_token,
        expires_in: body.expires_in,
        scope: body.scope,
        token_type: body.token_type,
        message,
    }))
}

async fn start_github_mcp_oauth(
    Json(req): Json<GitHubMcpOauthStartRequest>,
) -> Result<Json<GitHubMcpOauthStartResponse>, (StatusCode, Json<serde_json::Value>)> {
    if req.client_id.trim().is_empty() || req.redirect_uri.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "client_id and redirect_uri are required"
            })),
        ));
    }

    let state = uuid::Uuid::new_v4().to_string();
    let (auth_url, scopes) =
        build_github_mcp_oauth_url(&req.service, &req.client_id, &req.redirect_uri, &state)
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
            })?;

    Ok(Json(GitHubMcpOauthStartResponse {
        ok: true,
        auth_url: auth_url.to_string(),
        redirect_uri: req.redirect_uri.trim().to_string(),
        scopes,
        state,
    }))
}

async fn exchange_github_mcp_oauth_code(
    Json(req): Json<GitHubMcpOauthExchangeRequest>,
) -> Result<Json<GitHubMcpOauthExchangeResponse>, (StatusCode, Json<serde_json::Value>)> {
    if req.client_id.trim().is_empty()
        || req.client_secret.trim().is_empty()
        || req.code.trim().is_empty()
        || req.redirect_uri.trim().is_empty()
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "service, code, client_id, client_secret, and redirect_uri are required"
            })),
        ));
    }

    if github_mcp_scopes(&req.service).is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Unsupported GitHub MCP OAuth service: {}", req.service)
            })),
        ));
    }

    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    let response = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", req.client_id.trim()),
            ("client_secret", req.client_secret.trim()),
            ("code", req.code.trim()),
            ("redirect_uri", req.redirect_uri.trim()),
        ])
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    let status = response.status();
    let body = response
        .json::<GitHubMcpOauthTokenResponse>()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    if !status.is_success() || body.access_token.is_none() {
        let detail = body
            .error_description
            .clone()
            .or(body.error.clone())
            .unwrap_or_else(|| "GitHub OAuth token exchange failed".to_string());
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": detail })),
        ));
    }

    Ok(Json(GitHubMcpOauthExchangeResponse {
        ok: true,
        access_token: body.access_token,
        scope: body.scope,
        token_type: body.token_type,
        message: Some("GitHub OAuth token exchange succeeded.".to_string()),
    }))
}

#[derive(Deserialize)]
struct McpSearchQuery {
    q: String,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct GoogleMcpOauthStartRequest {
    service: String,
    client_id: String,
    redirect_uri: String,
}

#[derive(Debug, Serialize)]
struct GoogleMcpOauthStartResponse {
    ok: bool,
    auth_url: String,
    redirect_uri: String,
    scopes: Vec<String>,
    state: String,
}

#[derive(Debug, Deserialize)]
struct GoogleMcpOauthExchangeRequest {
    service: String,
    code: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

#[derive(Debug, Deserialize)]
struct GoogleMcpOauthTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
    token_type: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Serialize)]
struct GoogleMcpOauthExchangeResponse {
    ok: bool,
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
    token_type: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubMcpOauthStartRequest {
    service: String,
    client_id: String,
    redirect_uri: String,
}

#[derive(Debug, Serialize)]
struct GitHubMcpOauthStartResponse {
    ok: bool,
    auth_url: String,
    redirect_uri: String,
    scopes: Vec<String>,
    state: String,
}

#[derive(Debug, Deserialize)]
struct GitHubMcpOauthExchangeRequest {
    service: String,
    code: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

#[derive(Debug, Deserialize)]
struct GitHubMcpOauthTokenResponse {
    access_token: Option<String>,
    scope: Option<String>,
    token_type: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Serialize)]
struct GitHubMcpOauthExchangeResponse {
    ok: bool,
    access_token: Option<String>,
    scope: Option<String>,
    token_type: Option<String>,
    message: Option<String>,
}

fn google_mcp_scopes(service: &str) -> Option<&'static [&'static str]> {
    let normalized = service.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "gmail" => Some(&["https://www.googleapis.com/auth/gmail.readonly"]),
        "google-calendar" | "gcal" | "calendar" => {
            Some(&["https://www.googleapis.com/auth/calendar"])
        }
        _ => None,
    }
}

fn github_mcp_scopes(service: &str) -> Option<&'static [&'static str]> {
    let normalized = service.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "github" | "gh" => Some(&["repo", "read:org", "read:user"]),
        _ => None,
    }
}

fn build_google_mcp_oauth_url(
    service: &str,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
) -> anyhow::Result<(reqwest::Url, Vec<String>)> {
    let scopes = google_mcp_scopes(service)
        .ok_or_else(|| anyhow::anyhow!("unsupported Google MCP OAuth service"))?
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut url = reqwest::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("client_id", client_id.trim());
        qp.append_pair("redirect_uri", redirect_uri.trim());
        qp.append_pair("response_type", "code");
        qp.append_pair("scope", &scopes.join(" "));
        qp.append_pair("access_type", "offline");
        qp.append_pair("prompt", "consent");
        qp.append_pair("include_granted_scopes", "true");
        qp.append_pair("state", state);
    }
    Ok((url, scopes))
}

fn build_github_mcp_oauth_url(
    service: &str,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
) -> anyhow::Result<(reqwest::Url, Vec<String>)> {
    let scopes = github_mcp_scopes(service)
        .ok_or_else(|| anyhow::anyhow!("unsupported GitHub MCP OAuth service"))?
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut url = reqwest::Url::parse("https://github.com/login/oauth/authorize")?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("client_id", client_id.trim());
        qp.append_pair("redirect_uri", redirect_uri.trim());
        qp.append_pair("scope", &scopes.join(" "));
        qp.append_pair("state", state);
    }
    Ok((url, scopes))
}

#[cfg(test)]
mod google_oauth_tests {
    use super::{
        build_github_mcp_oauth_url, build_google_mcp_oauth_url, github_mcp_scopes,
        google_mcp_scopes,
    };

    #[test]
    fn google_oauth_scopes_support_known_services() {
        assert_eq!(
            google_mcp_scopes("gmail"),
            Some(&["https://www.googleapis.com/auth/gmail.readonly"][..])
        );
        assert_eq!(
            google_mcp_scopes("google-calendar"),
            Some(&["https://www.googleapis.com/auth/calendar"][..])
        );
        assert!(google_mcp_scopes("github").is_none());
    }

    #[test]
    fn build_google_oauth_url_contains_offline_access_flags() {
        let (url, scopes) = build_google_mcp_oauth_url(
            "gmail",
            "client-123",
            "http://localhost:8080/mcp/oauth/google/callback",
            "state-xyz",
        )
        .expect("oauth url");
        let rendered = url.as_str().to_string();
        assert_eq!(
            scopes,
            vec!["https://www.googleapis.com/auth/gmail.readonly"]
        );
        assert!(rendered.contains("access_type=offline"));
        assert!(rendered.contains("prompt=consent"));
        assert!(rendered.contains("state=state-xyz"));
    }

    #[test]
    fn github_oauth_scopes_support_github_service() {
        assert_eq!(
            github_mcp_scopes("github"),
            Some(&["repo", "read:org", "read:user"][..])
        );
        assert!(github_mcp_scopes("gmail").is_none());
    }

    #[test]
    fn build_github_oauth_url_contains_state_and_scope() {
        let (url, scopes) = build_github_mcp_oauth_url(
            "github",
            "gh-client",
            "http://localhost:8080/mcp/oauth/github/callback",
            "state-123",
        )
        .expect("oauth url");
        let rendered = url.as_str().to_string();
        assert_eq!(scopes, vec!["repo", "read:org", "read:user"]);
        assert!(rendered.contains("client_id=gh-client"));
        assert!(rendered.contains("state=state-123"));
        assert!(rendered.contains("scope=repo+read%3Aorg+read%3Auser"));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpInstallGuideEnvSpec {
    key: String,
    description: Option<String>,
    required: Option<bool>,
    secret: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpInstallGuideRequest {
    id: String,
    display_name: Option<String>,
    description: Option<String>,
    docs_url: Option<String>,
    transport: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
    env: Option<Vec<McpInstallGuideEnvSpec>>,
    language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpInstallGuideEnvHelp {
    key: String,
    why: String,
    where_to_get: String,
    format_hint: String,
    vault_hint: String,
    #[serde(default)]
    retrieval_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpInstallGuideDocumentation {
    url: Option<String>,
    summary: String,
    #[serde(default)]
    highlights: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpInstallGuideResponse {
    ok: bool,
    source: String, // llm | docs | fallback
    summary: String,
    steps: Vec<String>,
    env_help: Vec<McpInstallGuideEnvHelp>,
    notes: Vec<String>,
    documentation: Option<McpInstallGuideDocumentation>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct McpInstallGuideLlmParsed {
    summary: String,
    #[serde(default)]
    steps: Vec<String>,
    #[serde(default)]
    env_help: Vec<McpInstallGuideEnvHelp>,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Debug, Clone)]
struct McpInstallGuideDocsContext {
    url: Option<String>,
    summary: String,
    highlights: Vec<String>,
    text_excerpt: String,
}

#[derive(Clone, Copy)]
enum GuideLanguage {
    English,
    Italian,
}

impl GuideLanguage {
    fn from_request(value: Option<&str>) -> Self {
        match value.unwrap_or("en").trim().to_ascii_lowercase().as_str() {
            "it" | "it-it" | "italian" | "italiano" => Self::Italian,
            _ => Self::English,
        }
    }

    fn llm_label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Italian => "Italian",
        }
    }

    fn is_italian(self) -> bool {
        matches!(self, Self::Italian)
    }
}

fn extract_json_object_block(input: &str) -> Option<&str> {
    let start = input.find('{')?;
    let end = input.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(&input[start..=end])
}

fn strip_html_tags_for_docs(html: &str) -> String {
    let mut result = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if in_script || in_style {
            if chars[i] == '<' && i + 1 < chars.len() && chars[i + 1] == '/' {
                let rest: String = chars[i..].iter().take(20).collect();
                let rest_lower = rest.to_ascii_lowercase();
                if in_script && rest_lower.starts_with("</script") {
                    in_script = false;
                } else if in_style && rest_lower.starts_with("</style") {
                    in_style = false;
                }
            }
            i += 1;
            continue;
        }

        if chars[i] == '<' {
            let rest: String = chars[i..].iter().take(20).collect();
            let rest_lower = rest.to_ascii_lowercase();
            if rest_lower.starts_with("<script") {
                in_script = true;
            } else if rest_lower.starts_with("<style") {
                in_style = true;
            }
            in_tag = true;
            i += 1;
            continue;
        }

        if chars[i] == '>' && in_tag {
            in_tag = false;
            result.push('\n');
            i += 1;
            continue;
        }

        if !in_tag {
            result.push(chars[i]);
        }
        i += 1;
    }

    result
}

fn normalize_docs_text(input: &str) -> String {
    input
        .replace('\r', "\n")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn split_docs_fragments(input: &str) -> Vec<String> {
    let normalized = normalize_docs_text(input);
    let mut out = Vec::new();
    for line in normalized.lines() {
        for chunk in line.split(['.', ';']) {
            let trimmed = chunk.trim();
            if trimmed.len() < 12 || trimmed.len() > 260 {
                continue;
            }
            out.push(trimmed.to_string());
        }
    }
    out
}

fn docs_search_terms(spec: &McpInstallGuideEnvSpec) -> Vec<String> {
    let key = spec.key.to_ascii_lowercase();
    let mut terms = vec![key.clone(), key.replace('_', " ")];

    if key.contains("token") {
        terms.push("token".to_string());
    }
    if key.contains("api_key") || key.contains("apikey") {
        terms.push("api key".to_string());
    }
    if key.contains("client_id") {
        terms.push("client id".to_string());
    }
    if key.contains("client_secret") {
        terms.push("client secret".to_string());
    }
    if key.contains("refresh_token") {
        terms.push("refresh token".to_string());
    }
    if key.contains("access_token") {
        terms.push("access token".to_string());
    }
    if key.contains("authorization") {
        terms.push("authorization".to_string());
        terms.push("bearer".to_string());
    }

    terms.sort();
    terms.dedup();
    terms
}

fn find_relevant_doc_fragments(
    docs: &McpInstallGuideDocsContext,
    spec: Option<&McpInstallGuideEnvSpec>,
    limit: usize,
) -> Vec<String> {
    let mut terms = vec![
        "install".to_string(),
        "setup".to_string(),
        "authentication".to_string(),
        "oauth".to_string(),
        "environment variable".to_string(),
        "configuration".to_string(),
    ];
    if let Some(spec) = spec {
        terms.extend(docs_search_terms(spec));
    }

    let mut results = Vec::new();
    for fragment in docs.text_excerpt.lines() {
        let lower = fragment.to_ascii_lowercase();
        if terms.iter().any(|term| lower.contains(term)) {
            let value = fragment.trim();
            if !value.is_empty() && !results.iter().any(|r: &String| r == value) {
                results.push(value.to_string());
            }
        }
        if results.len() >= limit {
            break;
        }
    }
    results
}

fn docs_context_to_view(docs: &McpInstallGuideDocsContext) -> Option<McpInstallGuideDocumentation> {
    if docs.summary.trim().is_empty() && docs.highlights.is_empty() {
        return None;
    }
    Some(McpInstallGuideDocumentation {
        url: docs.url.clone(),
        summary: docs.summary.clone(),
        highlights: docs.highlights.clone(),
    })
}

fn parse_github_repo(url: &str) -> Option<(String, String)> {
    let parsed = reqwest::Url::parse(url).ok()?;
    if parsed.domain()? != "github.com" {
        return None;
    }
    let mut segs = parsed
        .path_segments()?
        .filter(|seg| !seg.trim().is_empty())
        .take(2);
    let owner = segs.next()?.to_string();
    let repo = segs.next()?.trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

async fn fetch_docs_text(url: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .user_agent("homun")
        .timeout(Duration::from_secs(12))
        .build()
        .ok()?;

    if let Some((owner, repo)) = parse_github_repo(url) {
        let api_url = format!("https://api.github.com/repos/{owner}/{repo}/readme");
        if let Ok(resp) = client
            .get(&api_url)
            .header("Accept", "application/vnd.github.raw")
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(text) = resp.text().await {
                    let normalized = normalize_docs_text(&text);
                    if !normalized.is_empty() {
                        return Some(normalized);
                    }
                }
            }
        }
    }

    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let raw = resp.text().await.ok()?;
    let stripped = if raw.contains("<html") || raw.contains("<HTML") {
        strip_html_tags_for_docs(&raw)
    } else {
        raw
    };
    let normalized = normalize_docs_text(&stripped);
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

async fn fetch_install_docs_context(
    req: &McpInstallGuideRequest,
) -> Option<McpInstallGuideDocsContext> {
    let url = req.docs_url.as_deref()?.trim();
    if url.is_empty() {
        return None;
    }

    let text = fetch_docs_text(url).await?;
    let fragments = split_docs_fragments(&text);
    if fragments.is_empty() {
        return None;
    }

    let mut highlights = Vec::new();
    let keywords = [
        "install",
        "setup",
        "oauth",
        "authentication",
        "environment variable",
        "token",
        "api key",
        "client id",
        "client secret",
        "refresh token",
        "authorization",
        "bearer",
    ];
    for fragment in &fragments {
        let lower = fragment.to_ascii_lowercase();
        if keywords.iter().any(|keyword| lower.contains(keyword))
            && !highlights.iter().any(|line| line == fragment)
        {
            highlights.push(fragment.clone());
        }
        if highlights.len() >= 6 {
            break;
        }
    }

    let summary = if highlights.is_empty() {
        fragments
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(". ")
    } else {
        highlights
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(". ")
    };

    let excerpt = fragments
        .into_iter()
        .take(120)
        .collect::<Vec<_>>()
        .join("\n");
    Some(McpInstallGuideDocsContext {
        url: Some(url.to_string()),
        summary,
        highlights,
        text_excerpt: excerpt,
    })
}

fn infer_service_tag(req: &McpInstallGuideRequest, spec: &McpInstallGuideEnvSpec) -> &'static str {
    let text = format!(
        "{} {} {} {} {}",
        req.id,
        req.display_name.clone().unwrap_or_default(),
        req.description.clone().unwrap_or_default(),
        req.docs_url.clone().unwrap_or_default(),
        spec.description.clone().unwrap_or_default()
    )
    .to_ascii_lowercase();
    if text.contains("google") || spec.key.to_ascii_lowercase().starts_with("google_") {
        "google"
    } else if text.contains("github") || spec.key.to_ascii_lowercase().contains("github") {
        "github"
    } else if text.contains("notion") || spec.key.to_ascii_lowercase().contains("notion") {
        "notion"
    } else if text.contains("aws") || spec.key.to_ascii_lowercase().contains("aws_") {
        "aws"
    } else {
        "generic"
    }
}

fn is_generic_where_to_get(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("open docs")
        || lower.contains("apri la documentazione")
        || lower.contains("apri i docs")
        || lower.contains("service dashboard")
        || lower.contains("dashboard del servizio")
        || lower.contains("open the server documentation")
}

fn fallback_env_help_for_spec(
    spec: &McpInstallGuideEnvSpec,
    req: &McpInstallGuideRequest,
    docs: Option<&McpInstallGuideDocsContext>,
    language: GuideLanguage,
) -> McpInstallGuideEnvHelp {
    let key = spec.key.trim();
    let k = key.to_ascii_lowercase();
    let service = infer_service_tag(req, spec);
    let is_secret = spec.secret.unwrap_or(false)
        || k.contains("token")
        || k.contains("secret")
        || k.contains("password")
        || k.contains("api_key")
        || k.contains("authorization");

    let mut out = McpInstallGuideEnvHelp {
        key: key.to_string(),
        why: spec
            .description
            .clone()
            .filter(|d| !d.trim().is_empty())
            .unwrap_or_else(|| {
                if language.is_italian() {
                    "Richiesto dal server MCP per autenticazione o configurazione dell'accesso."
                        .to_string()
                } else {
                    "Required by the MCP server to authenticate or configure access.".to_string()
                }
            }),
        where_to_get: req
            .docs_url
            .as_ref()
            .map(|d| {
                if language.is_italian() {
                    format!(
                        "Apri {} e cerca \"{}\" nelle sezioni installazione, autenticazione o variabili ambiente.",
                        d, key
                    )
                } else {
                    format!(
                        "Open {} and search \"{}\" in Installation/Authentication/Environment Variables.",
                        d, key
                    )
                }
            })
            .unwrap_or_else(|| {
                if language.is_italian() {
                    format!(
                        "Apri la documentazione del server e cerca \"{}\" nelle sezioni autenticazione o environment.",
                        key
                    )
                } else {
                    format!(
                        "Open server docs and search \"{}\" in authentication/environment sections.",
                        key
                    )
                }
            }),
        format_hint: if k.contains("authorization") {
            format!("{key}=Bearer <token>")
        } else if is_secret {
            format!("{key}=<secret>")
        } else if k.contains("url") || k.contains("endpoint") {
            format!("{key}=https://...")
        } else {
            format!("{key}=<value>")
        },
        vault_hint: if language.is_italian() {
            format!(
                "Preferisci un riferimento Vault: {key}=vault://mcp.{}",
                k.replace('_', ".")
            )
        } else {
            format!(
                "Prefer vault reference: {key}=vault://mcp.{}",
                k.replace('_', ".")
            )
        },
        retrieval_steps: vec![
            if language.is_italian() {
                format!("Trova `{}` nella documentazione env/auth del server.", key)
            } else {
                format!("Find `{}` in the server env/auth documentation.", key)
            },
            if language.is_italian() {
                "Genera o copia il valore richiesto dalla dashboard o console del provider."
                    .to_string()
            } else {
                "Generate or copy the required value from the provider dashboard/console."
                    .to_string()
            },
            if language.is_italian() {
                "Salvalo nel Vault e usa un riferimento vault:// nelle env.".to_string()
            } else {
                "Save in Vault and use vault:// reference in env.".to_string()
            },
        ],
    };

    if service == "github" && (k.contains("token") || k.contains("pat")) {
        out.why = if language.is_italian() {
            "Token GitHub usato per accedere a repository, issue e pull request.".to_string()
        } else {
            "GitHub token used to access repositories, issues, and pull requests.".to_string()
        };
        out.where_to_get = if language.is_italian() {
            "GitHub -> Settings -> Developer settings -> Personal access tokens.".to_string()
        } else {
            "GitHub -> Settings -> Developer settings -> Personal access tokens.".to_string()
        };
        out.retrieval_steps = vec![
            if language.is_italian() {
                "Apri la pagina GitHub Personal Access Tokens.".to_string()
            } else {
                "Open GitHub Personal Access Tokens page.".to_string()
            },
            if language.is_italian() {
                "Crea un token con gli scope richiesti da questa integrazione MCP.".to_string()
            } else {
                "Create token with scopes required by this MCP integration.".to_string()
            },
            if language.is_italian() {
                "Copia il token una sola volta e salvalo nel Vault.".to_string()
            } else {
                "Copy token once and store it in Vault.".to_string()
            },
        ];
    } else if service == "notion" && k.contains("token") {
        out.why = if language.is_italian() {
            "Token di integrazione Notion per accedere a workspace e database.".to_string()
        } else {
            "Notion integration token for workspace/database access.".to_string()
        };
        out.where_to_get =
            "Notion -> Settings & members -> Integrations -> Develop your own integrations."
                .to_string();
    } else if service == "google"
        && (k.contains("google_client_id") || k.contains("google_client_secret"))
    {
        out.why = if language.is_italian() {
            "Credenziali OAuth client usate per le API Google.".to_string()
        } else {
            "OAuth client credentials used for Google APIs.".to_string()
        };
        out.where_to_get =
            "Google Cloud Console -> APIs & Services -> Credentials -> OAuth client ID."
                .to_string();
        out.retrieval_steps = vec![
            if language.is_italian() {
                "Crea o seleziona un progetto in Google Cloud.".to_string()
            } else {
                "Create/select project in Google Cloud.".to_string()
            },
            if language.is_italian() {
                "Configura la schermata consenso OAuth e gli scope richiesti.".to_string()
            } else {
                "Configure OAuth consent screen and required scopes.".to_string()
            },
            if language.is_italian() {
                "Crea un OAuth Client ID e copia client_id e client_secret.".to_string()
            } else {
                "Create OAuth Client ID and copy client_id/client_secret.".to_string()
            },
        ];
    } else if service == "google" && k.contains("google_refresh_token") {
        out.why = if language.is_italian() {
            "Refresh token per ottenere nuovi access token Google senza rifare il login."
                .to_string()
        } else {
            "Refresh token to obtain new Google access tokens without re-login.".to_string()
        };
        out.where_to_get = if language.is_italian() {
            "Si genera durante il flusso OAuth con accesso offline abilitato.".to_string()
        } else {
            "Generate during OAuth consent flow with offline access enabled.".to_string()
        };
        out.retrieval_steps = vec![
            if language.is_italian() {
                "Esegui il flusso OAuth authorization code con offline access.".to_string()
            } else {
                "Run OAuth authorization code flow with offline access.".to_string()
            },
            if language.is_italian() {
                "Scambia il codice autorizzativo con i token e copia refresh_token.".to_string()
            } else {
                "Exchange auth code for tokens and copy refresh_token.".to_string()
            },
            if language.is_italian() {
                "Usa il refresh token nelle env per evitare aggiornamenti manuali frequenti."
                    .to_string()
            } else {
                "Use refresh token in env to avoid frequent manual updates.".to_string()
            },
        ];
    } else if service == "google" && k.contains("google_access_token") {
        out.why = if language.is_italian() {
            "Access token OAuth Google a breve durata usato per autorizzare le API.".to_string()
        } else {
            "Short-lived Google OAuth access token for API authorization.".to_string()
        };
        out.where_to_get = if language.is_italian() {
            "Si ottiene dallo scambio token OAuth (OAuth Playground o il tuo flusso OAuth)."
                .to_string()
        } else {
            "Generated by OAuth token exchange (OAuth Playground or your OAuth flow).".to_string()
        };
        out.retrieval_steps = vec![
            if language.is_italian() {
                "Autorizza l'app ed esegui lo scambio codice presso l'endpoint token OAuth Google."
                    .to_string()
            } else {
                "Authorize and exchange code at Google OAuth token endpoint.".to_string()
            },
            if language.is_italian() {
                "Copia `access_token` e annota la scadenza.".to_string()
            } else {
                "Copy `access_token` and note expiration time.".to_string()
            },
            if language.is_italian() {
                "Se supportato, preferisci il refresh token invece di aggiornare manualmente l'access token."
                    .to_string()
            } else {
                "If supported, prefer refresh token flow instead of manual access token updates."
                    .to_string()
            },
        ];
    } else if service == "aws"
        && (k.contains("aws_access_key_id") || k.contains("aws_secret_access_key"))
    {
        out.why = if language.is_italian() {
            "Credenziali AWS IAM usate dal server MCP.".to_string()
        } else {
            "AWS IAM credentials used by the MCP server.".to_string()
        };
        out.where_to_get =
            "AWS Console -> IAM -> Users -> Security credentials -> Create access key.".to_string();
    } else if k.contains("refresh_token") {
        out.why = if language.is_italian() {
            "Refresh token usato per ottenere automaticamente nuovi access token.".to_string()
        } else {
            "Refresh token used to obtain new access tokens automatically.".to_string()
        };
        out.where_to_get = if language.is_italian() {
            "Generalo in un flusso OAuth Authorization Code con offline access o scope equivalente."
                .to_string()
        } else {
            "Generate in OAuth Authorization Code flow with offline access/scope enabled."
                .to_string()
        };
    } else if k.contains("access_token") {
        out.why = if language.is_italian() {
            "Access token usato per autorizzare le API, di solito con durata breve.".to_string()
        } else {
            "Access token used for API authorization, usually short-lived.".to_string()
        };
        out.where_to_get = if language.is_italian() {
            "Generalo tramite endpoint OAuth/token del provider o dashboard API token.".to_string()
        } else {
            "Generate through provider OAuth/token endpoint or API token dashboard.".to_string()
        };
    } else if k.contains("token") || k.contains("api_key") || k.contains("secret") {
        out.why = if language.is_italian() {
            "Credenziale di autenticazione richiesta dal servizio o API di destinazione."
                .to_string()
        } else {
            "Authentication credential required by the target API/service.".to_string()
        };
        out.where_to_get = req
            .docs_url
            .as_deref()
            .map(|d| {
                if language.is_italian() {
                    format!(
                        "Apri la documentazione e segui la sezione autenticazione: {}",
                        d
                    )
                } else {
                    format!("Open docs and follow authentication section: {}", d)
                }
            })
            .unwrap_or_else(|| {
                if language.is_italian() {
                    "Apri la dashboard del servizio e crea un token o API key.".to_string()
                } else {
                    "Open the service dashboard and create an API token/key.".to_string()
                }
            });
    } else if k.contains("authorization") {
        out.why = if language.is_italian() {
            "Valore dell'header Authorization richiesto dall'endpoint MCP remoto.".to_string()
        } else {
            "Authorization header value expected by the remote MCP endpoint.".to_string()
        };
        out.where_to_get = req
            .docs_url
            .as_deref()
            .map(|d| {
                if language.is_italian() {
                    format!(
                        "Controlla la documentazione per il formato dell'header (es. Bearer token): {}",
                        d
                    )
                } else {
                    format!("Check docs for header format (e.g. Bearer token): {}", d)
                }
            })
            .unwrap_or_else(|| {
                if language.is_italian() {
                    "Controlla la documentazione dell'endpoint per il formato richiesto dell'header Authorization."
                        .to_string()
                } else {
                    "Check endpoint docs for required Authorization header format.".to_string()
                }
            });
        out.format_hint = format!("{key}=Bearer <token>");
    }

    if let Some(docs) = docs {
        let clues = find_relevant_doc_fragments(docs, Some(spec), 3);
        if !clues.is_empty() {
            out.where_to_get = if let Some(url) = docs.url.as_deref() {
                if language.is_italian() {
                    format!("Apri {} e cerca questo passaggio: {}", url, clues[0])
                } else {
                    format!("Open {} and look for: {}", url, clues[0])
                }
            } else {
                if language.is_italian() {
                    format!("Cerca nella documentazione questo passaggio: {}", clues[0])
                } else {
                    format!("Look in documentation for: {}", clues[0])
                }
            };
            out.retrieval_steps = clues
                .iter()
                .map(|clue| {
                    if language.is_italian() {
                        format!("Nella documentazione trova questo indizio: {}", clue)
                    } else {
                        format!("In the docs, locate this clue: {}", clue)
                    }
                })
                .collect();
            if out.retrieval_steps.len() < 3 {
                out.retrieval_steps.push(if language.is_italian() {
                    "Copia il formato esatto del valore dalla documentazione, poi salva i segreti nel Vault."
                        .to_string()
                } else {
                    "Copy the exact value format from docs, then store secrets in Vault."
                        .to_string()
                });
            }
        }
    }

    out
}

fn build_fallback_install_guide(
    req: &McpInstallGuideRequest,
    docs: Option<&McpInstallGuideDocsContext>,
    error: Option<String>,
    language: GuideLanguage,
) -> McpInstallGuideResponse {
    let env_specs = req.env.clone().unwrap_or_default();
    let env_help = env_specs
        .iter()
        .map(|spec| fallback_env_help_for_spec(spec, req, docs, language))
        .collect::<Vec<_>>();

    let display_name = req
        .display_name
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(&req.id);
    let transport = req.transport.clone().unwrap_or_else(|| "stdio".to_string());
    let command = req.command.clone().unwrap_or_default();
    let args = req.args.clone().unwrap_or_default().join(" ");

    let mut steps = vec![
        if let Some(docs) = req.docs_url.clone().filter(|d| !d.trim().is_empty()) {
            if language.is_italian() {
                format!("Apri la documentazione di {}: {}", display_name, docs)
            } else {
                format!("Open docs for {}: {}", display_name, docs)
            }
        } else {
            if language.is_italian() {
                format!("Rivedi questo server: {} ({})", display_name, req.id)
            } else {
                format!("Review this server: {} ({})", display_name, req.id)
            }
        },
        if language.is_italian() {
            "Raccogli le credenziali richieste con la guida qui sotto e salva i segreti nel Vault (vault://...)".to_string()
        } else {
            "Collect required credentials with the guidance below and store secrets in Vault (vault://...)".to_string()
        },
        if language.is_italian() {
            "Salva la configurazione del server ed esegui Test per verificare la connessione."
                .to_string()
        } else {
            "Save the server configuration and run Test to validate connectivity.".to_string()
        },
    ];
    if transport == "http" {
        steps.insert(
            1,
            if language.is_italian() {
                "Conferma URL endpoint e valori richiesti per Authorization/header.".to_string()
            } else {
                "Confirm endpoint URL and required Authorization/header values".to_string()
            },
        );
    } else if !command.trim().is_empty() {
        steps.insert(
            1,
            if language.is_italian() {
                format!("Comando runtime: {} {}", command, args)
            } else {
                format!("Runtime command: {} {}", command, args)
            },
        );
    }

    McpInstallGuideResponse {
        ok: true,
        source: "fallback".to_string(),
        summary: if language.is_italian() {
            format!(
                "Configurazione guidata per {}. Leggi i passaggi, compila le variabili env, salva ed esegui il test di connessione.",
                display_name
            )
        } else {
            format!(
                "Guided setup for {}. Read the steps, fill env variables, save, and run connection test.",
                display_name
            )
        },
        steps,
        env_help,
        notes: vec![
            if language.is_italian() {
                "Usa riferimenti Vault per i segreti: KEY=vault://your.key".to_string()
            } else {
                "Use vault references for secrets: KEY=vault://your.key".to_string()
            },
            if language.is_italian() {
                "Se il test fallisce, verifica scope API, permessi e impostazioni di consenso."
                    .to_string()
            } else {
                "If test fails, verify API scopes/permissions and consent settings.".to_string()
            },
        ],
        documentation: docs.and_then(docs_context_to_view),
        error,
    }
}

async fn try_generate_install_guide_with_llm(
    config: &crate::config::Config,
    req: &McpInstallGuideRequest,
    docs: Option<&McpInstallGuideDocsContext>,
    language: GuideLanguage,
) -> anyhow::Result<McpInstallGuideResponse> {
    let model = config.agent.model.trim().to_string();
    if model.is_empty() {
        anyhow::bail!("no active model configured");
    }

    let (_, provider) = crate::provider::create_single_provider(config, &model)
        .map_err(|e| anyhow::anyhow!("provider unavailable: {e}"))?;

    let req_json = serde_json::to_string_pretty(req)?;
    let docs_block = docs
        .map(|d| {
            format!(
                "Documentation summary:\n{}\n\nDocumentation highlights:\n{}\n\nDocumentation excerpt:\n{}",
                d.summary,
                d.highlights.join("\n"),
                d.text_excerpt
            )
        })
        .unwrap_or_else(|| "Documentation summary:\nUnavailable".to_string());
    let system_prompt = format!(
        "You are an MCP installation assistant. Return ONLY valid JSON with keys: summary (string), steps (string[]), env_help (array of {{key,why,where_to_get,format_hint,vault_hint,retrieval_steps}}), notes (string[]). Keep concise and practical. Write every response string in {}.",
        language.llm_label()
    );
    let user_prompt = format!(
        "Prepare setup guidance for this MCP server config.\nInput JSON:\n{}\n\n{}\n\nRules:\n- steps max 6\n- env_help must include all env keys from input\n- where_to_get must include exact dashboard/console/doc path\n- each env_help item must include retrieval_steps with 2-4 concrete actions\n- use the provided documentation when available\n- explain where a dummy user can actually retrieve the missing credentials\n- no markdown",
        req_json,
        docs_block
    );

    let response = tokio::time::timeout(
        Duration::from_secs(20),
        provider.chat(crate::provider::ChatRequest {
            messages: vec![
                crate::provider::ChatMessage::system(&system_prompt),
                crate::provider::ChatMessage::user(&user_prompt),
            ],
            tools: vec![],
            model,
            max_tokens: 700,
            temperature: 0.2,
            think: None,
        }),
    )
    .await
    .map_err(|_| anyhow::anyhow!("llm timeout"))??;

    let content = response
        .content
        .ok_or_else(|| anyhow::anyhow!("empty llm response"))?;
    let json_block = extract_json_object_block(&content)
        .ok_or_else(|| anyhow::anyhow!("could not extract JSON from llm response"))?;
    let parsed: McpInstallGuideLlmParsed =
        serde_json::from_str(json_block).map_err(|e| anyhow::anyhow!("invalid llm JSON: {e}"))?;

    let mut by_key = parsed
        .env_help
        .iter()
        .map(|h| (h.key.to_ascii_lowercase(), h.clone()))
        .collect::<HashMap<_, _>>();
    let env_help = req
        .env
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|spec| {
            let llm = by_key.remove(&spec.key.to_ascii_lowercase());
            let fallback = fallback_env_help_for_spec(&spec, req, docs, language);
            match llm {
                Some(mut help) => {
                    if help.key.trim().is_empty() {
                        help.key = spec.key.clone();
                    }
                    if help.why.trim().is_empty() {
                        help.why = fallback.why.clone();
                    }
                    if help.where_to_get.trim().is_empty()
                        || is_generic_where_to_get(&help.where_to_get)
                    {
                        help.where_to_get = fallback.where_to_get.clone();
                    }
                    if help.format_hint.trim().is_empty() {
                        help.format_hint = fallback.format_hint.clone();
                    }
                    if help.vault_hint.trim().is_empty() {
                        help.vault_hint = fallback.vault_hint.clone();
                    }
                    if help.retrieval_steps.is_empty() {
                        help.retrieval_steps = fallback.retrieval_steps.clone();
                    }
                    help
                }
                None => fallback,
            }
        })
        .collect::<Vec<_>>();

    Ok(McpInstallGuideResponse {
        ok: true,
        source: if docs.is_some() {
            "llm+docs".to_string()
        } else {
            "llm".to_string()
        },
        summary: parsed.summary,
        steps: parsed.steps,
        env_help,
        notes: parsed.notes,
        documentation: docs.and_then(docs_context_to_view),
        error: None,
    })
}

async fn mcp_install_guide(
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpInstallGuideRequest>,
) -> Json<McpInstallGuideResponse> {
    let config = state.config.read().await.clone();
    let language = GuideLanguage::from_request(req.language.as_deref());
    let docs = fetch_install_docs_context(&req).await;
    match try_generate_install_guide_with_llm(&config, &req, docs.as_ref(), language).await {
        Ok(out) => Json(out),
        Err(e) => {
            let mut out =
                build_fallback_install_guide(&req, docs.as_ref(), Some(e.to_string()), language);
            if docs.is_some() {
                out.source = "docs".to_string();
            }
            Json(out)
        }
    }
}

#[derive(Deserialize)]
struct NpmSearchResponse {
    objects: Vec<NpmSearchObject>,
}

#[derive(Deserialize)]
struct NpmSearchObject {
    package: NpmPackage,
    score: NpmScore,
    downloads: Option<NpmDownloads>,
}

#[derive(Deserialize)]
struct NpmPackage {
    name: String,
    description: Option<String>,
    links: Option<NpmLinks>,
}

#[derive(Deserialize)]
struct NpmLinks {
    npm: Option<String>,
    repository: Option<String>,
    homepage: Option<String>,
}

#[derive(Deserialize)]
struct NpmScore {
    #[serde(rename = "final")]
    final_score: f64,
}

#[derive(Deserialize)]
struct NpmDownloads {
    monthly: Option<u64>,
}

fn looks_like_mcp_server_package(name: &str, description: &str) -> bool {
    let n = name.to_lowercase();
    let d = description.to_lowercase();
    n.contains("mcp")
        || d.contains("model context protocol")
        || d.contains("mcp server")
        || d.contains("model-context-protocol")
}

fn display_name_from_package(pkg: &str) -> String {
    let mut name = pkg.to_string();
    if let Some((_, rest)) = name.split_once('/') {
        name = rest.to_string();
    }
    name = name
        .replace("server-", "")
        .replace("-server", "")
        .replace("mcp-", "")
        .replace("-mcp", "")
        .replace('.', " ")
        .replace('_', " ")
        .replace('-', " ");
    name.split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

async fn search_mcp_catalog(Query(query): Query<McpSearchQuery>) -> Json<Vec<McpCatalogItemView>> {
    let q = query.q.trim();
    let limit = query.limit.unwrap_or(20).clamp(1, 50);
    let leaderboard = load_mcpmarket_index(100).await;

    if q.is_empty() {
        return list_mcp_catalog().await;
    }

    let mut out: Vec<McpCatalogItemView> = crate::skills::suggest_mcp_presets(q)
        .into_iter()
        .map(preset_to_view)
        .collect();
    for item in &mut out {
        apply_market_entry(item, &leaderboard);
    }

    let mut seen = out
        .iter()
        .map(|r| r.id.to_lowercase())
        .collect::<HashSet<_>>();

    let official_results =
        fetch_official_registry_servers(Some(q), (limit * 4).clamp(20, 100)).await;
    for entry in official_results {
        let Some(mut item) = official_registry_entry_to_view(entry) else {
            continue;
        };
        let dedupe_key = item.id.to_ascii_lowercase();
        if !seen.insert(dedupe_key) {
            continue;
        }
        apply_market_entry(&mut item, &leaderboard);
        out.push(item);
        if out.len() >= limit {
            break;
        }
    }

    let mut url = match reqwest::Url::parse("https://registry.npmjs.org/-/v1/search") {
        Ok(u) => u,
        Err(_) => return Json(out.into_iter().take(limit).collect()),
    };
    url.query_pairs_mut()
        .append_pair("text", &format!("{} mcp", q))
        .append_pair("size", &(limit * 4).to_string());

    let client = reqwest::Client::builder()
        .user_agent("homun")
        .timeout(Duration::from_secs(10))
        .build();

    if let Ok(client) = client {
        match client.get(url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(parsed) = resp.json::<NpmSearchResponse>().await {
                    for entry in parsed.objects {
                        let description = entry.package.description.unwrap_or_default();
                        if !looks_like_mcp_server_package(&entry.package.name, &description) {
                            continue;
                        }
                        let id = entry.package.name.clone();
                        if seen.contains(&id.to_lowercase()) {
                            continue;
                        }
                        let docs_url = entry.package.links.as_ref().and_then(|l| {
                            l.npm
                                .clone()
                                .or(l.repository.clone())
                                .or(l.homepage.clone())
                        });
                        let mut item = McpCatalogItemView {
                            kind: "npm".to_string(),
                            source: "npm".to_string(),
                            id: id.clone(),
                            display_name: display_name_from_package(&id),
                            description,
                            command: "npx".to_string(),
                            args: vec!["-y".to_string(), id.clone()],
                            transport: Some("stdio".to_string()),
                            url: None,
                            install_supported: true,
                            package_name: Some(id.clone()),
                            downloads_monthly: entry.downloads.and_then(|d| d.monthly),
                            score: Some(entry.score.final_score),
                            popularity_rank: None,
                            popularity_value: None,
                            popularity_source: None,
                            env: vec![],
                            docs_url,
                            aliases: vec![],
                            keywords: vec![],
                            recommended: false,
                            recommended_reason: None,
                            decision_tags: vec![],
                            setup_effort: "Moderate".to_string(),
                            auth_profile: "Unknown".to_string(),
                            preflight_checks: vec![],
                            why_choose: None,
                            tradeoff: None,
                        };
                        apply_market_entry(&mut item, &leaderboard);
                        out.push(item);
                        seen.insert(id.to_lowercase());
                        if out.len() >= limit {
                            break;
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, query = %q, "MCP npm search failed");
            }
        }
    }

    annotate_query_items(&mut out, q);
    sort_mcp_catalog_for_query(&mut out, q);
    apply_query_recommendation(&mut out, q);
    Json(out.into_iter().take(limit).collect())
}

#[derive(Serialize)]
struct McpServerEnvView {
    key: String,
    value_preview: String,
    is_vault_ref: bool,
}

#[derive(Serialize)]
struct McpServerView {
    name: String,
    transport: String,
    command: Option<String>,
    args: Vec<String>,
    url: Option<String>,
    capabilities: Vec<String>,
    enabled: bool,
    env: Vec<McpServerEnvView>,
}

fn normalize_mcp_capabilities(values: &[String]) -> Vec<String> {
    let mut out = values
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

fn mcp_env_preview(value: &str) -> (String, bool) {
    if value.starts_with("vault://") {
        (value.to_string(), true)
    } else if value.is_empty() {
        (String::new(), false)
    } else {
        ("(set)".to_string(), false)
    }
}

async fn list_mcp_servers(State(state): State<Arc<AppState>>) -> Json<Vec<McpServerView>> {
    let config = state.config.read().await;
    let mut servers = config
        .mcp
        .servers
        .iter()
        .map(|(name, server)| {
            let mut env = server
                .env
                .iter()
                .map(|(key, value)| {
                    let (value_preview, is_vault_ref) = mcp_env_preview(value);
                    McpServerEnvView {
                        key: key.clone(),
                        value_preview,
                        is_vault_ref,
                    }
                })
                .collect::<Vec<_>>();
            env.sort_by(|a, b| a.key.cmp(&b.key));

            McpServerView {
                name: name.clone(),
                transport: server.transport.clone(),
                command: server.command.clone(),
                args: server.args.clone(),
                url: server.url.clone(),
                capabilities: normalize_mcp_capabilities(&server.capabilities),
                enabled: server.enabled,
                env,
            }
        })
        .collect::<Vec<_>>();
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    Json(servers)
}

#[derive(Deserialize)]
struct McpSetupRequest {
    service: String,
    name: Option<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    overwrite: Option<bool>,
    skip_test: Option<bool>,
}

#[derive(Serialize)]
struct McpSetupResponse {
    ok: bool,
    message: String,
    name: String,
    missing_required_env: Vec<String>,
    stored_vault_keys: Vec<String>,
    tested: bool,
    connected: Option<bool>,
    tool_count: Option<usize>,
}

async fn setup_mcp_server(
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpSetupRequest>,
) -> Result<Json<McpSetupResponse>, StatusCode> {
    let Some(preset) = crate::skills::find_mcp_preset(&req.service) else {
        return Err(StatusCode::BAD_REQUEST);
    };

    let mut config = state.config.read().await.clone();
    let server_name = req.name.clone().unwrap_or_else(|| preset.id.clone());
    let overwrite = req.overwrite.unwrap_or(false);
    let skip_test = req.skip_test.unwrap_or(false);

    let setup = match crate::mcp_setup::apply_mcp_preset_setup(
        &mut config,
        &preset,
        &server_name,
        &req.env,
        overwrite,
    ) {
        Ok(result) => result,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("already exists") {
                return Err(StatusCode::CONFLICT);
            }
            tracing::warn!(error = %e, service = %req.service, "MCP setup failed");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    state
        .save_config(config.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !setup.missing_required_env.is_empty() {
        return Ok(Json(McpSetupResponse {
            ok: true,
            message: "Configured, but required env vars are still missing.".to_string(),
            name: server_name,
            missing_required_env: setup.missing_required_env,
            stored_vault_keys: setup.stored_vault_keys,
            tested: false,
            connected: None,
            tool_count: None,
        }));
    }

    if skip_test {
        return Ok(Json(McpSetupResponse {
            ok: true,
            message: "Configured. Connection test skipped.".to_string(),
            name: server_name,
            missing_required_env: vec![],
            stored_vault_keys: setup.stored_vault_keys,
            tested: false,
            connected: None,
            tool_count: None,
        }));
    }

    let Some(server) = config.mcp.servers.get(&server_name) else {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let report = crate::mcp_setup::test_mcp_server_connection(
        &server_name,
        server,
        Some(config.security.execution_sandbox.clone()),
    )
    .await;

    Ok(Json(McpSetupResponse {
        ok: true,
        message: if report.connected {
            format!(
                "Configured and connected ({} tools discovered).",
                report.tool_count
            )
        } else {
            match report.error.as_deref() {
                Some(err) => format!("Configured, but connection test failed: {err}"),
                None => "Configured, but connection test failed.".to_string(),
            }
        },
        name: server_name,
        missing_required_env: vec![],
        stored_vault_keys: setup.stored_vault_keys,
        tested: true,
        connected: Some(report.connected),
        tool_count: Some(report.tool_count),
    }))
}

#[derive(Deserialize)]
struct McpServerUpsertRequest {
    name: String,
    transport: Option<String>,
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    url: Option<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    capabilities: Vec<String>,
    enabled: Option<bool>,
    overwrite: Option<bool>,
}

async fn upsert_mcp_server(
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpServerUpsertRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let mut config = state.config.read().await.clone();
    let exists = config.mcp.servers.contains_key(&req.name);
    if exists && !req.overwrite.unwrap_or(false) {
        return Err(StatusCode::CONFLICT);
    }

    let transport = req.transport.unwrap_or_else(|| "stdio".to_string());
    if transport != "stdio" && transport != "http" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let command = req.command.map(|s| s.trim().to_string());
    let url = req.url.map(|s| s.trim().to_string());
    let args = req
        .args
        .iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect::<Vec<_>>();
    let env = req
        .env
        .iter()
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .filter(|(k, _)| !k.is_empty())
        .collect::<HashMap<_, _>>();
    let capabilities = normalize_mcp_capabilities(&req.capabilities);

    if transport == "stdio" && command.clone().unwrap_or_default().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if transport == "http" && url.clone().unwrap_or_default().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let server = crate::config::McpServerConfig {
        transport,
        command,
        args,
        url,
        env,
        capabilities,
        enabled: req.enabled.unwrap_or(true),
    };

    config.mcp.servers.insert(req.name.clone(), server);
    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(OkResponse {
        ok: true,
        message: Some(format!("MCP server '{}' saved", req.name)),
    }))
}

#[derive(Deserialize)]
struct McpToggleRequest {
    enabled: Option<bool>,
}

async fn toggle_mcp_server(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpToggleRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let mut config = state.config.read().await.clone();
    let Some(server) = config.mcp.servers.get_mut(&name) else {
        return Err(StatusCode::NOT_FOUND);
    };

    let new_enabled = req.enabled.unwrap_or(!server.enabled);
    server.enabled = new_enabled;

    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !new_enabled {
        if let Some(db) = &state.db {
            let reason = format!("Missing or disabled MCP dependency: {name}");
            if let Err(e) = db
                .invalidate_automations_by_dependency("mcp", &name, &reason)
                .await
            {
                tracing::warn!(
                    error = %e,
                    server = %name,
                    "Failed to invalidate dependent automations after MCP disable"
                );
            }
        }
    }

    Ok(Json(OkResponse {
        ok: true,
        message: Some(format!(
            "MCP server '{}' {}",
            name,
            if new_enabled { "enabled" } else { "disabled" }
        )),
    }))
}

#[derive(Serialize)]
struct McpTestResponse {
    ok: bool,
    connected: bool,
    message: String,
    tool_count: usize,
    server_name: String,
    server_version: String,
    error: Option<String>,
}

async fn test_mcp_server(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<McpTestResponse>, StatusCode> {
    let config = state.config.read().await;
    let Some(server) = config.mcp.servers.get(&name) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let server = server.clone();
    let sandbox = config.security.execution_sandbox.clone();
    drop(config);

    let report = crate::mcp_setup::test_mcp_server_connection(&name, &server, Some(sandbox)).await;

    Ok(Json(McpTestResponse {
        ok: true,
        connected: report.connected,
        message: if report.connected {
            format!("Connected: {} tool(s) discovered.", report.tool_count)
        } else {
            match report.error.as_deref() {
                Some(err) => format!("Connection failed: {err}"),
                None => {
                    "Connection failed. Check command, args, and environment variables.".to_string()
                }
            }
        },
        tool_count: report.tool_count,
        server_name: report.server_name,
        server_version: report.server_version,
        error: report.error,
    }))
}

async fn delete_mcp_server(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<OkResponse>, StatusCode> {
    let mut config = state.config.read().await.clone();
    if config.mcp.servers.remove(&name).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(db) = &state.db {
        let reason = format!("Missing or disabled MCP dependency: {name}");
        if let Err(e) = db
            .invalidate_automations_by_dependency("mcp", &name, &reason)
            .await
        {
            tracing::warn!(
                error = %e,
                server = %name,
                "Failed to invalidate dependent automations after MCP removal"
            );
        }
    }

    Ok(Json(OkResponse {
        ok: true,
        message: Some(format!("MCP server '{}' removed", name)),
    }))
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
            // Resolve trigger word: config → vault → auto-generate (on_demand only)
            let trigger_word = default_acc
                .and_then(|a| a.trigger_word.as_deref().filter(|s| !s.is_empty()))
                .map(|s| s.to_string())
                .or_else(|| {
                    // Check vault (always — user may have generated one previously)
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
                    .map_or(true, |t| t.is_empty())
            {
                if let Ok(secrets) = crate::storage::global_secrets() {
                    let key = crate::storage::SecretKey::custom("email.default.trigger_word");
                    let existing = secrets.get(&key).ok().flatten();
                    if existing.is_none() {
                        let tw = generate_email_trigger_word();
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
            // Keep phone_number and db_path — user might re-pair
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
                    message: format!("Cannot reach {}:{} — {}", imap_host, imap_port, e),
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
    uploads_deleted: u64,
    upload_dirs_deleted: u64,
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
        Ok(result) => {
            let upload_cleanup = cleanup_chat_upload_dirs(db, conv_days)
                .await
                .unwrap_or_default();
            Ok(Json(MemoryCleanupResponse {
                ok: true,
                messages_deleted: result.messages_deleted,
                chunks_deleted: result.chunks_deleted,
                uploads_deleted: upload_cleanup.files_deleted,
                upload_dirs_deleted: upload_cleanup.directories_deleted,
                message: format!(
                    "Cleaned up {} old messages, {} old history chunks, and {} uploaded chat files",
                    result.messages_deleted, result.chunks_deleted, upload_cleanup.files_deleted
                ),
            }))
        }
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
    /// Relevance score from hybrid search (0.0–1.0). None for FTS5-only results.
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<f64>,
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
            score: None,
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

    // Try hybrid search (vector + FTS5) if memory searcher is available
    #[cfg(feature = "local-embeddings")]
    if let Some(ref searcher_mutex) = state.memory_searcher {
        let mut searcher = searcher_mutex.lock().await;
        match searcher.search(&q.q, q.limit).await {
            Ok(results) => {
                let chunks: Vec<ChunkView> = results
                    .into_iter()
                    .map(|r| ChunkView {
                        id: r.chunk.id,
                        date: r.chunk.date,
                        source: r.chunk.source,
                        heading: r.chunk.heading,
                        content: r.chunk.content,
                        memory_type: r.chunk.memory_type,
                        created_at: r.chunk.created_at,
                        score: Some(r.score),
                    })
                    .collect();
                return Ok(Json(SearchResponse { chunks }));
            }
            Err(e) => {
                tracing::warn!(error = %e, "Hybrid search failed, falling back to FTS5-only");
                // Fall through to FTS5-only search below
            }
        }
    }

    // Fallback: FTS5-only search (no vector similarity)
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

#[cfg(feature = "vault-2fa")]
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

#[cfg(not(feature = "vault-2fa"))]
async fn reveal_vault_secret(
    Path(key): Path<String>,
    _req: Json<RevealRequest>,
) -> Result<Json<RevealResponse>, StatusCode> {
    // 2FA feature not enabled, allow direct access
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

// ─── Chat History ──────────────────────────────────────────────

#[derive(Deserialize)]
struct ChatHistoryQuery {
    limit: Option<u32>,
    conversation_id: Option<String>,
}

#[derive(Serialize)]
struct ChatHistoryMessage {
    role: String,
    content: String,
    tools_used: Vec<String>,
    timestamp: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    attachments: Vec<super::chat_attachments::ChatAttachment>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    mcp_servers: Vec<super::chat_attachments::ChatMcpServerRef>,
}

#[derive(Serialize)]
struct ChatConversationSummary {
    conversation_id: String,
    title: String,
    preview: String,
    created_at: String,
    updated_at: String,
    message_count: u32,
    archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_run: Option<super::run_state::WebChatRunSnapshot>,
}

#[derive(Deserialize)]
struct ChatConversationListQuery {
    limit: Option<u32>,
    q: Option<String>,
    include_archived: Option<bool>,
}

#[derive(Deserialize, Default)]
struct ChatConversationQuery {
    conversation_id: Option<String>,
}

#[derive(Deserialize, Serialize, Default)]
struct ChatConversationMetadata {
    title: Option<String>,
    archived: Option<bool>,
}

#[derive(Deserialize)]
struct UpdateChatConversationRequest {
    title: Option<String>,
    archived: Option<bool>,
}

#[derive(Serialize)]
struct ChatUploadResponse {
    ok: bool,
    attachment: super::chat_attachments::ChatAttachment,
}

struct ValidatedChatUpload {
    kind: String,
    content_type: String,
    max_bytes: usize,
}

fn web_session_key(conversation_id: &str) -> String {
    format!("web:{conversation_id}")
}

fn chat_uploads_root() -> PathBuf {
    crate::config::Config::workspace_dir().join(".chat-uploads")
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ChatUploadCleanupStats {
    pub files_deleted: u64,
    pub directories_deleted: u64,
    pub bytes_deleted: u64,
}

async fn collect_upload_dir_stats(
    path: &std::path::Path,
) -> std::io::Result<ChatUploadCleanupStats> {
    let mut stats = ChatUploadCleanupStats::default();
    let mut stack = vec![path.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };

        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                stats.directories_deleted += 1;
                stack.push(entry.path());
            } else {
                stats.files_deleted += 1;
                if let Ok(metadata) = entry.metadata().await {
                    stats.bytes_deleted += metadata.len();
                }
            }
        }
    }

    Ok(stats)
}

async fn remove_chat_upload_dir(conversation_id: &str) -> std::io::Result<ChatUploadCleanupStats> {
    let conversation = sanitize_chat_segment(conversation_id, "default");
    let path = chat_uploads_root().join(conversation);
    let mut stats = collect_upload_dir_stats(&path).await?;
    match tokio::fs::remove_dir_all(&path).await {
        Ok(()) => {
            stats.directories_deleted += 1;
            Ok(stats)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ChatUploadCleanupStats::default()),
        Err(e) => Err(e),
    }
}

pub(crate) async fn cleanup_chat_upload_dirs(
    db: &crate::storage::Database,
    retention_days: u32,
) -> anyhow::Result<ChatUploadCleanupStats> {
    let root = chat_uploads_root();
    if !root.exists() {
        return Ok(ChatUploadCleanupStats::default());
    }

    let rows = db.list_sessions_by_prefix("web:%", 5000).await?;
    let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
    let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();
    let keep = rows
        .into_iter()
        .filter_map(|row| {
            row.key
                .strip_prefix("web:")
                .map(|conversation_id| (conversation_id.to_string(), row.updated_at))
        })
        .filter(|(_, updated_at)| updated_at >= &cutoff_str)
        .map(|(conversation_id, _)| sanitize_chat_segment(&conversation_id, "default"))
        .collect::<HashSet<_>>();

    let mut stats = ChatUploadCleanupStats::default();
    let mut entries = tokio::fs::read_dir(&root).await?;
    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        if !file_type.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();
        if keep.contains(&dir_name) {
            continue;
        }
        let dir_stats = remove_chat_upload_dir(&dir_name).await?;
        stats.files_deleted += dir_stats.files_deleted;
        stats.directories_deleted += dir_stats.directories_deleted;
        stats.bytes_deleted += dir_stats.bytes_deleted;
    }

    Ok(stats)
}

fn sanitize_chat_segment(value: &str, fallback: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized
    }
}

fn chat_upload_path(conversation_id: &str, file_name: &str) -> Option<PathBuf> {
    let conversation = sanitize_chat_segment(conversation_id, "default");
    let file_name = sanitize_chat_segment(file_name, "upload.bin");
    let path = chat_uploads_root().join(conversation).join(file_name);
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return None;
    }
    Some(path)
}

fn validate_chat_upload_kind(
    kind: &str,
    file_name: &str,
    content_type: Option<&str>,
) -> Option<ValidatedChatUpload> {
    let normalized_kind = kind.trim().to_lowercase();
    let extension = std::path::Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_lowercase())
        .unwrap_or_default();
    let guessed = content_type
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| {
            mime_guess::from_path(file_name)
                .first_or_octet_stream()
                .essence_str()
                .to_string()
        });

    match normalized_kind.as_str() {
        "image" if guessed.starts_with("image/") => Some(ValidatedChatUpload {
            kind: "image".to_string(),
            content_type: guessed,
            max_bytes: 15 * 1024 * 1024,
        }),
        "document" => {
            let allowed = matches!(extension.as_str(), "pdf" | "md" | "txt" | "doc" | "docx")
                || matches!(
                    guessed.as_str(),
                    "application/pdf"
                        | "text/markdown"
                        | "text/plain"
                        | "application/msword"
                        | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                );
            if allowed {
                Some(ValidatedChatUpload {
                    kind: "document".to_string(),
                    content_type: guessed,
                    max_bytes: 25 * 1024 * 1024,
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

fn chat_conversation_id(query: &ChatConversationQuery) -> String {
    query
        .conversation_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("default")
        .to_string()
}

fn chat_conversation_title(metadata: &str, first_user_message: Option<&str>) -> String {
    let metadata_title = parse_chat_conversation_metadata(metadata)
        .title
        .filter(|value| !value.trim().is_empty());

    metadata_title
        .or_else(|| {
            first_user_message
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(truncate_conversation_label)
        })
        .unwrap_or_else(|| "New conversation".to_string())
}

fn truncate_conversation_label(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let truncated: String = chars.by_ref().take(48).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else if truncated.is_empty() {
        "New conversation".to_string()
    } else {
        truncated
    }
}

fn parse_chat_conversation_metadata(metadata: &str) -> ChatConversationMetadata {
    serde_json::from_str(metadata).unwrap_or_default()
}

fn chat_message_label(raw: &str) -> String {
    let parsed = super::chat_attachments::parse_message_content(raw);
    let text = parsed.text.trim().to_string();
    if !text.is_empty() {
        return text;
    }
    if let Some(attachment) = parsed.attachments.first() {
        return attachment.name.clone();
    }
    parsed
        .mcp_servers
        .first()
        .map(|server| server.name.clone())
        .unwrap_or_default()
}

fn build_chat_conversation_summary(
    state: &Arc<AppState>,
    row: crate::storage::SessionListRow,
) -> Option<ChatConversationSummary> {
    let conversation_id = row.key.strip_prefix("web:")?.to_string();
    let session_key = web_session_key(&conversation_id);
    let metadata = parse_chat_conversation_metadata(&row.metadata);
    let first_user_message = row.first_user_message.as_deref().map(chat_message_label);
    let title = metadata
        .title
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| chat_conversation_title(&row.metadata, first_user_message.as_deref()));
    let preview = row
        .last_message_preview
        .as_deref()
        .map(chat_message_label)
        .map(|value| truncate_conversation_label(&value))
        .unwrap_or_default();
    let updated_at = row.last_message_at.unwrap_or(row.updated_at);
    Some(ChatConversationSummary {
        conversation_id,
        title,
        preview,
        created_at: row.created_at,
        updated_at,
        message_count: row.message_count.max(0) as u32,
        archived: metadata.archived.unwrap_or(false),
        active_run: state.web_runs.active_snapshot(&session_key),
    })
}

async fn list_chat_conversations(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ChatConversationListQuery>,
) -> Result<Json<Vec<ChatConversationSummary>>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let rows = db
        .list_sessions_by_prefix("web:%", q.limit.unwrap_or(50))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let search = q.q.as_deref().map(|value| value.trim().to_lowercase());
    let include_archived = q.include_archived.unwrap_or(false);

    let conversations = rows
        .into_iter()
        .filter_map(|row| build_chat_conversation_summary(&state, row))
        .filter(|conversation| {
            if !include_archived && conversation.archived {
                return false;
            }
            if let Some(search) = search.as_deref() {
                let haystack = format!(
                    "{} {}",
                    conversation.title.to_lowercase(),
                    conversation.preview.to_lowercase()
                );
                haystack.contains(search)
            } else {
                true
            }
        })
        .collect();

    Ok(Json(conversations))
}

async fn create_chat_conversation(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ChatConversationSummary>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let conversation_id = uuid::Uuid::new_v4().to_string();
    let session_key = web_session_key(&conversation_id);
    let metadata = serde_json::json!({});

    db.upsert_session(&session_key, 0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    db.set_session_metadata(&session_key, &metadata.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = db
        .load_session(&session_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ChatConversationSummary {
        conversation_id,
        title: String::new(),
        preview: String::new(),
        created_at: session.created_at,
        updated_at: session.updated_at,
        message_count: 0,
        archived: false,
        active_run: None,
    }))
}

async fn update_chat_conversation(
    State(state): State<Arc<AppState>>,
    Path(conversation_id): Path<String>,
    Json(req): Json<UpdateChatConversationRequest>,
) -> Result<Json<ChatConversationSummary>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let session_key = web_session_key(&conversation_id);
    let existing = db
        .load_session(&session_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut metadata = parse_chat_conversation_metadata(&existing.metadata);
    if let Some(title) = req.title {
        let title = title.trim();
        metadata.title = if title.is_empty() {
            None
        } else {
            Some(truncate_conversation_label(title))
        };
    }
    if let Some(archived) = req.archived {
        metadata.archived = Some(archived);
    }

    let metadata_json =
        serde_json::to_string(&metadata).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    db.set_session_metadata(&session_key, &metadata_json)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = db
        .list_sessions_by_prefix(&session_key, 1)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row = rows.into_iter().next().ok_or(StatusCode::NOT_FOUND)?;
    let summary = build_chat_conversation_summary(&state, row).ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(summary))
}

async fn delete_chat_conversation(
    State(state): State<Arc<AppState>>,
    Path(conversation_id): Path<String>,
) -> Result<Json<OkResponse>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let session_key = web_session_key(&conversation_id);

    state.web_runs.clear_session(&session_key);
    let _ = db.delete_web_chat_runs(&session_key).await;
    let deleted = db
        .delete_session(&session_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deleted {
        if let Err(e) = remove_chat_upload_dir(&conversation_id).await {
            tracing::warn!(conversation_id = %conversation_id, error = %e, "Failed to remove chat upload directory");
        }
    }

    Ok(Json(OkResponse {
        ok: deleted,
        message: Some(if deleted {
            "Conversation deleted".to_string()
        } else {
            "Conversation not found".to_string()
        }),
    }))
}

async fn chat_history(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ChatHistoryQuery>,
) -> Result<Json<Vec<ChatHistoryMessage>>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let limit = q.limit.unwrap_or(50);
    let conversation_id = q
        .conversation_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("default");
    let session_key = web_session_key(conversation_id);

    let rows = db
        .load_messages(&session_key, limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let messages: Vec<ChatHistoryMessage> = rows
        .into_iter()
        .filter(|r| r.role == "user" || r.role == "assistant")
        .map(|r| {
            let tools: Vec<String> = serde_json::from_str(&r.tools_used).unwrap_or_default();
            let parsed = super::chat_attachments::parse_message_content(&r.content);
            ChatHistoryMessage {
                role: r.role,
                content: parsed.text,
                tools_used: tools,
                timestamp: r.timestamp,
                attachments: parsed.attachments,
                mcp_servers: parsed.mcp_servers,
            }
        })
        .collect();

    Ok(Json(messages))
}

async fn upload_chat_attachment(
    mut multipart: Multipart,
) -> Result<Json<ChatUploadResponse>, StatusCode> {
    let mut conversation_id = "default".to_string();
    let mut kind = "image".to_string();
    let mut file_name = None;
    let mut content_type = None;
    let mut bytes = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        match field.name() {
            Some("conversation_id") => {
                conversation_id = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            }
            Some("kind") => {
                kind = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            }
            Some("file") => {
                file_name = Some(field.file_name().unwrap_or("upload.bin").to_string());
                content_type = field.content_type().map(ToString::to_string);
                bytes = Some(field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?);
            }
            _ => {}
        }
    }

    let file_name = sanitize_chat_segment(file_name.as_deref().unwrap_or("upload.bin"), "image");
    let bytes = bytes.ok_or(StatusCode::BAD_REQUEST)?;
    let validated = validate_chat_upload_kind(&kind, &file_name, content_type.as_deref())
        .ok_or(StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
    if bytes.is_empty() || bytes.len() > validated.max_bytes {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    let conversation_id = sanitize_chat_segment(&conversation_id, "default");
    let extension = std::path::Path::new(&file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| sanitize_chat_segment(value, "bin"))
        .filter(|value| !value.is_empty());
    let stored_name = match extension {
        Some(ext) => format!("{}.{}", uuid::Uuid::new_v4(), ext),
        None => uuid::Uuid::new_v4().to_string(),
    };
    let path = chat_upload_path(&conversation_id, &stored_name).ok_or(StatusCode::BAD_REQUEST)?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ChatUploadResponse {
        ok: true,
        attachment: super::chat_attachments::ChatAttachment {
            kind: validated.kind,
            name: file_name,
            stored_path: path.to_string_lossy().to_string(),
            preview_url: format!("/api/v1/chat/uploads/{conversation_id}/{stored_name}"),
            content_type: validated.content_type,
            size_bytes: bytes.len() as u64,
        },
    }))
}

async fn get_chat_uploaded_file(
    Path((conversation_id, file_name)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let path = chat_upload_path(&conversation_id, &file_name).ok_or(StatusCode::BAD_REQUEST)?;
    let data = tokio::fs::read(&path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let content_type = mime_guess::from_path(&path).first_or_octet_stream();

    let mut response = Response::new(Body::from(data));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(content_type.as_ref())
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );
    Ok(response)
}

#[cfg(test)]
mod chat_upload_tests {
    use super::validate_chat_upload_kind;

    #[test]
    fn accepts_supported_image_uploads() {
        let upload = validate_chat_upload_kind("image", "photo.png", Some("image/png"))
            .expect("expected image upload to validate");
        assert_eq!(upload.kind, "image");
        assert_eq!(upload.content_type, "image/png");
    }

    #[test]
    fn accepts_supported_document_uploads() {
        let upload = validate_chat_upload_kind("document", "report.pdf", Some("application/pdf"))
            .expect("expected document upload to validate");
        assert_eq!(upload.kind, "document");
        assert_eq!(upload.content_type, "application/pdf");
    }

    #[test]
    fn rejects_unsupported_document_uploads() {
        assert!(
            validate_chat_upload_kind("document", "archive.zip", Some("application/zip")).is_none()
        );
    }
}

/// Clear chat history for the web session
async fn clear_chat_history(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ChatConversationQuery>,
) -> Result<Json<OkResponse>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let conversation_id = chat_conversation_id(&q);
    let session_key = web_session_key(&conversation_id);

    db.clear_messages(&session_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = db.delete_web_chat_runs(&session_key).await;

    state.web_runs.clear_session(&session_key);

    Ok(Json(OkResponse {
        ok: true,
        message: Some("Chat history cleared".to_string()),
    }))
}

async fn current_chat_run(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ChatConversationQuery>,
) -> Result<Json<Option<super::run_state::WebChatRunSnapshot>>, StatusCode> {
    let conversation_id = chat_conversation_id(&q);
    let session_key = web_session_key(&conversation_id);

    if let Some(run) = state.web_runs.active_snapshot(&session_key) {
        return Ok(Json(Some(run)));
    }

    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let run = db
        .load_restorable_web_chat_run(&session_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(run))
}

/// Compact chat conversation (trigger memory consolidation)
async fn compact_chat(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ChatConversationQuery>,
) -> Result<Json<OkResponse>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let conversation_id = chat_conversation_id(&q);
    let session_key = web_session_key(&conversation_id);

    // Check if there are enough messages to consolidate
    let count = db
        .count_messages(&session_key)
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
    db.upsert_session(&session_key, 0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(OkResponse {
        ok: true,
        message: Some("Conversation will be compacted on next message".to_string()),
    }))
}

/// Request cancellation of the current web chat run.
async fn stop_chat_run(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ChatConversationQuery>,
) -> Result<Json<OkResponse>, StatusCode> {
    let conversation_id = chat_conversation_id(&q);
    let session_key = web_session_key(&conversation_id);
    let active = state.web_runs.request_stop(&session_key);
    if let Some(run) = active.as_ref() {
        if let Some(db) = state.db.as_ref() {
            if let Err(error) = db.upsert_web_chat_run(run).await {
                tracing::error!(run_id = %run.run_id, %error, "Failed to persist stopping web chat run");
            }
        }
    }
    crate::agent::stop::request_stop();
    Ok(Json(OkResponse {
        ok: true,
        message: Some(if active.is_some() {
            "Stop requested".to_string()
        } else {
            "No active chat run".to_string()
        }),
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

/// Get process execution sandbox configuration.
async fn get_execution_sandbox(
    State(state): State<Arc<AppState>>,
) -> Json<crate::config::ExecutionSandboxConfig> {
    let config = state.config.read().await;
    Json(config.security.execution_sandbox.clone())
}

/// Update process execution sandbox configuration.
async fn put_execution_sandbox(
    State(state): State<Arc<AppState>>,
    Json(sandbox): Json<crate::config::ExecutionSandboxConfig>,
) -> Result<Json<crate::config::ExecutionSandboxConfig>, (StatusCode, String)> {
    let sandbox = normalize_execution_sandbox(sandbox)?;
    let mut config = state.config.write().await;
    config.security.execution_sandbox = sandbox;

    if let Err(e) = config.save() {
        tracing::error!("Failed to save execution sandbox config: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to save execution sandbox config".to_string(),
        ));
    }

    Ok(Json(config.security.execution_sandbox.clone()))
}

fn normalize_execution_sandbox(
    mut sandbox: crate::config::ExecutionSandboxConfig,
) -> Result<crate::config::ExecutionSandboxConfig, (StatusCode, String)> {
    let backend = sandbox.backend.trim().to_ascii_lowercase();
    let docker_network = sandbox.docker_network.trim().to_ascii_lowercase();
    let docker_image = sandbox.docker_image.trim().to_string();

    let backend = if backend.is_empty() {
        "auto".to_string()
    } else {
        backend
    };
    if backend != "none" && backend != "auto" && backend != "docker" {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid sandbox backend. Expected one of: auto, docker, none.".to_string(),
        ));
    }

    let docker_network = if docker_network.is_empty() {
        "none".to_string()
    } else {
        docker_network
    };
    if docker_network != "none" && docker_network != "bridge" && docker_network != "host" {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid docker network. Expected one of: none, bridge, host.".to_string(),
        ));
    }

    if !sandbox.docker_cpus.is_finite() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid docker CPU limit. Must be a finite number.".to_string(),
        ));
    }
    if sandbox.docker_cpus < 0.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid docker CPU limit. Must be >= 0.".to_string(),
        ));
    }
    if sandbox.docker_cpus > 256.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid docker CPU limit. Must be <= 256.".to_string(),
        ));
    }

    if sandbox.docker_memory_mb > 1_048_576 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid docker memory limit. Must be <= 1048576 MB.".to_string(),
        ));
    }

    sandbox.backend = backend;
    sandbox.docker_network = docker_network;
    sandbox.docker_image = if docker_image.is_empty() {
        "node:22-alpine".to_string()
    } else {
        docker_image
    };

    Ok(sandbox)
}

#[cfg(test)]
mod sandbox_config_tests {
    use super::get_execution_sandbox_presets;
    use super::normalize_execution_sandbox;
    use crate::config::ExecutionSandboxConfig;
    use axum::http::StatusCode;

    #[test]
    fn normalize_sandbox_accepts_valid_values() {
        let cfg = ExecutionSandboxConfig {
            backend: "DoCkEr".to_string(),
            docker_network: "Bridge".to_string(),
            docker_image: " node:22-alpine ".to_string(),
            ..ExecutionSandboxConfig::default()
        };
        let normalized = normalize_execution_sandbox(cfg).expect("valid sandbox config");
        assert_eq!(normalized.backend, "docker");
        assert_eq!(normalized.docker_network, "bridge");
        assert_eq!(normalized.docker_image, "node:22-alpine");
    }

    #[test]
    fn normalize_sandbox_rejects_invalid_backend() {
        let cfg = ExecutionSandboxConfig {
            backend: "firecracker".to_string(),
            ..ExecutionSandboxConfig::default()
        };
        let err = normalize_execution_sandbox(cfg).expect_err("expected backend validation error");
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn normalize_sandbox_rejects_invalid_network() {
        let cfg = ExecutionSandboxConfig {
            docker_network: "custom".to_string(),
            ..ExecutionSandboxConfig::default()
        };
        let err = normalize_execution_sandbox(cfg).expect_err("expected network validation error");
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn normalize_sandbox_defaults_empty_image() {
        let cfg = ExecutionSandboxConfig {
            docker_image: "   ".to_string(),
            ..ExecutionSandboxConfig::default()
        };
        let normalized = normalize_execution_sandbox(cfg).expect("expected default image");
        assert_eq!(normalized.docker_image, "node:22-alpine");
    }

    #[tokio::test]
    async fn sandbox_presets_include_safe_and_strict() {
        let presets = get_execution_sandbox_presets().await.0;
        assert!(presets.iter().any(|p| p.id == "safe"));
        assert!(presets.iter().any(|p| p.id == "strict"));

        let recommended_count = presets.iter().filter(|p| p.recommended).count();
        assert_eq!(recommended_count, 1);
    }
}

#[derive(Serialize)]
struct ExecutionSandboxStatusResponse {
    enabled: bool,
    host_os: String,
    configured_backend: String,
    resolved_backend: String,
    strict: bool,
    docker_available: bool,
    valid: bool,
    fallback_to_native: bool,
    recommended_preset: String,
    message: String,
}

#[derive(Serialize)]
struct ExecutionSandboxPresetResponse {
    id: String,
    label: String,
    description: String,
    recommended: bool,
    config: crate::config::ExecutionSandboxConfig,
}

#[derive(Deserialize)]
struct ExecutionSandboxEventsQuery {
    limit: Option<usize>,
}

#[derive(Serialize)]
struct ExecutionSandboxImagePullResponse {
    status: crate::tools::sandbox_exec::SandboxImageStatus,
    output: String,
}

fn host_os_label() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        "unknown"
    }
}

/// Get resolved runtime status for process execution sandbox.
async fn get_execution_sandbox_status(
    State(state): State<Arc<AppState>>,
) -> Json<ExecutionSandboxStatusResponse> {
    let config = state.config.read().await;
    let sandbox = config.security.execution_sandbox.clone();
    drop(config);

    let configured_backend = sandbox.backend.trim().to_ascii_lowercase();
    let docker_available = crate::tools::sandbox_exec::docker_backend_available();
    let (resolved_backend, valid, mut message) =
        match crate::tools::sandbox_exec::resolve_sandbox_backend(&sandbox) {
            Ok(resolved) => (resolved.as_str().to_string(), true, String::new()),
            Err(e) => ("none".to_string(), false, e.to_string()),
        };
    let fallback_to_native = sandbox.enabled
        && resolved_backend == "none"
        && configured_backend != "none"
        && !sandbox.strict;
    let recommended_preset = if docker_available { "strict" } else { "safe" };
    if valid {
        message = if !sandbox.enabled {
            "Sandbox disabled.".to_string()
        } else if fallback_to_native {
            format!(
                "Configured backend '{}' unavailable; using native fallback.",
                configured_backend
            )
        } else if resolved_backend == "none" {
            "Sandbox enabled with 'none' backend (native execution).".to_string()
        } else {
            format!("Sandbox active with '{}' backend.", resolved_backend)
        };
    }

    Json(ExecutionSandboxStatusResponse {
        enabled: sandbox.enabled,
        host_os: host_os_label().to_string(),
        configured_backend: configured_backend.clone(),
        resolved_backend,
        strict: sandbox.strict,
        docker_available,
        valid,
        fallback_to_native,
        recommended_preset: recommended_preset.to_string(),
        message,
    })
}

/// List opinionated execution sandbox presets for the current host.
async fn get_execution_sandbox_presets() -> Json<Vec<ExecutionSandboxPresetResponse>> {
    let mut safe_cfg = crate::config::ExecutionSandboxConfig::default();
    safe_cfg.enabled = true;
    safe_cfg.backend = "auto".to_string();
    safe_cfg.strict = false;
    safe_cfg.docker_network = "none".to_string();
    safe_cfg.docker_read_only_rootfs = true;
    safe_cfg.docker_mount_workspace = true;

    let mut strict_cfg = safe_cfg.clone();
    strict_cfg.strict = true;

    let docker_available = crate::tools::sandbox_exec::docker_backend_available();
    let host = host_os_label();

    Json(vec![
        ExecutionSandboxPresetResponse {
            id: "safe".to_string(),
            label: format!("{host} Safe"),
            description: "Prefers sandbox backend but allows native fallback when unavailable."
                .to_string(),
            recommended: !docker_available,
            config: safe_cfg,
        },
        ExecutionSandboxPresetResponse {
            id: "strict".to_string(),
            label: format!("{host} Strict"),
            description: "Requires sandbox backend; blocks execution if backend is unavailable."
                .to_string(),
            recommended: docker_available,
            config: strict_cfg,
        },
    ])
}

/// Inspect the configured sandbox runtime image.
async fn get_execution_sandbox_image_status(
    State(state): State<Arc<AppState>>,
) -> Json<crate::tools::sandbox_exec::SandboxImageStatus> {
    let config = state.config.read().await;
    let image = config.security.execution_sandbox.docker_image.clone();
    drop(config);

    Json(crate::tools::sandbox_exec::get_docker_image_status(&image))
}

/// Pull the configured sandbox runtime image.
async fn pull_execution_sandbox_image(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ExecutionSandboxImagePullResponse>, (StatusCode, String)> {
    let config = state.config.read().await;
    let image = config.security.execution_sandbox.docker_image.clone();
    drop(config);

    let result = crate::tools::sandbox_exec::pull_docker_image(&image)
        .await
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    Ok(Json(ExecutionSandboxImagePullResponse {
        status: result.status,
        output: result.output,
    }))
}

/// Return the most recent sandbox preparation events.
async fn get_execution_sandbox_events(
    Query(query): Query<ExecutionSandboxEventsQuery>,
) -> Json<Vec<crate::tools::sandbox_exec::SandboxEvent>> {
    let limit = query.limit.unwrap_or(12).clamp(1, 50);
    Json(crate::tools::sandbox_exec::list_recent_sandbox_events(
        limit,
    ))
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
        metadata: None,
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
    let status = config.browser.runtime_status();
    if !status.available {
        return Json(BrowserTestResponse {
            success: false,
            message: status.reason.unwrap_or_else(|| {
                "Browser automation is unavailable in the current configuration".to_string()
            }),
        });
    }

    // With MCP-based browser, the Playwright server starts on demand when the agent
    // first calls a browser tool. We just confirm prerequisites are met.
    let exe_info = status
        .executable_path
        .map(|p| format!(" (Chrome: {})", p))
        .unwrap_or_default();
    Json(BrowserTestResponse {
        success: true,
        message: format!(
            "Browser prerequisites OK. MCP server (@playwright/mcp) will start on first use{}.",
            exe_info
        ),
    })
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

// --- Automations ---

#[derive(Deserialize)]
struct CreateAutomationRequest {
    name: String,
    prompt: String,
    schedule: Option<String>,
    cron: Option<String>,
    every: Option<u64>,
    trigger: Option<String>,
    trigger_value: Option<String>,
    enabled: Option<bool>,
    deliver_to: Option<String>,
}

#[derive(Deserialize)]
struct PatchAutomationRequest {
    name: Option<String>,
    prompt: Option<String>,
    schedule: Option<String>,
    cron: Option<String>,
    every: Option<u64>,
    trigger: Option<String>,
    trigger_value: Option<String>,
    clear_trigger_value: Option<bool>,
    enabled: Option<bool>,
    status: Option<String>,
    deliver_to: Option<String>,
    clear_deliver_to: Option<bool>,
}

#[derive(Deserialize)]
struct AutomationHistoryQuery {
    limit: Option<u32>,
}

#[derive(Serialize)]
struct RunAutomationResponse {
    run_id: String,
    status: String,
    message: String,
}

#[derive(Serialize)]
struct AutomationListItem {
    #[serde(flatten)]
    row: crate::storage::AutomationRow,
    next_run: Option<String>,
}

#[derive(Serialize)]
struct AutomationTarget {
    value: String,
    label: String,
}

fn automation_channel_label(channel: &str) -> String {
    match channel {
        "telegram" => "Telegram".to_string(),
        "discord" => "Discord".to_string(),
        "slack" => "Slack".to_string(),
        "whatsapp" => "WhatsApp".to_string(),
        "web" => "Web".to_string(),
        ch if ch.starts_with("email:") => format!("Email ({})", &ch[6..]),
        "email" => "Email".to_string(),
        other => other.to_string(),
    }
}

fn resolve_automation_target_chat_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if !trimmed.starts_with("vault://") {
        return trimmed.to_string();
    }

    let Some(key) = trimmed.strip_prefix("vault://") else {
        return trimmed.to_string();
    };
    let vault_key = key.trim();
    if vault_key.is_empty() {
        return trimmed.to_string();
    }

    if let Ok(secrets) = crate::storage::global_secrets() {
        let secret_key = crate::storage::SecretKey::custom(&format!("vault.{vault_key}"));
        if let Ok(Some(value)) = secrets.get(&secret_key) {
            let resolved = value.trim();
            if !resolved.is_empty() {
                return resolved.to_string();
            }
        }
    }
    trimmed.to_string()
}

fn build_automation_schedule(
    schedule: Option<&str>,
    cron: Option<&str>,
    every: Option<u64>,
) -> Result<String, (StatusCode, String)> {
    if let Some(s) = schedule {
        return crate::scheduler::AutomationSchedule::parse_stored(s)
            .map(|v| v.as_stored())
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()));
    }

    match (cron, every) {
        (Some(expr), None) => crate::scheduler::AutomationSchedule::from_cron(expr)
            .map(|v| v.as_stored())
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string())),
        (None, Some(secs)) => crate::scheduler::AutomationSchedule::from_every(secs)
            .map(|v| v.as_stored())
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string())),
        _ => Err((
            StatusCode::BAD_REQUEST,
            "Provide either `schedule` or one of (`cron`, `every`)".to_string(),
        )),
    }
}

fn parse_deliver_to(deliver_to: &str) -> Result<(String, String), (StatusCode, String)> {
    let (channel, chat_id) = deliver_to.rsplit_once(':').ok_or((
        StatusCode::BAD_REQUEST,
        "deliver_to must be in format channel:chat_id".to_string(),
    ))?;
    let channel = channel.trim();
    let chat_id = chat_id.trim();
    if channel.is_empty() || chat_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "deliver_to must be in format channel:chat_id".to_string(),
        ));
    }
    Ok((channel.to_string(), chat_id.to_string()))
}

fn normalize_automation_trigger(
    trigger: Option<&str>,
    trigger_value: Option<&str>,
) -> Result<(String, Option<String>), (StatusCode, String)> {
    let trigger = trigger
        .unwrap_or("always")
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_");

    match trigger.as_str() {
        "always" => Ok(("always".to_string(), None)),
        "on_change" | "changed" => Ok(("on_change".to_string(), None)),
        "contains" => {
            let value = trigger_value
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .ok_or((
                    StatusCode::BAD_REQUEST,
                    "trigger_value is required when trigger=contains".to_string(),
                ))?;
            Ok(("contains".to_string(), Some(value.to_string())))
        }
        _ => Err((
            StatusCode::BAD_REQUEST,
            "trigger must be one of: always, on_change, contains".to_string(),
        )),
    }
}

/// GET /api/v1/automations/targets
async fn list_automation_targets(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<AutomationTarget>> {
    let mut channels = {
        let cfg = state.config.read().await;
        cfg.channels.clone()
    };
    channels.migrate_legacy_email();

    let mut seen = HashSet::new();
    let mut targets = vec![AutomationTarget {
        value: "cli:default".to_string(),
        label: "CLI (default)".to_string(),
    }];
    seen.insert("cli:default".to_string());

    for (channel, chat_id) in channels.active_channels_with_chat_ids() {
        let chat_id = resolve_automation_target_chat_id(&chat_id);
        if chat_id.is_empty() || chat_id.starts_with("vault://") {
            continue;
        }
        let value = format!("{channel}:{chat_id}");
        if !seen.insert(value.clone()) {
            continue;
        }
        let label = format!("{} ({chat_id})", automation_channel_label(&channel));
        targets.push(AutomationTarget { value, label });
    }

    if let Some(db) = &state.db {
        if let Ok(users) = db.load_all_users().await {
            if let Some(owner) = users.into_iter().next() {
                if let Ok(identities) = db.load_user_identities(&owner.id).await {
                    for identity in identities {
                        let channel = identity.channel.trim().to_ascii_lowercase();
                        let platform_id = identity.platform_id.trim().to_string();
                        if channel.is_empty() || platform_id.is_empty() {
                            continue;
                        }

                        let value = format!("{channel}:{platform_id}");
                        if !seen.insert(value.clone()) {
                            continue;
                        }

                        let label_suffix = identity
                            .display_name
                            .as_deref()
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .unwrap_or(&platform_id);
                        let label =
                            format!("{} ({label_suffix})", automation_channel_label(&channel));
                        targets.push(AutomationTarget { value, label });
                    }
                }
            }
        }
    }

    if let Some(cli_idx) = targets.iter().position(|t| t.value == "cli:default") {
        let cli = targets.remove(cli_idx);
        targets.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
        targets.insert(0, cli);
    } else {
        targets.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    }

    Json(targets)
}

/// GET /api/v1/automations
async fn list_automations(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AutomationListItem>>, (StatusCode, String)> {
    let db = state.db.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;
    let rows = db.load_automations().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list automations: {e}"),
        )
    })?;
    let now = chrono::Utc::now();
    let items = rows
        .into_iter()
        .map(|row| AutomationListItem {
            next_run: crate::scheduler::AutomationSchedule::next_run_from_stored(
                &row.schedule,
                row.last_run.as_deref(),
                now,
            )
            .map(|dt| dt.to_rfc3339()),
            row,
        })
        .collect::<Vec<_>>();
    Ok(Json(items))
}

/// POST /api/v1/automations
async fn create_automation(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateAutomationRequest>,
) -> Result<Json<crate::storage::AutomationRow>, (StatusCode, String)> {
    let db = state.db.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;

    if req.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Name cannot be empty".to_string()));
    }
    if req.prompt.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Prompt cannot be empty".to_string(),
        ));
    }
    if let Some(deliver_to) = req.deliver_to.as_deref() {
        parse_deliver_to(deliver_to)?;
    }
    let (trigger_kind, trigger_value) =
        normalize_automation_trigger(req.trigger.as_deref(), req.trigger_value.as_deref())?;

    let schedule =
        build_automation_schedule(req.schedule.as_deref(), req.cron.as_deref(), req.every)?;
    let prompt = req.prompt.trim().to_string();
    let compiled_plan = {
        let cfg = state.config.read().await.clone();
        crate::scheduler::automations::compile_automation_plan(&prompt, &cfg)
    };
    let id = uuid::Uuid::new_v4().to_string();
    let enabled = req.enabled.unwrap_or(true);
    let status = if !enabled {
        "paused"
    } else if compiled_plan.is_valid() {
        "active"
    } else {
        "invalid_config"
    };

    let plan_json = compiled_plan.plan_json();
    let dependencies_json = compiled_plan.dependencies_json();
    let validation_errors_json = compiled_plan.validation_errors_json();
    db.insert_automation_with_plan(
        &id,
        req.name.trim(),
        &prompt,
        &schedule,
        enabled,
        status,
        req.deliver_to.as_deref(),
        &trigger_kind,
        trigger_value.as_deref(),
        Some(&plan_json),
        &dependencies_json,
        compiled_plan.plan.version,
        validation_errors_json.as_deref(),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create automation: {e}"),
        )
    })?;

    let created = db.load_automation(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load created automation: {e}"),
        )
    })?;

    created.map(Json).ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Automation not found after insert".to_string(),
    ))
}

/// PATCH /api/v1/automations/{id}
async fn patch_automation(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<PatchAutomationRequest>,
) -> Result<Json<crate::storage::AutomationRow>, (StatusCode, String)> {
    let db = state.db.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;

    let current = db.load_automation(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load automation: {e}"),
        )
    })?;
    let Some(current) = current else {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Automation '{id}' not found"),
        ));
    };

    let requested_status = req.status.as_deref().map(|v| v.trim().to_string());
    let mut update = crate::storage::AutomationUpdate {
        name: req.name.map(|v| v.trim().to_string()),
        prompt: req.prompt.map(|v| v.trim().to_string()),
        enabled: req.enabled,
        status: requested_status.clone(),
        ..Default::default()
    };

    if req.clear_deliver_to.unwrap_or(false) {
        update.deliver_to = Some(None);
    } else if let Some(deliver_to) = req.deliver_to {
        parse_deliver_to(&deliver_to)?;
        update.deliver_to = Some(Some(deliver_to));
    }

    if req.schedule.is_some() || req.cron.is_some() || req.every.is_some() {
        update.schedule = Some(build_automation_schedule(
            req.schedule.as_deref(),
            req.cron.as_deref(),
            req.every,
        )?);
    }

    if req.clear_trigger_value.unwrap_or(false) {
        update.trigger_value = Some(None);
        if req.trigger.is_none() && current.trigger_kind == "contains" {
            update.trigger_kind = Some("always".to_string());
        }
    }

    if req.trigger.is_some() || req.trigger_value.is_some() {
        let desired_trigger = req.trigger.as_deref().unwrap_or(&current.trigger_kind);
        let desired_trigger_value = if req.trigger_value.is_some() {
            req.trigger_value.as_deref()
        } else {
            current.trigger_value.as_deref()
        };
        let (trigger_kind, trigger_value) =
            normalize_automation_trigger(Some(desired_trigger), desired_trigger_value)?;
        update.trigger_kind = Some(trigger_kind);
        update.trigger_value = Some(trigger_value);
    }

    let final_prompt = update
        .prompt
        .clone()
        .unwrap_or_else(|| current.prompt.clone());
    let final_enabled = update.enabled.unwrap_or(current.enabled);
    let compiled_plan = {
        let cfg = state.config.read().await.clone();
        crate::scheduler::automations::compile_automation_plan(&final_prompt, &cfg)
    };
    update.plan_json = Some(Some(compiled_plan.plan_json()));
    update.dependencies_json = Some(Some(compiled_plan.dependencies_json()));
    update.plan_version = Some(compiled_plan.plan.version);
    update.validation_errors = Some(compiled_plan.validation_errors_json());

    let mut next_status = update
        .status
        .clone()
        .unwrap_or_else(|| current.status.clone());
    if final_enabled && !compiled_plan.is_valid() {
        next_status = "invalid_config".to_string();
        let summary = compiled_plan.validation_errors.join(" | ");
        update.last_result = Some(Some(format!("Automation configuration invalid: {summary}")));
    } else if !final_enabled && requested_status.is_none() {
        next_status = "paused".to_string();
    } else if final_enabled
        && compiled_plan.is_valid()
        && requested_status.is_none()
        && current.status.eq_ignore_ascii_case("invalid_config")
    {
        next_status = "active".to_string();
    }
    update.status = Some(next_status);

    let updated = db.update_automation(&id, update).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to update automation: {e}"),
        )
    })?;

    if !updated {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Automation '{id}' not found (or no fields to update)"),
        ));
    }

    let row = db.load_automation(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load updated automation: {e}"),
        )
    })?;

    row.map(Json).ok_or((
        StatusCode::NOT_FOUND,
        format!("Automation '{id}' not found"),
    ))
}

/// DELETE /api/v1/automations/{id}
async fn delete_automation(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let db = state.db.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;

    let removed = db.delete_automation(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete automation: {e}"),
        )
    })?;

    if !removed {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Automation '{id}' not found"),
        ));
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "id": id
    })))
}

/// GET /api/v1/automations/{id}/history
async fn get_automation_history(
    Path(id): Path<String>,
    Query(q): Query<AutomationHistoryQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<crate::storage::AutomationRunRow>>, (StatusCode, String)> {
    let db = state.db.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let runs = db.load_automation_runs(&id, limit).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load automation history: {e}"),
        )
    })?;
    Ok(Json(runs))
}

/// POST /api/v1/automations/{id}/run
async fn run_automation_now(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<RunAutomationResponse>, (StatusCode, String)> {
    let db = state.db.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;

    let automation = db.load_automation(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load automation: {e}"),
        )
    })?;
    let Some(automation) = automation else {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Automation '{id}' not found"),
        ));
    };

    let compiled_plan = {
        let cfg = state.config.read().await.clone();
        crate::scheduler::automations::compile_automation_plan(&automation.prompt, &cfg)
    };
    let plan_json = compiled_plan.plan_json();
    let dependencies_json = compiled_plan.dependencies_json();
    let validation_errors_json = compiled_plan.validation_errors_json();
    let derived_status = if !automation.enabled {
        "paused".to_string()
    } else if compiled_plan.is_valid() {
        "active".to_string()
    } else {
        "invalid_config".to_string()
    };
    let _ = db
        .update_automation(
            &automation.id,
            crate::storage::AutomationUpdate {
                status: Some(derived_status.clone()),
                plan_json: Some(Some(plan_json)),
                dependencies_json: Some(Some(dependencies_json)),
                plan_version: Some(compiled_plan.plan.version),
                validation_errors: Some(validation_errors_json.clone()),
                ..Default::default()
            },
        )
        .await;

    if derived_status.eq_ignore_ascii_case("invalid_config") {
        let errors = crate::scheduler::automations::parse_validation_errors_json(
            validation_errors_json.as_deref(),
        );
        let reason = if errors.is_empty() {
            "Automation configuration is invalid. Update dependencies before running.".to_string()
        } else {
            format!(
                "Automation configuration is invalid: {}",
                errors.join(" | ")
            )
        };
        let run_id = uuid::Uuid::new_v4().to_string();
        let _ = db
            .insert_automation_run(&run_id, &automation.id, "error", Some(&reason))
            .await;
        let _ = db
            .update_automation(
                &automation.id,
                crate::storage::AutomationUpdate {
                    status: Some("invalid_config".to_string()),
                    last_result: Some(Some(reason.clone())),
                    touch_last_run: true,
                    ..Default::default()
                },
            )
            .await;
        return Ok(Json(RunAutomationResponse {
            run_id,
            status: "error".to_string(),
            message: reason,
        }));
    }

    let target = automation
        .deliver_to
        .clone()
        .unwrap_or_else(|| "cli:default".to_string());
    let (channel, chat_id) = parse_deliver_to(&target)?;

    let run_id = uuid::Uuid::new_v4().to_string();
    db.insert_automation_run(
        &run_id,
        &automation.id,
        "queued",
        Some("Manual run requested"),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create automation run: {e}"),
        )
    })?;

    let Some(inbound_tx) = &state.inbound_tx else {
        let _ = db
            .complete_automation_run(
                &run_id,
                "error",
                Some("Agent queue unavailable (setup-only mode)"),
            )
            .await;
        let _ = db
            .update_automation(
                &automation.id,
                crate::storage::AutomationUpdate {
                    status: Some("error".to_string()),
                    last_result: Some(Some(
                        "Manual run failed: agent queue unavailable".to_string(),
                    )),
                    touch_last_run: true,
                    ..Default::default()
                },
            )
            .await;
        return Ok(Json(RunAutomationResponse {
            run_id,
            status: "error".to_string(),
            message: "Agent queue unavailable (setup-only mode)".to_string(),
        }));
    };

    let runtime_prompt = crate::scheduler::automations::build_runtime_run_input_from_plan(
        automation.plan_json.as_deref(),
        &automation.prompt,
    );

    let msg = crate::bus::InboundMessage {
        channel,
        sender_id: format!("automation:{}", automation.id),
        chat_id,
        content: runtime_prompt,
        timestamp: chrono::Utc::now(),
        metadata: Some(crate::bus::MessageMetadata {
            is_system: true,
            scheduler_kind: Some("automation".to_string()),
            scheduler_job_id: Some(automation.id.clone()),
            automation_run_id: Some(run_id.clone()),
            ..Default::default()
        }),
    };

    match inbound_tx.send(msg).await {
        Ok(()) => {
            let result_msg = format!("Run queued to {target}");
            let _ = db
                .update_automation(
                    &automation.id,
                    crate::storage::AutomationUpdate {
                        status: Some("active".to_string()),
                        last_result: Some(Some(result_msg.clone())),
                        touch_last_run: true,
                        ..Default::default()
                    },
                )
                .await;
            Ok(Json(RunAutomationResponse {
                run_id,
                status: "queued".to_string(),
                message: result_msg,
            }))
        }
        Err(e) => {
            let msg = format!("Failed to enqueue automation run: {e}");
            let _ = db
                .complete_automation_run(&run_id, "error", Some(&msg))
                .await;
            let _ = db
                .update_automation(
                    &automation.id,
                    crate::storage::AutomationUpdate {
                        status: Some("error".to_string()),
                        last_result: Some(Some(msg.clone())),
                        touch_last_run: true,
                        ..Default::default()
                    },
                )
                .await;
            Ok(Json(RunAutomationResponse {
                run_id,
                status: "error".to_string(),
                message: msg,
            }))
        }
    }
}

// --- Token Usage ---

#[derive(Deserialize)]
struct UsageQuery {
    session: Option<String>,
    since: Option<String>,
    until: Option<String>,
}

#[derive(Serialize)]
struct UsageResponse {
    models: Vec<crate::storage::TokenUsageAggRow>,
    days: Vec<crate::storage::TokenUsageDailyRow>,
    totals: UsageTotals,
}

#[derive(Serialize)]
struct UsageTotals {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
    call_count: i64,
}

async fn get_usage(
    State(state): State<Arc<AppState>>,
    Query(q): Query<UsageQuery>,
) -> Result<Json<UsageResponse>, (StatusCode, String)> {
    let db = state.db.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;

    let rows = db
        .query_token_usage(q.session.as_deref(), q.since.as_deref(), q.until.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query usage: {}", e),
            )
        })?;

    let days = db
        .query_token_usage_daily(q.session.as_deref(), q.since.as_deref(), q.until.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query daily usage: {}", e),
            )
        })?;

    let totals = UsageTotals {
        prompt_tokens: rows.iter().map(|r| r.prompt_tokens).sum(),
        completion_tokens: rows.iter().map(|r| r.completion_tokens).sum(),
        total_tokens: rows.iter().map(|r| r.total_tokens).sum(),
        call_count: rows.iter().map(|r| r.call_count).sum(),
    };

    Ok(Json(UsageResponse {
        models: rows,
        days,
        totals,
    }))
}

// ─────────────────────────────────────────────────────────────────────────
// Email Accounts (multi-account)
// ─────────────────────────────────────────────────────────────────────────

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

/// Generate a random trigger word like "hm-x7k2p9" for on_demand email mode.
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

fn generate_email_trigger_word() -> String {
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

// ─── Knowledge Base (RAG) API ──────────────────────────────────────

/// GET /api/v1/knowledge/stats
#[cfg(feature = "local-embeddings")]
async fn knowledge_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let Some(ref rag) = state.rag_engine else {
        return Json(serde_json::json!({"error": "Knowledge base not initialized"})).into_response();
    };
    let engine = rag.lock().await;
    match engine.stats().await {
        Ok(stats) => Json(serde_json::json!(stats)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

/// GET /api/v1/knowledge/sources
#[cfg(feature = "local-embeddings")]
async fn list_knowledge_sources(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let Some(ref rag) = state.rag_engine else {
        return Json(serde_json::json!({"error": "Knowledge base not initialized"})).into_response();
    };
    let engine = rag.lock().await;
    match engine.list_sources().await {
        Ok(sources) => Json(serde_json::json!({"sources": sources})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

/// DELETE /api/v1/knowledge/sources?id=N
#[cfg(feature = "local-embeddings")]
async fn delete_knowledge_source(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(ref rag) = state.rag_engine else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Knowledge base not initialized"})),
        )
            .into_response();
    };

    let Some(id_str) = params.get("id") else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Missing 'id' parameter"})),
        )
            .into_response();
    };

    let Ok(source_id) = id_str.parse::<i64>() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Invalid 'id' parameter"})),
        )
            .into_response();
    };

    let mut engine = rag.lock().await;
    match engine.remove_source(source_id).await {
        Ok(_) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

/// GET /api/v1/knowledge/search?q=...&limit=5
#[cfg(feature = "local-embeddings")]
async fn search_knowledge(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let Some(ref rag) = state.rag_engine else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Knowledge base not initialized"})),
        )
            .into_response();
    };

    let Some(query) = params.get("q") else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Missing 'q' parameter"})),
        )
            .into_response();
    };

    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(5);

    let mut engine = rag.lock().await;
    match engine.search(query, limit).await {
        Ok(results) => {
            let items: Vec<serde_json::Value> = results
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "source_file": r.source_file,
                        "chunk_index": r.chunk.chunk_index,
                        "heading": r.chunk.heading,
                        "content": r.chunk.content,
                        "score": r.score,
                        "sensitive": r.chunk.sensitive,
                        "chunk_id": r.chunk.id,
                    })
                })
                .collect();
            Json(serde_json::json!({"results": items})).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

/// POST /api/v1/knowledge/ingest — multipart file upload
#[cfg(feature = "local-embeddings")]
async fn ingest_knowledge(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let Some(ref rag) = state.rag_engine else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Knowledge base not initialized"})),
        )
            .into_response();
    };

    let mut ingested = Vec::new();
    let mut errors = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let file_name = field
            .file_name()
            .unwrap_or("upload.txt")
            .to_string();

        let Ok(bytes) = field.bytes().await else {
            errors.push(format!("{file_name}: failed to read upload"));
            continue;
        };

        // Write to a temp file so RagEngine can process it
        let tmp_dir = std::env::temp_dir().join("homun_uploads");
        if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
            errors.push(format!("{file_name}: {e}"));
            continue;
        }
        let tmp_path = tmp_dir.join(&file_name);
        if let Err(e) = std::fs::write(&tmp_path, &bytes) {
            errors.push(format!("{file_name}: {e}"));
            continue;
        }

        let mut engine = rag.lock().await;
        match engine.ingest_file(&tmp_path, "web").await {
            Ok(Some(id)) => ingested.push(serde_json::json!({"file": file_name, "source_id": id})),
            Ok(None) => ingested.push(serde_json::json!({"file": file_name, "status": "duplicate"})),
            Err(e) => errors.push(format!("{file_name}: {e}")),
        }

        // Clean up temp file
        let _ = std::fs::remove_file(&tmp_path);
    }

    Json(serde_json::json!({
        "ingested": ingested,
        "errors": errors,
    }))
    .into_response()
}

/// POST /api/v1/knowledge/ingest-directory — index a server-side folder
#[cfg(feature = "local-embeddings")]
async fn ingest_knowledge_directory(
    State(state): State<Arc<AppState>>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let Some(ref rag) = state.rag_engine else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Knowledge base not initialized"})),
        )
            .into_response();
    };

    let path_str = req["path"].as_str().unwrap_or("");
    if path_str.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Missing 'path' field"})),
        )
            .into_response();
    }
    let recursive = req["recursive"].as_bool().unwrap_or(false);

    // Expand tilde
    let path = if path_str.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            home.join(&path_str[2..])
        } else {
            std::path::PathBuf::from(path_str)
        }
    } else {
        std::path::PathBuf::from(path_str)
    };

    if !path.is_dir() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Not a directory: {}", path.display())})),
        )
            .into_response();
    }

    let mut engine = rag.lock().await;
    match engine.ingest_directory(&path, recursive, "web").await {
        Ok(ids) => Json(serde_json::json!({
            "indexed": ids.len(),
            "source_ids": ids,
        }))
        .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}

/// POST /api/v1/knowledge/reveal — reveal a sensitive chunk (optionally with TOTP)
#[cfg(feature = "local-embeddings")]
async fn reveal_knowledge_chunk(
    State(state): State<Arc<AppState>>,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let Some(ref rag) = state.rag_engine else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Knowledge base not initialized"})),
        )
            .into_response();
    };

    let Some(chunk_id) = req["chunk_id"].as_i64() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Missing 'chunk_id'"})),
        )
            .into_response();
    };

    // If 2FA is enabled, verify TOTP code
    #[cfg(feature = "vault-2fa")]
    {
        use crate::security::{TotpManager, TwoFactorStorage};

        if let Ok(storage) = TwoFactorStorage::new() {
            if let Ok(config) = storage.load() {
                if config.enabled {
                    let code = req["code"].as_str().unwrap_or("");
                    if code.is_empty() {
                        return (
                            StatusCode::UNAUTHORIZED,
                            Json(serde_json::json!({"error": "2FA code required", "requires_2fa": true})),
                        )
                            .into_response();
                    }
                    match TotpManager::new(&config.totp_secret, &config.account) {
                        Ok(manager) => {
                            if !manager.verify(code) {
                                return (
                                    StatusCode::FORBIDDEN,
                                    Json(serde_json::json!({"error": "Invalid 2FA code"})),
                                )
                                    .into_response();
                            }
                        }
                        Err(_) => {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(serde_json::json!({"error": "2FA configuration error"})),
                            )
                                .into_response();
                        }
                    }
                }
            }
        }
    }

    let engine = rag.lock().await;
    match engine.reveal_chunk(chunk_id).await {
        Ok(Some(chunk)) => Json(serde_json::json!({
            "chunk_id": chunk.id,
            "content": chunk.content,
            "heading": chunk.heading,
        }))
        .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Chunk not found"})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}
