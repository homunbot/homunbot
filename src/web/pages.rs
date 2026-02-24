use std::sync::Arc;

use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;

use super::server::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(dashboard))
        .route("/setup", get(setup_page))
        .route("/chat", get(chat_page))
        .route("/skills", get(skills_page))
        .route("/memory", get(memory_page))
        .route("/vault", get(vault_page))
        .route("/permissions", get(permissions_page))
        .route("/account", get(account_page))
        .route("/logs", get(logs_page))
}

// ─── Shared layout pieces ───────────────────────────────────────

/// SVG icons used in the sidebar nav
const ICON_DASHBOARD: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="1" y="1" width="7" height="7" rx="1.5"/><rect x="10" y="1" width="7" height="4" rx="1.5"/><rect x="1" y="10" width="7" height="4" rx="1.5" transform="translate(0,3)"/><rect x="10" y="7" width="7" height="7" rx="1.5" transform="translate(0,3)"/></svg>"#;
const ICON_CHAT: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12.5V3.5A1.5 1.5 0 0 1 3.5 2h11A1.5 1.5 0 0 1 16 3.5v7a1.5 1.5 0 0 1-1.5 1.5H6L2 16V12.5z"/><line x1="6" y1="6" x2="12" y2="6"/><line x1="6" y1="9" x2="10" y2="9"/></svg>"#;
const ICON_SKILLS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 1L11.5 6.5 17 7.5 13 11.5 14 17 9 14.5 4 17 5 11.5 1 7.5 6.5 6.5z"/></svg>"#;
const ICON_SETTINGS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="9" r="2.5"/><path d="M14.7 11.1a1.2 1.2 0 0 0 .24 1.32l.04.04a1.44 1.44 0 1 1-2.04 2.04l-.04-.04a1.2 1.2 0 0 0-1.32-.24 1.2 1.2 0 0 0-.72 1.08v.12a1.44 1.44 0 0 1-2.88 0v-.06a1.2 1.2 0 0 0-.78-1.08 1.2 1.2 0 0 0-1.32.24l-.04.04a1.44 1.44 0 1 1-2.04-2.04l.04-.04a1.2 1.2 0 0 0 .24-1.32 1.2 1.2 0 0 0-1.08-.72h-.12a1.44 1.44 0 0 1 0-2.88h.06a1.2 1.2 0 0 0 1.08-.78 1.2 1.2 0 0 0-.24-1.32l-.04-.04a1.44 1.44 0 1 1 2.04-2.04l.04.04a1.2 1.2 0 0 0 1.32.24h.06a1.2 1.2 0 0 0 .72-1.08V2.88a1.44 1.44 0 0 1 2.88 0v.06a1.2 1.2 0 0 0 .72 1.08 1.2 1.2 0 0 0 1.32-.24l.04-.04a1.44 1.44 0 1 1 2.04 2.04l-.04.04a1.2 1.2 0 0 0-.24 1.32v.06a1.2 1.2 0 0 0 1.08.72h.12a1.44 1.44 0 0 1 0 2.88h-.06a1.2 1.2 0 0 0-1.08.72z"/></svg>"#;
const ICON_LOGS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 15V3a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v12"/><line x1="6" y1="6" x2="12" y2="6"/><line x1="6" y1="9" x2="12" y2="9"/><line x1="6" y1="12" x2="9" y2="12"/></svg>"#;
const ICON_MEMORY: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 2v14"/><path d="M3 9h12"/><circle cx="9" cy="9" r="3"/><circle cx="9" cy="9" r="7"/></svg>"#;
const ICON_VAULT: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="5" width="14" height="11" rx="1.5"/><path d="M5 5V4a4 4 0 0 1 8 0v1"/><circle cx="9" cy="11" r="1.5"/><path d="M9 12.5V14"/></svg>"#;
const ICON_PERMISSIONS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="1" y="4" width="16" height="12" rx="1.5"/><circle cx="9" cy="10" r="2"/><path d="M5 4V3a4 4 0 0 1 8 0v1"/></svg>"#;
const ICON_ACCOUNT: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="6" r="3.5"/><path d="M3 17c0-3.5 2.5-6 6-6s6 2.5 6 6"/></svg>"#;

/// Channel icons — minimal stroke SVGs for dashboard/settings
const ICON_WEB: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="9" r="7.5"/><path d="M1.5 9h15"/><path d="M9 1.5a11.5 11.5 0 0 1 3 7.5 11.5 11.5 0 0 1-3 7.5"/><path d="M9 1.5a11.5 11.5 0 0 0-3 7.5 11.5 11.5 0 0 0 3 7.5"/></svg>"#;
const ICON_TELEGRAM: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M15.5 2.5L1.5 8l5 2m9-7.5L6.5 10m9-7.5l-3 13-5.5-5.5"/><path d="M6.5 10v4.5l2.5-2.5"/></svg>"#;
const ICON_DISCORD: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6.5 3C5 3 3 3.5 2 5c-1.5 3-.5 7.5 1 9.5.5.5 1.5 1.5 3 1.5s2-1 3-1 1.5 1 3 1 2.5-1 3-1.5c1.5-2 2.5-6.5 1-9.5-1-1.5-3-2-4.5-2"/><circle cx="6.5" cy="10" r="1"/><circle cx="11.5" cy="10" r="1"/></svg>"#;
const ICON_PHONE: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="1" width="10" height="16" rx="2"/><line x1="9" y1="14" x2="9" y2="14"/></svg>"#;

/// Logo icon — serves the SVG logotype via <img> tag.
const LOGO_ICON: &str = r#"<img class="logo-icon" src="/static/img/logo.svg" alt="HOMUN">"#;


/// Build the sidebar navigation HTML
fn sidebar(active: &str) -> String {
    let nav_items = [
        ("dashboard", "/", "Dashboard", ICON_DASHBOARD),
        ("chat", "/chat", "Chat", ICON_CHAT),
        ("skills", "/skills", "Skills", ICON_SKILLS),
        ("memory", "/memory", "Memory", ICON_MEMORY),
        ("vault", "/vault", "Vault", ICON_VAULT),
        ("permissions", "/permissions", "Permissions", ICON_PERMISSIONS),
        ("account", "/account", "Account", ICON_ACCOUNT),
        ("settings", "/setup", "Settings", ICON_SETTINGS),
        ("logs", "/logs", "Logs", ICON_LOGS),
    ];

    let links: String = nav_items
        .iter()
        .map(|(id, href, label, icon)| {
            let cls = if *id == active { " active" } else { "" };
            format!(
                r#"<a href="{href}" class="nav-link{cls}">
                    <span class="nav-icon">{icon}</span>
                    <span class="nav-label">{label}</span>
                </a>"#
            )
        })
        .collect();

    format!(
        r#"<nav class="sidebar">
            <div class="sidebar-header">
                <a href="/" class="logo-link">
                    {icon}
                </a>
            </div>
            <div class="nav">
                <div class="nav-section">Main</div>
                {links}
            </div>
            <div class="sidebar-footer">
                <span class="version-badge">v{version}</span>
            </div>
        </nav>"#,
        icon = LOGO_ICON,
        links = links,
        version = env!("CARGO_PKG_VERSION"),
    )
}

/// HTML document skeleton
fn page_html(title: &str, active: &str, body: &str, scripts: &[&str]) -> String {
    let sidebar_html = sidebar(active);
    let script_tags: String = scripts
        .iter()
        .map(|s| format!(r#"<script src="/static/js/{s}"></script>"#))
        .collect();

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{title} — Homun</title>
    <link rel="icon" href="/static/img/favicon.svg" type="image/svg+xml">
    <link rel="stylesheet" href="/static/css/style.css">
</head>
<body>
    <div class="app">
        {sidebar_html}
        {body}
    </div>
    {script_tags}
</body>
</html>"#
    )
}

// ─── Dashboard ──────────────────────────────────────────────────

async fn dashboard(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let config = state.config.read().await;
    let uptime = state.started_at.elapsed().as_secs();
    let provider = config
        .resolve_provider(&config.agent.model)
        .map(|(n, _)| n)
        .unwrap_or("none");

    let skills_count = crate::skills::SkillInstaller::list_installed()
        .await
        .map(|s| s.len())
        .unwrap_or(0);

    let channels_html = build_channels_html(&config);
    let uptime_display = format_uptime(uptime);
    let channel_count = count_active_channels(&config);

    // Show warning if no model is configured
    let no_model_warning = if provider == "none" || config.agent.model.is_empty() {
        r#"<div class="no-model-warning">
            <span class="no-model-warning-icon">⚠️</span>
            <div class="no-model-warning-text">
                <strong>No model configured</strong>
                <p>Select a model in <a href="/setup">Settings → Agent Configuration</a> to enable the assistant.</p>
            </div>
        </div>"#
    } else {
        ""
    };

    let body = format!(
        r#"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Dashboard</h1>
                        <span class="badge badge-success">Running</span>
                    </div>
                </div>

                {no_model_warning}

                <div class="stats-grid">
                    <a href="/setup" class="stat-card stat-card-link">
                        <div class="stat-label">Model</div>
                        <div class="stat-value">{model}</div>
                        <div class="stat-sub">via {provider} → Settings</div>
                    </a>
                    <div class="stat-card">
                        <div class="stat-label">Uptime</div>
                        <div class="stat-value" data-live-uptime="{uptime_secs}">{uptime_display}</div>
                        <div class="stat-sub">since start</div>
                    </div>
                    <div class="stat-card" data-editable data-key="agent.temperature">
                        <div class="stat-label">Temperature</div>
                        <div class="stat-value">{temperature}</div>
                        <div class="stat-sub">creativity</div>
                        <div class="inline-edit">
                            <input type="number" class="inline-input" value="{temperature}" step="0.1" min="0" max="2">
                            <div class="inline-actions">
                                <button class="btn btn-save btn-sm">Save</button>
                                <button class="btn btn-cancel btn-sm">Cancel</button>
                            </div>
                        </div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-label">Channels</div>
                        <div class="stat-value">{channel_count}</div>
                        <div class="stat-sub">{skills_count} skills</div>
                    </div>
                </div>

                <section class="section">
                    <h2>Channels</h2>
                    <div class="item-list">
                        {channels_html}
                    </div>
                </section>

                <section class="section">
                    <h2>Quick Actions</h2>
                    <div class="actions">
                        <a href="/chat" class="btn btn-primary">Open Chat</a>
                        <a href="/skills" class="btn btn-secondary">Manage Skills</a>
                        <a href="/setup" class="btn btn-secondary">Settings</a>
                    </div>
                </section>
            </div>
        </main>"#,
        model = config.agent.model,
        provider = provider,
        uptime_secs = uptime,
        uptime_display = uptime_display,
        temperature = config.agent.temperature,
        skills_count = skills_count,
        channel_count = channel_count,
        channels_html = channels_html,
        no_model_warning = no_model_warning,
    );

    Html(page_html("Dashboard", "dashboard", &body, &["dashboard.js"])).into_response()
}

// ─── Settings ───────────────────────────────────────────────────

async fn setup_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let config = state.config.read().await;
    let providers_html = build_providers_html(&config);

    // Show warning if no model is configured
    let no_model_warning = if config.agent.model.is_empty() {
        r#"<div class="no-model-warning">
            <span class="no-model-warning-icon">⚠️</span>
            <div class="no-model-warning-text">
                <strong>No model configured</strong>
                <p>Select a model below to enable the assistant.</p>
            </div>
        </div>"#
    } else {
        ""
    };

    let body = format!(
        r#"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Settings</h1>
                    </div>
                </div>

                {no_model_warning}

                <section class="section">
                    <h2>Agent Configuration</h2>
                    <form class="form form--full" id="agent-form">
                        <div class="form-group model-selector-section">
                            <label class="model-selector-label">Model</label>
                            <select id="model-select" class="input">
                                <option value="">Loading models…</option>
                            </select>
                            <input type="hidden" name="model" id="model-value" value="{model}">
                            <div class="form-hint">Select a model from configured providers, or type to search.</div>
                        </div>
                        <div class="form-row">
                            <div class="form-group">
                                <label>Max Tokens</label>
                                <input type="number" name="max_tokens" value="{max_tokens}" class="input">
                            </div>
                            <div class="form-group">
                                <label>Temperature</label>
                                <input type="number" name="temperature" value="{temperature}" step="0.1" min="0" max="2" class="input">
                            </div>
                            <div class="form-group">
                                <label>Max Iterations</label>
                                <input type="number" name="max_iterations" value="{max_iterations}" class="input">
                            </div>
                        </div>
                        <button type="submit" class="btn btn-primary">Save Agent Config</button>
                    </form>
                </section>

                <section class="section">
                    <h2>Providers</h2>
                    <div class="provider-grid">
                        {providers_html}
                    </div>
                </section>

                <section class="section">
                    <h2>Channels</h2>
                    <div class="provider-grid" id="channel-grid">
                        {channels_html}
                    </div>
                </section>

                <section class="section">
                    <h2>Memory</h2>
                    <form class="form" id="memory-form">
                        <div class="form-row">
                            <div class="form-group">
                                <label>Conversation Retention (days)</label>
                                <input type="number" name="conversation_retention_days" value="{conversation_retention_days}" min="1" max="365" class="input">
                                <div class="form-hint">Delete chat messages older than this many days.</div>
                            </div>
                            <div class="form-group">
                                <label>History Retention (days)</label>
                                <input type="number" name="history_retention_days" value="{history_retention_days}" min="1" max="3650" class="input">
                                <div class="form-hint">Delete memory chunks older than this many days.</div>
                            </div>
                            <div class="form-group">
                                <label>Daily Archive Months</label>
                                <input type="number" name="daily_archive_months" value="{daily_archive_months}" min="1" max="24" class="input">
                                <div class="form-hint">Group daily logs by month in the UI.</div>
                            </div>
                        </div>
                        <div class="form-group">
                            <label class="toggle-label-inline">
                                <input type="checkbox" name="auto_cleanup" class="toggle-input" {auto_cleanup_checked}>
                                <span>Auto-cleanup on startup</span>
                            </label>
                            <div class="form-hint">Automatically run memory cleanup when gateway starts.</div>
                        </div>
                        <div class="form-row">
                            <button type="submit" class="btn btn-primary">Save Memory Config</button>
                            <button type="button" class="btn btn-secondary" id="btn-run-cleanup">Run Cleanup Now</button>
                        </div>
                        <div id="memory-result" class="form-hint" style="margin-top:10px;"></div>
                    </form>
                </section>
            </div>
        </main>

        <!-- Provider Configuration Modal -->
        <div id="provider-modal" class="modal">
            <div class="modal-backdrop"></div>
            <div class="modal-content">
                <div class="modal-header">
                    <h3 class="modal-title" id="modal-provider-name">Provider</h3>
                    <button class="modal-close" type="button">&times;</button>
                </div>
                <div class="modal-body">
                    <p class="modal-description" id="modal-provider-desc"></p>

                    <form id="provider-config-form">
                        <input type="hidden" id="modal-provider-id" name="provider">

                        <div class="form-group" id="api-key-group">
                            <label for="api-key">API Key</label>
                            <input type="password" id="api-key" name="api_key" class="input" placeholder="sk-...">
                            <div class="form-hint">Your API key is stored locally and never sent to our servers.</div>
                        </div>

                        <div class="form-group" id="api-base-group">
                            <label for="api-base">Base URL</label>
                            <input type="text" id="api-base" name="api_base" class="input" placeholder="https://api.example.com/v1">
                            <div class="form-hint" id="api-base-hint">Custom API endpoint (optional)</div>
                        </div>

                        <div class="modal-actions">
                            <button type="button" class="btn btn-secondary modal-cancel">Cancel</button>
                            <button type="submit" class="btn btn-primary">Save</button>
                        </div>
                    </form>
                </div>
            </div>
        </div>

        <!-- Channel Configuration Modal -->
        <div id="channel-modal" class="modal">
            <div class="modal-backdrop"></div>
            <div class="modal-content">
                <div class="modal-header">
                    <h3 class="modal-title" id="modal-channel-name">Channel</h3>
                    <button class="modal-close ch-modal-close" type="button">&times;</button>
                </div>
                <div class="modal-body">
                    <div id="channel-guide" class="channel-guide"></div>

                    <form id="channel-config-form">
                        <input type="hidden" id="modal-channel-id" name="channel">

                        <div class="form-group" id="ch-token-group" style="display:none;">
                            <label for="ch-token">Bot Token</label>
                            <input type="password" id="ch-token" name="token" class="input">
                            <div class="form-hint" id="ch-token-hint">Stored encrypted locally.</div>
                        </div>

                        <div class="form-group" id="ch-phone-group" style="display:none;">
                            <label for="ch-phone">Phone Number</label>
                            <input type="tel" id="ch-phone" name="phone_number" class="input" placeholder="393331234567">
                            <div class="form-hint">International format without + (e.g. 393331234567)</div>
                        </div>

                        <div id="ch-wa-pairing" style="display:none;">
                            <div id="ch-wa-pairing-status" class="pairing-status"></div>
                            <div id="ch-wa-pairing-code" class="pairing-code" style="display:none;"></div>
                        </div>

                        <div class="form-group" id="ch-allow-from-group" style="display:none;">
                            <label>Allowed Users</label>
                            <input type="text" id="ch-allow-from" name="allow_from" class="input" placeholder="User IDs, comma-separated">
                            <div class="form-hint" id="ch-allow-from-hint">Only these users can interact with the bot.</div>
                        </div>

                        <div class="form-group" id="ch-discord-channel-group" style="display:none;">
                            <label for="ch-discord-channel">Default Channel ID</label>
                            <input type="text" id="ch-discord-channel" name="default_channel_id" class="input">
                            <div class="form-hint">For proactive messages (optional)</div>
                        </div>

                        <div class="form-group" id="ch-web-host-group" style="display:none;">
                            <label for="ch-web-host">Host</label>
                            <input type="text" id="ch-web-host" name="host" class="input">
                        </div>
                        <div class="form-group" id="ch-web-port-group" style="display:none;">
                            <label for="ch-web-port">Port</label>
                            <input type="number" id="ch-web-port" name="port" class="input">
                        </div>

                        <div class="modal-actions">
                            <button type="button" class="btn btn-secondary ch-modal-cancel">Cancel</button>
                            <button type="submit" class="btn btn-primary" id="btn-ch-save">Save &amp; Enable</button>
                            <button type="button" id="btn-test-channel" class="btn btn-secondary">Test Connection</button>
                            <button type="button" id="btn-wa-pair" class="btn btn-success" style="display:none;">Start Pairing</button>
                        </div>
                    </form>

                    <div id="ch-test-result" class="form-hint" style="margin-top:10px;"></div>
                </div>
            </div>
        </div>"#,
        model = config.agent.model,
        max_tokens = config.agent.max_tokens,
        temperature = config.agent.temperature,
        max_iterations = config.agent.max_iterations,
        conversation_retention_days = config.memory.conversation_retention_days,
        history_retention_days = config.memory.history_retention_days,
        daily_archive_months = config.memory.daily_archive_months,
        auto_cleanup_checked = if config.memory.auto_cleanup { "checked" } else { "" },
        providers_html = providers_html,
        channels_html = build_channels_cards_html(&config),
        no_model_warning = no_model_warning,
    );

    Html(page_html("Settings", "settings", &body, &["setup.js"]))
}

// ─── Chat ───────────────────────────────────────────────────────

async fn chat_page() -> Html<String> {
    let body = r#"<main class="content chat-layout">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Chat</h1>
                        <span class="badge badge-neutral" id="ws-status">Connecting…</span>
                    </div>
                    <div class="chat-actions">
                        <button class="btn btn-ghost btn-sm" id="btn-new-chat" title="New conversation">
                            <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><line x1="9" y1="3" x2="9" y2="15"/><line x1="3" y1="9" x2="15" y2="9"/></svg>
                        </button>
                        <button class="btn btn-ghost btn-sm" id="btn-compact-chat" title="Compact conversation">
                            <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 9 12 15 6"/></svg>
                        </button>
                        <button class="btn btn-ghost btn-sm" id="btn-clear-chat" title="Clear screen">
                            <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 4 9 9 14 4"/><polyline points="4 14 9 9 14 14"/></svg>
                        </button>
                    </div>
                </div>
                <div class="chat-messages" id="messages"></div>
                <form class="chat-input" id="chat-form">
                    <textarea id="chat-text" placeholder="Send a message… (Shift+Enter for new line)" autocomplete="off" class="input chat-textarea" rows="1"></textarea>
                    <button type="submit" class="btn btn-primary">Send</button>
                </form>
            </div>
        </main>
        <script src="https://cdn.jsdelivr.net/npm/marked/marked.min.js"></script>
        <script src="https://cdn.jsdelivr.net/npm/dompurify/dist/purify.min.js"></script>"#;

    Html(page_html("Chat", "chat", body, &["chat.js"]))
}

// ─── Skills ─────────────────────────────────────────────────────

async fn skills_page() -> Html<String> {
    let installed = crate::skills::SkillInstaller::list_installed()
        .await
        .unwrap_or_default();

    let installed_html: String = if installed.is_empty() {
        r#"<div class="empty-state" id="installed-empty">
                <svg class="empty-state-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M12 2L15 8.5 22 9.5 17 14.5 18 22 12 19 6 22 7 14.5 2 9.5 9 8.5z"/></svg>
                <p>No skills installed yet.</p>
                <p>Search ClawHub or enter <code>owner/repo</code> to install.</p>
            </div>"#
            .to_string()
    } else {
        installed
            .iter()
            .map(|s| {
                let source = if s.path.join(".clawhub-source").exists() {
                    "clawhub"
                } else if s.path.join(".openskills-source").exists() {
                    "openskills"
                } else {
                    "github"
                };
                let source_label = match source {
                    "clawhub" => "ClawHub",
                    "openskills" => "Open Skills",
                    _ => "GitHub",
                };
                format!(
                    r#"<div class="skill-card" data-skill-name="{name}" data-skill-source="{source}">
                        <div class="skill-card-header">
                            <div class="skill-name">{name}</div>
                            <span class="skill-source-badge skill-source-badge--{source}">{source_label}</span>
                        </div>
                        <div class="skill-desc">{desc}</div>
                        <div class="skill-card-footer">
                            <span class="skill-path">{path}</span>
                            <button class="btn btn-sm btn-danger skill-remove-btn" data-skill="{name}">Remove</button>
                        </div>
                    </div>"#,
                    name = s.name,
                    desc = s.description,
                    path = s.path.display(),
                    source = source,
                    source_label = source_label,
                )
            })
            .collect()
    };

    // Count skills by source for the source indicators
    let mut count_clawhub = 0usize;
    let mut count_openskills = 0usize;
    let mut count_github = 0usize;
    for s in &installed {
        if s.path.join(".clawhub-source").exists() {
            count_clawhub += 1;
        } else if s.path.join(".openskills-source").exists() {
            count_openskills += 1;
        } else {
            count_github += 1;
        }
    }

    // Build source counter chips (only show non-zero)
    let mut source_chips = Vec::new();
    if count_clawhub > 0 {
        source_chips.push(format!(
            r#"<span class="skill-source-chip skill-source-chip--clawhub">{} ClawHub</span>"#,
            count_clawhub
        ));
    }
    if count_github > 0 {
        source_chips.push(format!(
            r#"<span class="skill-source-chip skill-source-chip--github">{} GitHub</span>"#,
            count_github
        ));
    }
    if count_openskills > 0 {
        source_chips.push(format!(
            r#"<span class="skill-source-chip skill-source-chip--openskills">{} Open Skills</span>"#,
            count_openskills
        ));
    }
    let source_chips_html = if source_chips.is_empty() {
        String::new()
    } else {
        format!(r#"<div class="skill-source-chips" id="source-chips">{}</div>"#, source_chips.join(""))
    };

    let body = format!(
        r#"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Skills</h1>
                        <span class="badge badge-info" id="installed-count">{count} installed</span>
                        {source_chips_html}
                    </div>
                </div>

                <div class="skills-search">
                    <svg class="skills-search-icon" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="7.5" cy="7.5" r="5.5"/><path d="M12 12l4.5 4.5"/></svg>
                    <input type="text" id="skill-search-input" class="input skills-search-input" placeholder="Search ClawHub, GitHub &amp; Open Skills, or enter owner/repo to install..." autocomplete="off">
                    <div class="skills-search-spinner" id="search-spinner" style="display:none"></div>
                </div>

                <div class="catalog-stats" id="catalog-stats"></div>

                <div class="catalog-banner" id="catalog-banner" style="display:none">
                    <div class="catalog-banner-content">
                        <div class="catalog-banner-icon">
                            <div class="skills-search-spinner" style="display:inline-block"></div>
                        </div>
                        <div class="catalog-banner-text">
                            <strong id="catalog-banner-title">Downloading skill catalog...</strong>
                            <span id="catalog-banner-detail">This only happens once, search will be instant after.</span>
                        </div>
                    </div>
                    <div class="catalog-banner-progress">
                        <div class="catalog-banner-bar" id="catalog-bar"></div>
                    </div>
                </div>

                <section class="section" id="installed-section">
                    <h2>Installed Skills</h2>
                    <div class="skill-list" id="installed-grid">
                        {installed_html}
                    </div>
                </section>

                <section class="section skills-results-section" id="search-section" style="display:none">
                    <div class="skills-results-header">
                        <h2>Search Results</h2>
                        <span class="badge badge-neutral" id="search-count"></span>
                    </div>
                    <div class="skill-list" id="search-grid"></div>
                </section>

                <div id="skill-toast" class="skill-toast" style="display:none"></div>

                <div class="skill-modal-overlay" id="skill-modal-overlay">
                    <div class="skill-modal" id="skill-modal">
                        <div class="skill-modal-header">
                            <div>
                                <div class="skill-modal-title" id="modal-title"></div>
                                <div class="skill-modal-subtitle" id="modal-subtitle"></div>
                            </div>
                            <button class="skill-modal-close" id="modal-close">&times;</button>
                        </div>
                        <div class="skill-modal-meta" id="modal-meta"></div>
                        <div class="skill-modal-body">
                            <div class="skill-modal-content" id="modal-content"></div>
                        </div>
                        <div class="skill-modal-footer" id="modal-footer"></div>
                    </div>
                </div>
            </div>
        </main>"#,
        count = installed.len(),
        installed_html = installed_html,
        source_chips_html = source_chips_html,
    );

    Html(page_html("Skills", "skills", &body, &["skills.js"]))
}

// ─── Logs ───────────────────────────────────────────────────────

async fn logs_page() -> Html<String> {
    let body = r#"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Logs</h1>
                    </div>
                </div>
                <div class="log-viewer" id="log-viewer">
                    <div class="empty-state">
                        <svg class="empty-state-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 15V3a1 1 0 0 1 1-1h16a1 1 0 0 1 1 1v18"/><line x1="7" y1="7" x2="17" y2="7"/><line x1="7" y1="11" x2="17" y2="11"/><line x1="7" y1="15" x2="12" y2="15"/></svg>
                        <p>Log streaming coming soon.</p>
                        <p>Check your terminal for real-time logs.</p>
                    </div>
                </div>
            </div>
        </main>"#;

    Html(page_html("Logs", "logs", body, &[]))
}

// ─── Memory ──────────────────────────────────────────────────────

async fn memory_page(State(state): State<Arc<AppState>>) -> Html<String> {
    // Gather stats for server-render
    let data_dir = crate::config::Config::data_dir();
    let chunk_count = match state.db.as_ref() {
        Some(db) => db.count_memory_chunks().await.unwrap_or(0),
        None => 0,
    };
    let daily_count = std::fs::read_dir(data_dir.join("memory"))
        .map(|e| e.filter_map(|f| f.ok()).filter(|f| {
            f.path().extension().map_or(false, |ext| ext == "md")
        }).count())
        .unwrap_or(0);
    let has_memory = data_dir.join("MEMORY.md").exists();
    let has_instructions = data_dir.join("brain").join("INSTRUCTIONS.md").exists()
        || data_dir.join("INSTRUCTIONS.md").exists();

    let body = format!(
        r#"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Memory</h1>
                        <span class="badge badge-info">{chunk_count} chunks</span>
                    </div>
                </div>

                <div class="stats-grid stats-grid--3">
                    <div class="stat-card">
                        <div class="stat-label">Memory Chunks</div>
                        <div class="stat-value">{chunk_count}</div>
                        <div class="stat-sub">in vector store</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-label">Daily Logs</div>
                        <div class="stat-value">{daily_count}</div>
                        <div class="stat-sub">conversation logs</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-label">Files</div>
                        <div class="stat-value">{file_count}</div>
                        <div class="stat-sub">{file_detail}</div>
                    </div>
                </div>

                <section class="section">
                    <h2>Search Memory</h2>
                    <div class="memory-search">
                        <input type="text" id="memory-search-input" class="input" placeholder="Search memory chunks (FTS5)…" autocomplete="off">
                    </div>
                    <div class="item-list" id="search-results" style="display:none"></div>
                </section>

                <section class="section">
                    <h2>Long-term Memory</h2>
                    <div class="memory-editor">
                        <textarea id="memory-textarea" class="input memory-textarea" placeholder="MEMORY.md content…" spellcheck="false"></textarea>
                        <div class="memory-editor-actions">
                            <button class="btn btn-primary btn-sm" id="btn-save-memory">Save</button>
                            <button class="btn btn-secondary btn-sm" id="btn-reload-memory">Reload</button>
                            <span class="form-hint" id="memory-status"></span>
                        </div>
                    </div>
                </section>

                <section class="section">
                    <div class="section-header" style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px">
                        <h2 style="margin: 0; cursor: pointer" id="instructions-header">
                            <span class="collapse-icon" id="instructions-collapse-icon">▼</span>
                            Instructions
                            <span class="badge badge-neutral" id="instructions-count"></span>
                        </h2>
                        <div>
                            <button class="btn btn-secondary btn-sm" id="btn-deduplicate-instructions" title="Remove duplicate/similar instructions">Deduplicate</button>
                        </div>
                    </div>
                    <div id="instructions-wrapper">
                        <div id="instructions-list" class="item-list"></div>
                        <div class="inline-form" style="margin-top: 12px">
                            <input type="text" id="instruction-input" class="input flex-grow" placeholder="Add a new instruction…">
                            <button class="btn btn-primary btn-sm" id="btn-add-instruction">Add</button>
                        </div>
                    </div>
                </section>

                <section class="section">
                    <div class="section-header" style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px">
                        <h2 style="margin: 0; cursor: pointer" id="history-header">
                            <span class="collapse-icon" id="history-collapse-icon">▼</span>
                            Conversation History
                            <span class="badge badge-neutral" id="history-count"></span>
                        </h2>
                    </div>
                    <div id="history-wrapper">
                        <div class="item-list" id="history-list"></div>
                        <button class="btn btn-secondary btn-sm" id="btn-load-more" style="margin-top: 12px; display: none">Load more</button>
                    </div>
                </section>

                <section class="section">
                    <div class="section-header" style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px">
                        <h2 style="margin: 0; cursor: pointer" id="daily-header">
                            <span class="collapse-icon" id="daily-collapse-icon">▼</span>
                            Daily Logs
                            <span class="badge badge-neutral" id="daily-count"></span>
                        </h2>
                    </div>
                    <div id="daily-wrapper">
                        <div id="daily-list" class="daily-list"></div>
                        <div id="daily-content" style="display:none">
                            <div class="daily-header">
                                <button class="btn btn-ghost btn-sm" id="btn-daily-back">← Back</button>
                                <span class="badge badge-neutral" id="daily-date-badge"></span>
                            </div>
                            <pre class="daily-viewer" id="daily-viewer"></pre>
                        </div>
                    </div>
                </section>

                <div id="memory-toast" class="skill-toast" style="display:none"></div>
            </div>
        </main>"#,
        chunk_count = chunk_count,
        daily_count = daily_count,
        file_count = [has_memory, has_instructions].iter().filter(|&&v| v).count(),
        file_detail = {
            let mut parts = Vec::new();
            if has_memory { parts.push("MEMORY.md"); }
            if has_instructions { parts.push("INSTRUCTIONS.md"); }
            if parts.is_empty() { "no files yet".to_string() } else { parts.join(" + ") }
        },
    );

    Html(page_html("Memory", "memory", &body, &["memory.js"]))
}

// ─── Vault ───────────────────────────────────────────────────────

async fn vault_page() -> Html<String> {
    let body = r#"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Vault</h1>
                        <span class="badge badge-neutral" id="vault-count">Loading…</span>
                    </div>
                </div>

                <div class="vault-notice">
                    <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" style="width:20px;height:20px;flex-shrink:0">
                        <rect x="2" y="5" width="14" height="11" rx="1.5"/>
                        <path d="M5 5V4a4 4 0 0 1 8 0v1"/>
                        <circle cx="9" cy="11" r="1.5"/>
                        <path d="M9 12.5V14"/>
                    </svg>
                    <div>
                        <strong>Encrypted Storage</strong><br>
                        Secrets are encrypted with AES-256-GCM using a master key stored in your OS keychain.
                        Values are never included in server-rendered pages — they are decrypted on-demand via POST requests.
                    </div>
                </div>

                <!-- 2FA Section -->
                <section class="section">
                    <div class="section-header">
                        <h2>Two-Factor Authentication</h2>
                        <span class="badge" id="twofa-status-badge">Checking…</span>
                    </div>
                    <div id="twofa-disabled-view">
                        <p style="color:var(--muted);margin-bottom:1rem">Require authenticator code to reveal secrets. Recommended for enhanced security.</p>
                        <button class="btn btn-primary btn-sm" id="btn-enable-2fa">Enable 2FA</button>
                    </div>
                    <div id="twofa-enabled-view" style="display:none">
                        <p style="margin-bottom:0.5rem">2FA is <strong>enabled</strong>. You'll need your authenticator app to reveal secrets.</p>
                        <p style="color:var(--muted);font-size:0.875rem;margin-bottom:1rem">
                            Session timeout: <span id="twofa-timeout">5 minutes</span> ·
                            Recovery codes: <span id="twofa-recovery-count">0</span> remaining
                        </p>
                        <div class="actions">
                            <button class="btn btn-secondary btn-sm" id="btn-view-recovery">View Recovery Codes</button>
                            <button class="btn btn-danger btn-sm" id="btn-disable-2fa">Disable 2FA</button>
                        </div>
                    </div>
                </section>

                <section class="section">
                    <h2>Store Secret</h2>
                    <form id="vault-form" class="form">
                        <div class="form-row form-row--2">
                            <div class="form-group">
                                <label>Key</label>
                                <input type="text" id="vault-key" class="input" placeholder="my_api_key" pattern="[a-z0-9_]+">
                                <div class="form-hint">Lowercase letters, numbers, underscores only</div>
                            </div>
                            <div class="form-group">
                                <label>Value</label>
                                <input type="password" id="vault-value" class="input" placeholder="secret value…">
                            </div>
                        </div>
                        <button type="submit" class="btn btn-primary btn-sm">Store Secret</button>
                    </form>
                </section>

                <section class="section">
                    <h2>Stored Secrets</h2>
                    <div class="item-list" id="vault-list">
                        <div class="empty-state" id="vault-empty">
                            <p>Loading secrets…</p>
                        </div>
                    </div>
                </section>

                <!-- 2FA Setup Modal -->
                <div id="twofa-setup-modal" class="modal">
                    <div class="modal-backdrop"></div>
                    <div class="modal-content">
                        <div class="modal-header">
                            <h3 class="modal-title">Setup Authenticator</h3>
                            <button class="modal-close" type="button">&times;</button>
                        </div>
                        <div class="modal-body">
                            <p style="margin-bottom:1rem">Scan this QR code with your authenticator app (Google Authenticator, Authy, 1Password, etc.)</p>
                            <div style="text-align:center;margin-bottom:1rem">
                                <img id="twofa-qr-image" src="" alt="QR Code" style="max-width:200px;border-radius:8px">
                            </div>
                            <div class="form-group">
                                <label>Or enter this code manually:</label>
                                <code id="twofa-secret" style="display:block;padding:0.5rem;background:var(--surface);border-radius:4px;font-size:0.875rem;word-break:break-all"></code>
                            </div>
                            <div class="form-group">
                                <label>Enter the 6-digit code from your app:</label>
                                <input type="text" id="twofa-setup-code" class="input" placeholder="000000" maxlength="6" pattern="[0-9]{6}" style="text-align:center;font-size:1.25rem;letter-spacing:0.5em">
                            </div>
                            <div class="modal-actions">
                                <button class="btn btn-secondary" id="btn-cancel-twofa-setup">Cancel</button>
                                <button class="btn btn-primary" id="btn-confirm-twofa-setup">Verify & Enable</button>
                            </div>
                        </div>
                    </div>
                </div>

                <!-- 2FA Code Modal (for reveal) -->
                <div id="twofa-code-modal" class="modal">
                    <div class="modal-backdrop"></div>
                    <div class="modal-content" style="max-width:360px">
                        <div class="modal-header">
                            <h3 class="modal-title">Authentication Required</h3>
                            <button class="modal-close" type="button">&times;</button>
                        </div>
                        <div class="modal-body">
                            <p style="margin-bottom:1rem;color:var(--muted)">Enter the code from your authenticator app to reveal this secret.</p>
                            <div class="form-group">
                                <input type="text" id="twofa-verify-code" class="input" placeholder="000000" maxlength="6" pattern="[0-9]{6}" style="text-align:center;font-size:1.25rem;letter-spacing:0.5em" autofocus>
                            </div>
                            <div class="modal-actions">
                                <button class="btn btn-secondary" id="btn-cancel-twofa-verify">Cancel</button>
                                <button class="btn btn-primary" id="btn-submit-twofa-verify">Verify</button>
                            </div>
                        </div>
                    </div>
                </div>

                <!-- Recovery Codes Modal -->
                <div id="recovery-modal" class="modal">
                    <div class="modal-backdrop"></div>
                    <div class="modal-content">
                        <div class="modal-header">
                            <h3 class="modal-title">Recovery Codes</h3>
                            <button class="modal-close" type="button">&times;</button>
                        </div>
                        <div class="modal-body">
                            <p style="margin-bottom:1rem;color:var(--muted)">Enter your authenticator code to view recovery codes.</p>
                            <div id="recovery-codes-list" style="display:none">
                                <p style="margin-bottom:0.5rem"><strong>Store these codes securely. Each can only be used once.</strong></p>
                                <div id="recovery-codes-grid" style="display:grid;grid-template-columns:1fr 1fr;gap:0.5rem;font-family:monospace"></div>
                            </div>
                            <div id="recovery-auth-section">
                                <div class="form-group">
                                    <input type="text" id="recovery-auth-code" class="input" placeholder="000000" maxlength="6" pattern="[0-9]{6}" style="text-align:center;font-size:1.25rem;letter-spacing:0.5em">
                                </div>
                            </div>
                            <div class="modal-actions">
                                <button class="btn btn-secondary" id="btn-close-recovery">Close</button>
                                <button class="btn btn-primary" id="btn-show-recovery" style="display:none">Copy All</button>
                            </div>
                        </div>
                    </div>
                </div>

                <!-- Reveal Modal -->
                <div id="reveal-modal" class="modal">
                    <div class="modal-backdrop"></div>
                    <div class="modal-content">
                        <div class="modal-header">
                            <h3 class="modal-title">Reveal Secret</h3>
                            <button class="modal-close" type="button">&times;</button>
                        </div>
                        <div class="modal-body">
                            <div class="form-group">
                                <label id="reveal-key-label">Key</label>
                                <div class="vault-reveal-value" id="reveal-value">Decrypting…</div>
                            </div>
                            <div class="vault-reveal-timer" id="reveal-timer">Auto-hide in 10s</div>
                            <div class="modal-actions">
                                <button class="btn btn-secondary" id="btn-copy-secret">Copy</button>
                                <button class="btn btn-primary" id="btn-close-reveal">Close</button>
                            </div>
                        </div>
                    </div>
                </div>

                <div id="vault-toast" class="skill-toast" style="display:none"></div>
            </div>
        </main>"#;

    Html(page_html("Vault", "vault", body, &["vault.js"]))
}

// ─── Permissions ─────────────────────────────────────────────────

async fn permissions_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let config = state.config.read().await;
    let mode = match config.permissions.mode {
        crate::config::PermissionMode::Open => "open",
        crate::config::PermissionMode::Workspace => "workspace",
        crate::config::PermissionMode::Acl => "acl",
    };
    let acl_count = config.permissions.acl.len();

    let body = format!(r#"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Permissions</h1>
                        <span class="badge badge-info">{acl_count} ACL rules</span>
                    </div>
                </div>

                <div class="permissions-notice">
                    <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" style="width:20px;height:20px;flex-shrink:0">
                        <rect x="1" y="4" width="16" height="12" rx="1.5"/>
                        <circle cx="9" cy="10" r="2"/>
                        <path d="M5 4V3a4 4 0 0 1 8 0v1"/>
                    </svg>
                    <div>
                        <strong>Permission Mode</strong><br>
                        Control what files and directories the agent can access. Changes take effect immediately.
                    </div>
                </div>

                <section class="section">
                    <h2>Permission Mode</h2>
                    <div class="permission-mode-grid">
                        <div class="permission-mode-card" data-mode="open">
                            <div class="permission-mode-header">
                                <span class="permission-mode-name">Open</span>
                                <span class="badge badge-neutral">Not Recommended</span>
                            </div>
                            <div class="permission-mode-desc">Agent can access any file (except hardcoded blocks like ~/.ssh)</div>
                        </div>
                        <div class="permission-mode-card" data-mode="workspace">
                            <div class="permission-mode-header">
                                <span class="permission-mode-name">Workspace</span>
                                <span class="badge badge-success">Default</span>
                            </div>
                            <div class="permission-mode-desc">Agent can access workspace + brain + memory directories</div>
                        </div>
                        <div class="permission-mode-card" data-mode="acl">
                            <div class="permission-mode-header">
                                <span class="permission-mode-name">ACL</span>
                                <span class="badge badge-info">Advanced</span>
                            </div>
                            <div class="permission-mode-desc">Full ACL-based control with per-path permissions</div>
                        </div>
                    </div>
                    <input type="hidden" id="current-mode" value="{mode}">
                </section>

                <section class="section">
                    <h2>Default Permissions</h2>
                    <p class="form-hint">These apply when no ACL rule matches a path.</p>
                    <div class="permission-defaults" id="permission-defaults">
                        <div class="perm-checkbox-group">
                            <label class="checkbox-label">
                                <input type="checkbox" id="default-read" {default_read_checked}>
                                <span>Read</span>
                            </label>
                            <label class="checkbox-label">
                                <input type="checkbox" id="default-write" {default_write_checked}>
                                <span>Write</span>
                            </label>
                            <label class="checkbox-label">
                                <input type="checkbox" id="default-delete" {default_delete_checked}>
                                <span>Delete</span>
                            </label>
                        </div>
                    </div>
                </section>

                <section class="section">
                    <h2>ACL Rules</h2>
                    <p class="form-hint">Rules are evaluated in order. First match wins. Built-in rules protect sensitive paths.</p>
                    
                    <div class="acl-actions">
                        <button class="btn btn-primary btn-sm" id="btn-add-acl">Add Rule</button>
                    </div>

                    <div class="acl-list" id="acl-list">
                        <div class="acl-loading">Loading ACL rules...</div>
                    </div>
                </section>

                <section class="section">
                    <h2>Shell Permissions</h2>
                    <p class="form-hint">OS-specific command restrictions for the shell tool.</p>
                    
                    <div class="shell-tabs">
                        <button class="shell-tab active" data-os="macos">macOS</button>
                        <button class="shell-tab" data-os="linux">Linux</button>
                        <button class="shell-tab" data-os="windows">Windows</button>
                    </div>

                    <div class="shell-profile-content" id="shell-profile-content">
                        <div class="form-group">
                            <label>Shell</label>
                            <select id="shell-select" class="input">
                                <option value="">Default (sh)</option>
                                <option value="bash">bash</option>
                                <option value="zsh">zsh</option>
                                <option value="powershell">PowerShell</option>
                                <option value="cmd">cmd</option>
                            </select>
                        </div>
                        <div class="form-group">
                            <label class="checkbox-label">
                                <input type="checkbox" id="allow-risky">
                                <span>Allow Risky Commands</span>
                            </label>
                            <div class="form-hint">Package removal, process killing, etc.</div>
                        </div>
                        <div class="form-group">
                            <label>Blocked Commands (one per line)</label>
                            <textarea id="blocked-commands" class="input" rows="3" placeholder="launchctl load&#10;defaults delete"></textarea>
                        </div>
                        <div class="form-group">
                            <label>Allowed Commands Whitelist (optional, one per line)</label>
                            <textarea id="allowed-commands" class="input" rows="3" placeholder="git&#10;npm&#10;cargo"></textarea>
                            <div class="form-hint">If non-empty, only these commands are allowed.</div>
                        </div>
                    </div>
                </section>

                <section class="section">
                    <h2>Quick Presets</h2>
                    <div class="preset-buttons">
                        <button class="btn btn-secondary" data-preset="developer">Developer</button>
                        <button class="btn btn-secondary" data-preset="restricted">Restricted</button>
                        <button class="btn btn-danger" data-preset="paranoid">Paranoid</button>
                    </div>
                </section>

                <section class="section">
                    <h2>Test Path</h2>
                    <p class="form-hint">Check if a path would be allowed for a specific operation.</p>
                    <div class="test-path-form">
                        <input type="text" id="test-path" class="input" placeholder="~/Projects/myfile.txt">
                        <select id="test-operation" class="input">
                            <option value="read">Read</option>
                            <option value="write">Write</option>
                            <option value="delete">Delete</option>
                        </select>
                        <button class="btn btn-primary" id="btn-test-path">Test</button>
                    </div>
                    <div id="test-result" class="test-result" style="display:none"></div>
                </section>

                <div id="permissions-toast" class="skill-toast" style="display:none"></div>
            </div>
        </main>

        <!-- ACL Edit Modal -->
        <div id="acl-modal" class="modal">
            <div class="modal-backdrop"></div>
            <div class="modal-content">
                <div class="modal-header">
                    <h3 class="modal-title" id="acl-modal-title">Add ACL Rule</h3>
                    <button class="modal-close acl-modal-close" type="button">&times;</button>
                </div>
                <div class="modal-body">
                    <form id="acl-form">
                        <div class="form-group">
                            <label>Path Pattern</label>
                            <div class="path-input-group">
                                <input type="text" id="acl-path" class="input" placeholder="~/Projects/**">
                                <button type="button" class="btn btn-secondary btn-sm" id="btn-browse-path">Browse</button>
                            </div>
                            <div class="form-hint">Glob patterns supported: ** (any depth), * (single segment), ? (single char)</div>
                        </div>
                        <div class="form-group">
                            <label>Type</label>
                            <select id="acl-type" class="input">
                                <option value="allow">Allow</option>
                                <option value="deny">Deny</option>
                            </select>
                        </div>
                        <div class="form-group">
                            <label>Permissions</label>
                            <div class="perm-checkbox-group">
                                <label class="checkbox-label">
                                    <input type="checkbox" id="acl-read" checked>
                                    <span>Read</span>
                                </label>
                                <label class="checkbox-label">
                                    <input type="checkbox" id="acl-write">
                                    <span>Write</span>
                                </label>
                                <label class="checkbox-label">
                                    <input type="checkbox" id="acl-delete">
                                    <span>Delete</span>
                                </label>
                            </div>
                        </div>
                        <div class="form-group">
                            <label>Confirmation Required</label>
                            <select id="acl-confirm" class="input">
                                <option value="none">None</option>
                                <option value="read">On Read</option>
                                <option value="write">On Write</option>
                                <option value="delete">On Delete</option>
                            </select>
                            <div class="form-hint">Agent will ask for confirmation before the operation.</div>
                        </div>
                        <div class="modal-actions">
                            <button type="button" class="btn btn-secondary acl-modal-cancel">Cancel</button>
                            <button type="submit" class="btn btn-primary">Save Rule</button>
                        </div>
                    </form>
                </div>
            </div>
        </div>

        <!-- Path Browser Modal -->
        <div id="path-browser-modal" class="modal">
            <div class="modal-backdrop"></div>
            <div class="modal-content modal-content--wide">
                <div class="modal-header">
                    <h3 class="modal-title">Browse Folders</h3>
                    <button class="modal-close path-browser-close" type="button">&times;</button>
                </div>
                <div class="modal-body">
                    <div class="path-browser-current">
                        <span id="browser-current-path">~</span>
                    </div>
                    <div class="path-browser-nav">
                        <button class="btn btn-sm btn-secondary" id="btn-browser-up">↑ Up</button>
                        <button class="btn btn-sm btn-secondary" id="btn-browser-home">🏠 Home</button>
                    </div>
                    <div class="path-browser-list" id="browser-list">
                        <div class="browser-loading">Loading...</div>
                    </div>
                    <div class="path-browser-selected">
                        <label>Selected Path:</label>
                        <input type="text" id="browser-selected-path" class="input" readonly>
                        <label class="checkbox-label">
                            <input type="checkbox" id="browser-recursive" checked>
                            <span>Include subdirectories (**)</span>
                        </label>
                    </div>
                </div>
                <div class="modal-actions">
                    <button type="button" class="btn btn-secondary path-browser-cancel">Cancel</button>
                    <button type="button" class="btn btn-primary" id="btn-select-path">Select Folder</button>
                </div>
            </div>
        </div>"#,
        mode = mode,
        acl_count = acl_count,
        default_read_checked = if config.permissions.default.read { "checked" } else { "" },
        default_write_checked = if config.permissions.default.write { "checked" } else { "" },
        default_delete_checked = if config.permissions.default.delete { "checked" } else { "" },
    );

    Html(page_html("Permissions", "permissions", &body, &["permissions.js"]))
}

// ─── Helpers ────────────────────────────────────────────────────

fn count_active_channels(config: &crate::config::Config) -> usize {
    let mut count = 1; // web is always active when this page loads
    if config.channels.telegram.enabled {
        count += 1;
    }
    if config.channels.discord.enabled {
        count += 1;
    }
    if config.channels.whatsapp.enabled {
        count += 1;
    }
    count
}

fn build_channels_html(config: &crate::config::Config) -> String {
    let channels = [
        (ICON_WEB, "Web UI", true, "Always on"),
        (
            ICON_TELEGRAM,
            "Telegram",
            config.channels.telegram.enabled,
            "Bot API",
        ),
        (
            ICON_DISCORD,
            "Discord",
            config.channels.discord.enabled,
            "Bot",
        ),
        (
            ICON_PHONE,
            "WhatsApp",
            config.channels.whatsapp.enabled,
            "Native",
        ),
    ];

    channels
        .iter()
        .map(|(icon, name, enabled, detail)| {
            let (badge_cls, status_text) = if *enabled {
                ("badge-success", "Connected")
            } else {
                ("badge-neutral", "Disabled")
            };
            format!(
                r#"<div class="item-row">
                    <div class="item-info">
                        <div class="item-icon">{icon}</div>
                        <div>
                            <div class="item-name">{name}</div>
                            <div class="item-detail">{detail}</div>
                        </div>
                    </div>
                    <span class="badge {badge_cls}">{status_text}</span>
                </div>"#,
            )
        })
        .collect()
}

fn build_providers_html(config: &crate::config::Config) -> String {
    /// Provider display metadata: (display_name, description, needs_api_key, needs_base_url)
    /// needs_base_url is true only for providers that REQUIRE a custom URL (vllm, custom)
    /// All cloud providers have fixed URLs and don't need user input
    fn get_provider_meta(name: &str) -> (&'static str, &'static str, bool, bool) {
        match name {
            // Primary providers (fixed URLs)
            "anthropic" => ("Anthropic", "Claude API (claude-3.5-sonnet, claude-opus, etc.)", true, false),
            "openai" => ("OpenAI", "GPT-4, GPT-4o, o1, o3 series", true, false),
            "openrouter" => ("OpenRouter", "Access to 200+ models via unified API", true, false),
            "gemini" => ("Google Gemini", "Gemini 1.5 Pro, Gemini 2.0 Flash", true, false),
            // Local/cloud providers
            "ollama" => ("Ollama (local)", "Run models locally (llama3, mistral, etc.)", false, true),
            "ollama_cloud" => ("Ollama Cloud", "Hosted Ollama models with API key", true, false),
            "vllm" => ("vLLM", "Self-hosted vLLM server", false, true),
            "custom" => ("Custom", "Any OpenAI-compatible API endpoint", false, true),
            // Cloud providers (all have fixed URLs)
            "deepseek" => ("DeepSeek", "DeepSeek V3, DeepSeek R1, Coder", true, false),
            "groq" => ("Groq", "Ultra-fast inference (llama, mixtral)", true, false),
            "mistral" => ("Mistral", "Mistral and Mixtral models", true, false),
            "xai" => ("xAI (Grok)", "Grok models by xAI", true, false),
            "together" => ("Together AI", "Open-source models at scale", true, false),
            "fireworks" => ("Fireworks AI", "Fast serverless inference", true, false),
            "perplexity" => ("Perplexity", "Sonar models with web search", true, false),
            "cohere" => ("Cohere", "Command R+, Command models", true, false),
            "venice" => ("Venice", "Privacy-focused AI inference", true, false),
            // Gateways/aggregators (fixed URLs)
            "aihubmix" => ("AiHubMix", "Multi-model aggregator gateway", true, false),
            "vercel" => ("Vercel AI", "Vercel AI Gateway", true, false),
            "cloudflare" => ("Cloudflare AI", "Cloudflare AI Gateway", true, false),
            "copilot" => ("GitHub Copilot", "GitHub Copilot API", true, false),
            "bedrock" => ("AWS Bedrock", "Amazon Bedrock foundation models", true, false),
            // Chinese providers (fixed URLs)
            "minimax" => ("MiniMax", "MiniMax AI models", true, false),
            "dashscope" => ("DashScope", "Alibaba Qwen models", true, false),
            "moonshot" => ("Moonshot (Kimi)", "Moonshot AI models", true, false),
            "zhipu" => ("Zhipu AI (GLM)", "GLM models by Zhipu", true, false),
            _ => ("Unknown", "Unknown provider", true, false),
        }
    }

    config
        .providers
        .iter()
        .map(|(name, pc)| {
            let configured = config.is_provider_configured(name);

            let (display_name, description, has_key, has_url) = get_provider_meta(name);
            let is_ollama = name == "ollama";

            // Build CSS class list for the card
            let mut card_classes = String::from("provider-card");
            if configured {
                card_classes.push_str(" is-configured");
            }

            // API key mask — check encrypted storage first, then plaintext
            let api_key_mask = if configured && has_key {
                // Don't leak actual key content; just show it's present
                "••••••••".to_string()
            } else {
                String::new()
            };

            // Toggle is checked when provider is configured
            let toggle_checked = if configured { "checked" } else { "" };

            format!(
                r#"<div class="{card_classes}" data-provider="{name}" data-display="{display_name}" data-description="{description}" data-has-key="{has_key}" data-has-url="{has_url}" data-is-ollama="{is_ollama}" data-configured="{configured}" data-api-key-mask="{api_key_mask}" data-api-base="{api_base}">
                    <div class="provider-card-header">
                        <div class="provider-card-info">
                            <span class="provider-card-name">{display_name}</span>
                        </div>
                        <div class="provider-card-actions">
                            <div class="toggle-wrap">
                                <input type="checkbox" class="toggle-input" id="toggle-{name}" {toggle_checked}>
                                <label class="toggle-label" for="toggle-{name}"></label>
                            </div>
                        </div>
                    </div>
                    <div class="provider-card-desc">{description}</div>
                </div>"#,
                api_base = pc.api_base.as_deref().unwrap_or(""),
            )
        })
        .collect()
}

fn build_channels_cards_html(config: &crate::config::Config) -> String {
    struct ChannelMeta {
        name: &'static str,
        display: &'static str,
        desc: &'static str,
        icon: &'static str,
        has_token: bool,
    }

    let channels = [
        ChannelMeta {
            name: "telegram",
            display: "Telegram",
            desc: "Send and receive messages via Telegram bot",
            icon: ICON_TELEGRAM,
            has_token: true,
        },
        ChannelMeta {
            name: "discord",
            display: "Discord",
            desc: "Discord bot integration",
            icon: ICON_DISCORD,
            has_token: true,
        },
        ChannelMeta {
            name: "whatsapp",
            display: "WhatsApp",
            desc: "Native WhatsApp Web client (no bridge needed)",
            icon: ICON_PHONE,
            has_token: false,
        },
        ChannelMeta {
            name: "web",
            display: "Web UI",
            desc: "Browser-based chat interface",
            icon: ICON_WEB,
            has_token: false,
        },
    ];

    channels
        .iter()
        .map(|ch| {
            let configured = config.is_channel_configured(ch.name);
            let enabled = match ch.name {
                "telegram" => config.channels.telegram.enabled,
                "discord" => config.channels.discord.enabled,
                "whatsapp" => config.channels.whatsapp.enabled,
                "web" => config.channels.web.enabled,
                _ => false,
            };
            let is_web = ch.name == "web";

            let mut classes = String::from("provider-card channel-card");
            if configured || is_web {
                classes.push_str(" is-configured");
            }
            if enabled || is_web {
                classes.push_str(" is-active");
            }

            let toggle_checked = if enabled || is_web { "checked" } else { "" };
            let toggle_disabled = if is_web { "disabled" } else { "" };

            // Badge: "Active" for enabled channels
            let badge = if enabled || is_web {
                r#"<span class="provider-default-badge">Active</span>"#
            } else {
                ""
            };

            // Channel-specific data attributes (resolve encrypted tokens)
            let token_mask = match ch.name {
                "telegram" if configured => resolve_and_mask_token("telegram", &config.channels.telegram.token),
                "discord" if configured => resolve_and_mask_token("discord", &config.channels.discord.token),
                _ => String::new(),
            };
            let allow_from = match ch.name {
                "telegram" => config.channels.telegram.allow_from.join(","),
                "discord" => config.channels.discord.allow_from.join(","),
                "whatsapp" => config.channels.whatsapp.allow_from.join(","),
                _ => String::new(),
            };
            let phone = &config.channels.whatsapp.phone_number;
            let discord_channel = &config.channels.discord.default_channel_id;
            let web_host = &config.channels.web.host;
            let web_port = config.channels.web.port;

            format!(
                r##"<div class="{classes}" data-channel="{name}" data-display="{display}" data-configured="{configured}" data-enabled="{enabled}" data-has-token="{has_token}" data-token-mask="{token_mask}" data-allow-from="{allow_from}" data-phone="{phone}" data-discord-channel="{discord_channel}" data-web-host="{web_host}" data-web-port="{web_port}" data-is-web="{is_web}">
                    <div class="provider-card-header">
                        <div class="provider-card-info">
                            <span class="channel-icon">{icon}</span>
                            <span class="provider-card-name">{display}</span>
                        </div>
                        <div class="provider-card-actions">
                            {badge}
                            <div class="toggle-wrap">
                                <input type="checkbox" class="toggle-input" id="toggle-ch-{name}" {toggle_checked} {toggle_disabled}>
                                <label class="toggle-label" for="toggle-ch-{name}"></label>
                            </div>
                        </div>
                    </div>
                    <div class="provider-card-desc">{desc}</div>
                </div>"##,
                name = ch.name,
                display = ch.display,
                desc = ch.desc,
                icon = ch.icon,
                has_token = ch.has_token,
            )
        })
        .collect()
}

fn mask_token(token: &str) -> String {
    if token.is_empty() {
        "Not configured".to_string()
    } else if token.len() > 8 {
        format!("{}•••••", &token[..6])
    } else {
        "•••••".to_string()
    }
}

/// Resolve encrypted token and mask it for display.
fn resolve_and_mask_token(channel_name: &str, toml_value: &str) -> String {
    if toml_value == "***ENCRYPTED***" {
        // Read real token from encrypted storage
        if let Ok(secrets) = crate::storage::global_secrets() {
            let key = crate::storage::SecretKey::channel_token(channel_name);
            if let Ok(Some(real_token)) = secrets.get(&key) {
                return mask_token(&real_token);
            }
        }
        "Encrypted ••••".to_string()
    } else {
        mask_token(toml_value)
    }
}

fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

// ─── Account Page ─────────────────────────────────────────────────

async fn account_page(State(_state): State<Arc<AppState>>) -> Html<String> {
    let body = r#"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Account</h1>
                        <span class="badge badge-neutral" id="account-status">Loading…</span>
                    </div>
                </div>

                <!-- Owner Info Section -->
                <section class="section">
                    <div class="section-header">
                        <h2>Owner</h2>
                    </div>
                    <div class="account-owner-card" id="owner-card">
                        <div class="owner-avatar">
                            <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" style="width:32px;height:32px">
                                <circle cx="9" cy="6" r="3.5"/>
                                <path d="M3 17c0-3.5 2.5-6 6-6s6 2.5 6 6"/>
                            </svg>
                        </div>
                        <div class="owner-info">
                            <div class="owner-username" id="owner-username">—</div>
                            <div class="owner-role" id="owner-role">—</div>
                        </div>
                    </div>
                    <div id="no-owner-warning" style="display:none" class="empty-state">
                        <p>No owner configured. Create one with: <code>homun users add &lt;username&gt; --admin</code></p>
                    </div>
                </section>

                <!-- Channel Identities Section -->
                <section class="section">
                    <div class="section-header">
                        <h2>Channel Identities</h2>
                        <span class="badge" id="identities-count">0</span>
                    </div>
                    <p style="color:var(--muted);margin-bottom:1rem;font-size:0.875rem">
                        Link your Telegram, Discord, or WhatsApp account to identify yourself as the owner.
                    </p>
                    <div class="item-list" id="identities-list">
                        <div class="empty-state" id="identities-empty">
                            <p>No identities linked</p>
                        </div>
                    </div>

                    <!-- Add Identity Form -->
                    <details class="details-collapse" style="margin-top:1rem">
                        <summary class="btn btn-secondary btn-sm">+ Link Identity</summary>
                        <form id="link-identity-form" class="form" style="margin-top:1rem">
                            <div class="form-row form-row--3">
                                <div class="form-group">
                                    <label>Channel</label>
                                    <select id="identity-channel" class="input">
                                        <option value="telegram">Telegram</option>
                                        <option value="discord">Discord</option>
                                        <option value="whatsapp">WhatsApp</option>
                                        <option value="web">Web</option>
                                    </select>
                                </div>
                                <div class="form-group">
                                    <label>Platform ID</label>
                                    <input type="text" id="identity-platform-id" class="input" placeholder="e.g., 123456789">
                                </div>
                                <div class="form-group">
                                    <label>Display Name (optional)</label>
                                    <input type="text" id="identity-display-name" class="input" placeholder="My Account">
                                </div>
                            </div>
                            <button type="submit" class="btn btn-primary btn-sm">Link</button>
                        </form>
                    </details>
                </section>

                <!-- Webhook Tokens Section -->
                <section class="section">
                    <div class="section-header">
                        <h2>Webhook Tokens</h2>
                        <span class="badge" id="tokens-count">0</span>
                    </div>
                    <p style="color:var(--muted);margin-bottom:1rem;font-size:0.875rem">
                        Create tokens to allow external services to send messages to your assistant.
                    </p>
                    <div class="item-list" id="tokens-list">
                        <div class="empty-state" id="tokens-empty">
                            <p>No webhook tokens</p>
                        </div>
                    </div>

                    <!-- Create Token Form -->
                    <details class="details-collapse" style="margin-top:1rem">
                        <summary class="btn btn-secondary btn-sm">+ Create Token</summary>
                        <form id="create-token-form" class="form" style="margin-top:1rem">
                            <div class="form-row form-row--2">
                                <div class="form-group">
                                    <label>Token Name</label>
                                    <input type="text" id="token-name" class="input" placeholder="e.g., Home Assistant">
                                </div>
                                <div class="form-group">
                                    <label>Webhook URL (after creation)</label>
                                    <input type="text" id="webhook-url-preview" class="input" readonly value="POST /api/v1/webhook/{token}">
                                </div>
                            </div>
                            <button type="submit" class="btn btn-primary btn-sm">Create Token</button>
                        </form>
                    </details>
                </section>

            </div>
        </main>"#;

    let html = page_html("Account", "account", body, &["account.js"]);
    Html(html)
}
