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
        .route("/automations", get(automations_page))
        .route("/skills", get(skills_page))
        .route("/memory", get(memory_page))
        .route("/vault", get(vault_page))
        .route("/permissions", get(permissions_page))
        .route("/approvals", get(approvals_page))
        .route("/account", get(account_page))
        .route("/logs", get(logs_page))
}

// ─── Shared layout pieces ───────────────────────────────────────

/// SVG icons used in the sidebar nav
const ICON_DASHBOARD: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="1" y="1" width="7" height="7" rx="1.5"/><rect x="10" y="1" width="7" height="4" rx="1.5"/><rect x="1" y="10" width="7" height="4" rx="1.5" transform="translate(0,3)"/><rect x="10" y="7" width="7" height="7" rx="1.5" transform="translate(0,3)"/></svg>"#;
const ICON_CHAT: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12.5V3.5A1.5 1.5 0 0 1 3.5 2h11A1.5 1.5 0 0 1 16 3.5v7a1.5 1.5 0 0 1-1.5 1.5H6L2 16V12.5z"/><line x1="6" y1="6" x2="12" y2="6"/><line x1="6" y1="9" x2="10" y2="9"/></svg>"#;
const ICON_AUTOMATIONS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="9" r="6.5"/><path d="M9 5.5v4l2.8 1.8"/><path d="M9 1v1.5M9 15.5V17M1 9h1.5M15.5 9H17"/></svg>"#;
const ICON_SKILLS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 1L11.5 6.5 17 7.5 13 11.5 14 17 9 14.5 4 17 5 11.5 1 7.5 6.5 6.5z"/></svg>"#;
const ICON_SETTINGS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="9" r="2.5"/><path d="M14.7 11.1a1.2 1.2 0 0 0 .24 1.32l.04.04a1.44 1.44 0 1 1-2.04 2.04l-.04-.04a1.2 1.2 0 0 0-1.32-.24 1.2 1.2 0 0 0-.72 1.08v.12a1.44 1.44 0 0 1-2.88 0v-.06a1.2 1.2 0 0 0-.78-1.08 1.2 1.2 0 0 0-1.32.24l-.04.04a1.44 1.44 0 1 1-2.04-2.04l.04-.04a1.2 1.2 0 0 0 .24-1.32 1.2 1.2 0 0 0-1.08-.72h-.12a1.44 1.44 0 0 1 0-2.88h.06a1.2 1.2 0 0 0 1.08-.78 1.2 1.2 0 0 0-.24-1.32l-.04-.04a1.44 1.44 0 1 1 2.04-2.04l.04.04a1.2 1.2 0 0 0 1.32.24h.06a1.2 1.2 0 0 0 .72-1.08V2.88a1.44 1.44 0 0 1 2.88 0v.06a1.2 1.2 0 0 0 .72 1.08 1.2 1.2 0 0 0 1.32-.24l.04-.04a1.44 1.44 0 1 1 2.04 2.04l-.04.04a1.2 1.2 0 0 0-.24 1.32v.06a1.2 1.2 0 0 0 1.08.72h.12a1.44 1.44 0 0 1 0 2.88h-.06a1.2 1.2 0 0 0-1.08.72z"/></svg>"#;
const ICON_LOGS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 15V3a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v12"/><line x1="6" y1="6" x2="12" y2="6"/><line x1="6" y1="9" x2="12" y2="9"/><line x1="6" y1="12" x2="9" y2="12"/></svg>"#;
const ICON_MEMORY: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 2v14"/><path d="M3 9h12"/><circle cx="9" cy="9" r="3"/><circle cx="9" cy="9" r="7"/></svg>"#;
const ICON_VAULT: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="5" width="14" height="11" rx="1.5"/><path d="M5 5V4a4 4 0 0 1 8 0v1"/><circle cx="9" cy="11" r="1.5"/><path d="M9 12.5V14"/></svg>"#;
const ICON_PERMISSIONS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="1" y="4" width="16" height="12" rx="1.5"/><circle cx="9" cy="10" r="2"/><path d="M5 4V3a4 4 0 0 1 8 0v1"/></svg>"#;
const ICON_APPROVALS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 1v4M9 13v4M1 9h4M13 9h4"/><circle cx="9" cy="9" r="3"/><path d="M6 9l2 2 4-4"/></svg>"#;
const ICON_ACCOUNT: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="6" r="3.5"/><path d="M3 17c0-3.5 2.5-6 6-6s6 2.5 6 6"/></svg>"#;

/// Channel icons — minimal stroke SVGs for dashboard/settings
const ICON_WEB: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="9" r="7.5"/><path d="M1.5 9h15"/><path d="M9 1.5a11.5 11.5 0 0 1 3 7.5 11.5 11.5 0 0 1-3 7.5"/><path d="M9 1.5a11.5 11.5 0 0 0-3 7.5 11.5 11.5 0 0 0 3 7.5"/></svg>"#;
const ICON_TELEGRAM: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M15.5 2.5L1.5 8l5 2m9-7.5L6.5 10m9-7.5l-3 13-5.5-5.5"/><path d="M6.5 10v4.5l2.5-2.5"/></svg>"#;
const ICON_DISCORD: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6.5 3C5 3 3 3.5 2 5c-1.5 3-.5 7.5 1 9.5.5.5 1.5 1.5 3 1.5s2-1 3-1 1.5 1 3 1 2.5-1 3-1.5c1.5-2 2.5-6.5 1-9.5-1-1.5-3-2-4.5-2"/><circle cx="6.5" cy="10" r="1"/><circle cx="11.5" cy="10" r="1"/></svg>"#;
const ICON_SLACK: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0z"/><path d="M9 6a1.5 1.5 0 1 1 0-3 1.5 1.5 0 0 1 0 3z"/><path d="M15 9a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0z"/><path d="M9 15a1.5 1.5 0 1 1 0-3 1.5 1.5 0 0 1 0 3z"/><path d="M6 6v3m0 3v3"/><path d="M12 6v3m0 3v3"/><path d="M6 6h3m3 0h3"/><path d="M6 12h3m3 0h3"/></svg>"#;
const ICON_PHONE: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="1" width="10" height="16" rx="2"/><line x1="9" y1="14" x2="9" y2="14"/></svg>"#;
const ICON_EMAIL: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="1" y="3" width="16" height="12" rx="2"/><path d="M1 5l8 5 8-5"/></svg>"#;

/// Logo icon — serves the SVG logotype via <img> tag.
const LOGO_ICON: &str = r#"<div class="logo-icon" title="HOMUN"></div>"#;

/// Build the sidebar navigation HTML
fn sidebar(active: &str) -> String {
    // Settings submenu (only visible when settings is active)
    let settings_submenu = if active == "settings" {
        r##"<div class="nav-submenu">
            <a href="#section-providers" class="nav-submenu-link">Model &amp; Providers</a>
            <a href="#section-channels" class="nav-submenu-link">Channels</a>
            <a href="#section-browser" class="nav-submenu-link">Browser</a>
            <a href="#section-memory" class="nav-submenu-link">Memory</a>
            <a href="#section-theme" class="nav-submenu-link">Theme</a>
        </div>"##
    } else {
        ""
    };

    let links = format!(
        r##"<div class="nav-group nav-group-featured">
            <a href="/chat" class="nav-link{chat_active}">
                <span class="nav-icon">{icon_chat}</span>
                <span class="nav-label">Chat</span>
            </a>
        </div>
        <div class="nav-section">Main</div>
        <a href="/" class="nav-link{dash_active}">
            <span class="nav-icon">{icon_dash}</span>
            <span class="nav-label">Dashboard</span>
        </a>
        <a href="/automations" class="nav-link{automations_active}">
            <span class="nav-icon">{icon_automations}</span>
            <span class="nav-label">Automations</span>
        </a>
        <a href="/skills" class="nav-link{skills_active}">
            <span class="nav-icon">{icon_skills}</span>
            <span class="nav-label">Skills</span>
        </a>
        <a href="/memory" class="nav-link{memory_active}">
            <span class="nav-icon">{icon_memory}</span>
            <span class="nav-label">Memory</span>
        </a>
        <a href="/vault" class="nav-link{vault_active}">
            <span class="nav-icon">{icon_vault}</span>
            <span class="nav-label">Vault</span>
        </a>
        <a href="/permissions" class="nav-link{perms_active}">
            <span class="nav-icon">{icon_perms}</span>
            <span class="nav-label">Permissions</span>
        </a>
        <a href="/approvals" class="nav-link{approvals_active}">
            <span class="nav-icon">{icon_approvals}</span>
            <span class="nav-label">Approvals</span>
        </a>
        <a href="/account" class="nav-link{account_active}">
            <span class="nav-icon">{icon_account}</span>
            <span class="nav-label">Account</span>
        </a>
        <div class="nav-section">Settings</div>
        <div class="nav-group nav-group-settings">
            <a href="/setup" class="nav-link{settings_active}">
                <span class="nav-icon">{icon_settings}</span>
                <span class="nav-label">Settings</span>
            </a>
            {settings_submenu}
        </div>
        <a href="/logs" class="nav-link{logs_active}">
            <span class="nav-icon">{icon_logs}</span>
            <span class="nav-label">Logs</span>
        </a>"##,
        chat_active = if active == "chat" { " active" } else { "" },
        dash_active = if active == "dashboard" { " active" } else { "" },
        automations_active = if active == "automations" {
            " active"
        } else {
            ""
        },
        skills_active = if active == "skills" { " active" } else { "" },
        memory_active = if active == "memory" { " active" } else { "" },
        vault_active = if active == "vault" { " active" } else { "" },
        perms_active = if active == "permissions" {
            " active"
        } else {
            ""
        },
        approvals_active = if active == "approvals" { " active" } else { "" },
        account_active = if active == "account" { " active" } else { "" },
        settings_active = if active == "settings" { " active" } else { "" },
        logs_active = if active == "logs" { " active" } else { "" },
        icon_chat = ICON_CHAT,
        icon_dash = ICON_DASHBOARD,
        icon_automations = ICON_AUTOMATIONS,
        icon_skills = ICON_SKILLS,
        icon_memory = ICON_MEMORY,
        icon_vault = ICON_VAULT,
        icon_perms = ICON_PERMISSIONS,
        icon_approvals = ICON_APPROVALS,
        icon_account = ICON_ACCOUNT,
        icon_settings = ICON_SETTINGS,
        icon_logs = ICON_LOGS,
        settings_submenu = settings_submenu,
    );

    format!(
        r##"<nav class="sidebar">
            <div class="sidebar-header">
                <a href="/" class="logo-link">
                    {icon}
                </a>
            </div>
            <div class="nav">
                {links}
            </div>
            <div class="sidebar-footer">
                <span class="version-badge">v{version}</span>
            </div>
        </nav>"##,
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
        r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{title} — Homun</title>
    <link rel="icon" href="/static/img/favicon/favicon.ico" sizes="any">
    <link rel="icon" href="/static/img/favicon.svg" type="image/svg+xml">
    <link rel="apple-touch-icon" href="/static/img/favicon/apple-touch-icon.png">
    <link rel="manifest" href="/static/img/favicon/site.webmanifest">
    <link rel="stylesheet" href="/static/css/style.css">
    <script>
    (function() {{
        const theme = localStorage.getItem('homun-theme') || 'system';
        if (theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)) {{
            document.documentElement.classList.add('dark');
        }}
    }})();
    </script>
</head>
<body>
    <div class="app">
        {sidebar_html}
        {body}
    </div>
    {script_tags}
</body>
</html>"##
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

                <section class="section usage-section">
                    <h2>Token Usage</h2>
                    <div class="usage-controls">
                        <div class="usage-presets">
                            <button class="btn btn-secondary btn-sm usage-range-btn is-active" type="button" data-days="7">7d</button>
                            <button class="btn btn-secondary btn-sm usage-range-btn" type="button" data-days="30">30d</button>
                            <button class="btn btn-secondary btn-sm usage-range-btn" type="button" data-days="90">90d</button>
                            <button class="btn btn-secondary btn-sm usage-range-btn" type="button" data-days="all">All</button>
                        </div>
                        <div class="usage-date-range">
                            <label class="usage-filter">
                                <span>From</span>
                                <input type="date" class="input usage-date-input" id="usage-since">
                            </label>
                            <label class="usage-filter">
                                <span>To</span>
                                <input type="date" class="input usage-date-input" id="usage-until">
                            </label>
                        </div>
                        <button class="btn btn-secondary btn-sm" type="button" id="usage-refresh">Refresh</button>
                    </div>

                    <div class="stats-grid usage-stats-grid">
                        <div class="stat-card">
                            <div class="stat-label">Total Tokens</div>
                            <div class="stat-value" id="usage-total-tokens">-</div>
                            <div class="stat-sub" id="usage-days-count">-</div>
                        </div>
                        <div class="stat-card">
                            <div class="stat-label">Prompt Tokens</div>
                            <div class="stat-value" id="usage-prompt-tokens">-</div>
                            <div class="stat-sub">input</div>
                        </div>
                        <div class="stat-card">
                            <div class="stat-label">Completion Tokens</div>
                            <div class="stat-value" id="usage-completion-tokens">-</div>
                            <div class="stat-sub">output</div>
                        </div>
                        <div class="stat-card">
                            <div class="stat-label">Estimated Cost (USD)</div>
                            <div class="stat-value" id="usage-estimated-cost">$0.00</div>
                            <div class="stat-sub" id="usage-total-calls">- calls</div>
                        </div>
                    </div>

                    <div class="usage-panels">
                        <div class="usage-panel">
                            <div class="usage-panel-title">Daily Token Trend</div>
                            <svg id="usage-chart" viewBox="0 0 720 220" preserveAspectRatio="none"></svg>
                            <div class="usage-chart-empty" id="usage-chart-empty" hidden>No usage data in selected range.</div>
                        </div>
                        <div class="usage-panel">
                            <div class="usage-panel-title">Prompt vs Completion</div>
                            <div class="usage-split" id="usage-split"></div>
                        </div>
                    </div>

                    <div class="usage-table-wrap">
                        <table class="usage-table" id="usage-models-table">
                            <thead>
                                <tr>
                                    <th>Model</th>
                                    <th>Provider</th>
                                    <th>Prompt</th>
                                    <th>Completion</th>
                                    <th>Total</th>
                                    <th>Calls</th>
                                    <th>Est. Cost</th>
                                </tr>
                            </thead>
                            <tbody id="usage-models-body">
                                <tr>
                                    <td colspan="7" class="usage-loading">Loading usage...</td>
                                </tr>
                            </tbody>
                        </table>
                    </div>
                </section>

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

    Html(page_html(
        "Dashboard",
        "dashboard",
        &body,
        &["dashboard.js"],
    ))
    .into_response()
}

// ─── Settings ───────────────────────────────────────────────────

async fn setup_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let config = state.config.read().await;
    let providers_output = build_providers_html::build(&config);
    let providers_html = &providers_output.cards_html;
    let catalog_modal_html = &providers_output.catalog_modal_html;

    // Resolve active provider for the banner
    let active_provider_name = config
        .resolve_provider(&config.agent.model)
        .map(|(name, _)| name.to_string())
        .unwrap_or_default();
    let active_model_display = if config.agent.model.is_empty() {
        String::new()
    } else {
        // Strip provider prefix for display
        config
            .agent
            .model
            .split_once('/')
            .map(|(_, m)| m.to_string())
            .unwrap_or_else(|| config.agent.model.clone())
    };
    let active_provider_display =
        build_providers_html::get_provider_display_name(&active_provider_name);

    let body = format!(
        r##"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Settings</h1>
                    </div>
                </div>

                <section class="section" id="section-providers">
                    <h2>Model &amp; Providers</h2>

                    <div class="active-model-banner" id="active-model-banner" {active_banner_hidden}>
                        <div class="active-model-info">
                            <span class="active-model-label">Active Model</span>
                            <span class="active-model-name" id="active-model-name">{active_model_display}</span>
                            <span class="active-model-provider" id="active-model-provider">via {active_provider_display}</span>
                        </div>
                    </div>

                    <div id="no-model-banner" class="no-model-warning" {no_model_hidden}>
                        <span class="no-model-warning-icon">!</span>
                        <div class="no-model-warning-text">
                            <strong>No model configured</strong>
                            <p>Configure a provider below, then select a model to get started.</p>
                        </div>
                    </div>

                    <div class="configured-providers-grid" id="provider-grid">
                        {providers_html}
                    </div>
                    <button type="button" class="btn btn-secondary" id="btn-add-provider" style="margin-top:12px;">+ Add Provider</button>

                    {catalog_modal_html}

                    <details class="section-advanced" id="advanced-agent">
                        <summary>Advanced Agent Settings</summary>
                        <form class="form form--full" id="agent-form" style="margin-top:12px;">
                            <div class="form-group model-selector-section">
                                <label class="model-selector-label">Vision Model</label>
                                <select id="vision-model-select" class="input">
                                    <option value="">Loading models…</option>
                                </select>
                                <input type="hidden" name="vision_model" id="vision-model-value" value="{vision_model}">
                                <div class="form-hint">Model for image analysis. Falls back to Chat Model if empty.</div>
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
                                <div class="form-group">
                                    <label>XML Fallback Delay (ms)</label>
                                    <input type="number" name="xml_fallback_delay_ms" value="{xml_fallback_delay_ms}" min="0" step="100" class="input">
                                    <div class="form-hint">Delay before retrying when switching to XML tool dispatch. Prevents rate-limit errors on free models.</div>
                                </div>
                            </div>
                            <div class="form-group">
                                <label>Fallback Models</label>
                                <div class="form-hint" style="margin-bottom:8px;">If the primary model fails (rate limit, outage), these models are tried in order.</div>
                                <div id="fallback-models-list" class="tag-list" data-models='{fallback_models_json}'></div>
                                <div class="fallback-add-row">
                                    <select id="fallback-model-select" class="input input--inline">
                                        <option value="">Add fallback model…</option>
                                    </select>
                                    <button type="button" id="btn-add-fallback" class="btn btn-secondary btn--sm">Add</button>
                                </div>
                            </div>
                            <button type="submit" class="btn btn-primary">Save Advanced Settings</button>
                        </form>
                    </details>
                </section>

                <section class="section" id="section-channels">
                    <h2>Channels</h2>
                    <div class="provider-grid" id="channel-grid">
                        {channels_html}
                    </div>
                </section>

                <section class="section" id="section-browser">
                    <h2>Browser Automation</h2>
                    <div class="form-hint" style="margin-bottom:12px;">Uses Chrome/Chromium via CDP. Auto-detected: {browser_status}.</div>
                    <form class="form" id="browser-form">
                        <div class="setting-toggle-row">
                            <div class="setting-toggle-info">
                                <span class="setting-toggle-name">Headless Mode</span>
                                <span class="setting-toggle-desc">Run browser without visible window</span>
                            </div>
                            <div class="toggle-wrap">
                                <input type="checkbox" id="browser-headless" name="headless" class="toggle-input" {browser_headless_checked}>
                                <label class="toggle-label" for="browser-headless"></label>
                            </div>
                        </div>
                        <div class="form-group">
                            <label>Chrome Executable Path</label>
                            <input type="text" id="browser-executable" name="executable_path" value="{executable_path}" class="input" placeholder="Auto-detect (leave empty)">
                            <div class="form-hint">Leave empty to auto-detect. Override if Chrome is in a custom location.</div>
                        </div>
                        <div class="form-row--2">
                            <div class="form-group">
                                <label>Action Timeout (s)</label>
                                <input type="number" id="browser-action-timeout" name="action_timeout_secs" value="{action_timeout_secs}" min="5" max="300" class="input">
                                <div class="form-hint">Max time for click, type, etc.</div>
                            </div>
                            <div class="form-group">
                                <label>Navigation Timeout (s)</label>
                                <input type="number" id="browser-nav-timeout" name="navigation_timeout_secs" value="{navigation_timeout_secs}" min="5" max="300" class="input">
                                <div class="form-hint">Max time for page loads.</div>
                            </div>
                        </div>
                        <div class="form-row--2">
                            <div class="form-group">
                                <label>Snapshot Limit</label>
                                <input type="number" id="browser-snapshot-limit" name="snapshot_limit" value="{snapshot_limit}" min="10" max="500" class="input">
                                <div class="form-hint">Max elements in accessibility tree.</div>
                            </div>
                        </div>
                        <div class="form-actions">
                            <button type="submit" class="btn btn-primary">Save Browser Config</button>
                            <button type="button" class="btn btn-secondary" id="btn-test-browser">Test Connection</button>
                        </div>
                        <div id="browser-result" class="form-hint" style="margin-top:10px;"></div>
                    </form>
                </section>

                <section class="section" id="section-memory">
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

                <section class="section" id="section-theme">
                    <h2>Appearance</h2>
                    <form class="form" id="appearance-form">
                        <div class="form-group">
                            <label>Theme</label>
                            <select name="theme" class="input" id="theme-select">
                                <option value="system" {theme_system}>System</option>
                                <option value="light" {theme_light}>Light</option>
                                <option value="dark" {theme_dark}>Dark</option>
                            </select>
                            <div class="form-hint">Choose the interface color scheme.</div>
                        </div>
                        <button type="submit" class="btn btn-primary">Save Appearance</button>
                    </form>
                </section>
            </div>
        </main>

        <!-- Channel Configuration Modal -->
        <div id="channel-modal" class="modal">
            <div class="modal-backdrop"></div>
            <div class="modal-content modal-content--channel">
                <div class="modal-header-group">
                    <div class="modal-header">
                        <h3 class="modal-title" id="modal-channel-name">Channel</h3>
                        <button class="modal-close ch-modal-close" type="button">&times;</button>
                    </div>
                    <p class="modal-subtitle" id="channel-subtitle"></p>
                </div>
                <div class="modal-body">
                    <form id="channel-config-form">
                        <input type="hidden" id="modal-channel-id" name="channel">

                        <!-- Token (Telegram/Discord/Slack) -->
                        <div class="form-group" id="ch-token-group" style="display:none;">
                            <label for="ch-token">Bot Token</label>
                            <input type="password" id="ch-token" name="token" class="input">
                            <div class="form-hint" id="ch-token-hint">Stored encrypted locally.</div>
                        </div>

                        <!-- WhatsApp phone + pairing -->
                        <div class="form-group" id="ch-phone-group" style="display:none;">
                            <label for="ch-phone">Phone Number</label>
                            <input type="tel" id="ch-phone" name="phone_number" class="input" placeholder="393331234567">
                            <div class="form-hint">International format without + (e.g. 393331234567)</div>
                        </div>
                        <div id="ch-wa-pairing" style="display:none;">
                            <div id="ch-wa-pairing-status" class="pairing-status"></div>
                            <div id="ch-wa-pairing-code" class="pairing-code" style="display:none;"></div>
                        </div>

                        <!-- Allowed Users -->
                        <div class="form-group" id="ch-allow-from-group" style="display:none;">
                            <label>Allowed Users</label>
                            <input type="text" id="ch-allow-from" name="allow_from" class="input" placeholder="User IDs, comma-separated">
                            <div class="form-hint" id="ch-allow-from-hint">Only these users can interact with the bot.</div>
                        </div>

                        <!-- Discord default channel -->
                        <div class="form-group" id="ch-discord-channel-group" style="display:none;">
                            <label for="ch-discord-channel">Default Channel ID</label>
                            <input type="text" id="ch-discord-channel" name="default_channel_id" class="input">
                            <div class="form-hint">For proactive messages (optional)</div>
                        </div>

                        <!-- Slack channel -->
                        <div class="form-group" id="ch-slack-channel-group" style="display:none;">
                            <label for="ch-slack-channel">Channel ID</label>
                            <input type="text" id="ch-slack-channel" name="slack_channel_id" class="input">
                            <div class="form-hint">Specific channel to monitor (e.g., C1234567890). Leave empty for auto-discovery.</div>
                        </div>

                        <!-- Email: Mail Servers (2 columns) -->
                        <div id="ch-email-servers-group" style="display:none;">
                            <div class="modal-section-label">Mail Servers</div>
                            <div class="form-row--2">
                                <div class="form-group">
                                    <label for="ch-email-imap-host">IMAP Server</label>
                                    <input type="text" id="ch-email-imap-host" name="imap_host" class="input" placeholder="imap.gmail.com">
                                </div>
                                <div class="form-group">
                                    <label for="ch-email-imap-port">IMAP Port</label>
                                    <input type="number" id="ch-email-imap-port" name="imap_port" class="input" placeholder="993">
                                </div>
                            </div>
                            <div class="form-row--2">
                                <div class="form-group">
                                    <label for="ch-email-smtp-host">SMTP Server</label>
                                    <input type="text" id="ch-email-smtp-host" name="smtp_host" class="input" placeholder="smtp.gmail.com">
                                </div>
                                <div class="form-group">
                                    <label for="ch-email-smtp-port">SMTP Port</label>
                                    <input type="number" id="ch-email-smtp-port" name="smtp_port" class="input" placeholder="465">
                                </div>
                            </div>
                        </div>

                        <!-- Email: Credentials (2 columns) -->
                        <div id="ch-email-credentials-group" style="display:none;">
                            <div class="modal-section-label">Credentials</div>
                            <div class="form-row--2">
                                <div class="form-group">
                                    <label for="ch-email-username">Username</label>
                                    <input type="text" id="ch-email-username" name="email_username" class="input" placeholder="bot@example.com">
                                </div>
                                <div class="form-group">
                                    <label for="ch-email-password">Password</label>
                                    <input type="password" id="ch-email-password" name="email_password" class="input" placeholder="App password (stored encrypted)">
                                </div>
                            </div>
                            <div class="form-group">
                                <label for="ch-email-from">From Address</label>
                                <input type="text" id="ch-email-from" name="from_address" class="input" placeholder="bot@example.com">
                            </div>
                        </div>

                        <!-- Email: Behavior (mode + notify) -->
                        <div id="ch-email-behavior-group" style="display:none;">
                            <div class="modal-section-label">Behavior</div>
                            <div class="form-row--2">
                                <div class="form-group" id="ch-email-mode-group">
                                    <label for="ch-email-mode">Response Mode</label>
                                    <select id="ch-email-mode" name="email_mode" class="input">
                                        <option value="assisted">Assisted (summary + approval)</option>
                                        <option value="automatic">Automatic (direct response)</option>
                                        <option value="on_demand">On-Demand (trigger word only)</option>
                                    </select>
                                    <div class="form-hint" id="ch-email-mode-hint">Generates summary and draft, sends to notification channel for approval.</div>
                                </div>
                                <div class="form-group" id="ch-email-trigger-group" style="display:none;">
                                    <label for="ch-email-trigger-word">Trigger Word</label>
                                    <input type="text" id="ch-email-trigger-word" name="email_trigger_word" class="input" placeholder="Auto-generated if empty">
                                    <div class="form-hint">Include in subject/body to activate the bot.</div>
                                </div>
                            </div>
                            <div id="ch-email-notify-group" style="display:none;">
                                <div class="form-row--2">
                                    <div class="form-group">
                                        <label for="ch-email-notify-channel">Notify Channel</label>
                                        <select id="ch-email-notify-channel" name="email_notify_channel" class="input">
                                            <option value="">None</option>
                                            <option value="telegram">Telegram</option>
                                            <option value="discord">Discord</option>
                                            <option value="slack">Slack</option>
                                            <option value="whatsapp">WhatsApp</option>
                                        </select>
                                    </div>
                                    <div class="form-group">
                                        <label for="ch-email-notify-chat-id">Notify Chat ID</label>
                                        <input type="text" id="ch-email-notify-chat-id" name="email_notify_chat_id" class="input" placeholder="User/Channel ID">
                                        <div class="form-hint form-hint--suggest" id="ch-notify-hint"></div>
                                    </div>
                                </div>
                            </div>
                        </div>

                        <!-- Web channel fields -->
                        <div class="form-group" id="ch-web-host-group" style="display:none;">
                            <label for="ch-web-host">Host</label>
                            <input type="text" id="ch-web-host" name="host" class="input">
                        </div>
                        <div class="form-group" id="ch-web-port-group" style="display:none;">
                            <label for="ch-web-port">Port</label>
                            <input type="number" id="ch-web-port" name="port" class="input">
                        </div>
                    </form>
                    <div id="ch-test-result" class="form-hint" style="margin-top:10px;"></div>
                </div>
                <div class="modal-footer">
                    <button type="button" class="btn btn-secondary ch-modal-cancel">Cancel</button>
                    <button type="submit" form="channel-config-form" class="btn btn-primary" id="btn-ch-save">Save &amp; Enable</button>
                    <button type="button" id="btn-test-channel" class="btn btn-secondary">Test Connection</button>
                    <button type="button" id="btn-wa-pair" class="btn btn-success" style="display:none;">Start Pairing</button>
                </div>
            </div>
        </div>

        "##,
        active_model_display = active_model_display,
        active_provider_display = active_provider_display,
        active_banner_hidden = if config.agent.model.is_empty() {
            "style=\"display:none\""
        } else {
            ""
        },
        no_model_hidden = if config.agent.model.is_empty() {
            ""
        } else {
            "style=\"display:none\""
        },
        vision_model = config.agent.vision_model,
        max_tokens = config.agent.max_tokens,
        temperature = config.agent.temperature,
        max_iterations = config.agent.max_iterations,
        xml_fallback_delay_ms = config.agent.xml_fallback_delay_ms,
        fallback_models_json = serde_json::to_string(&config.agent.fallback_models)
            .unwrap_or_else(|_| "[]".to_string()),
        conversation_retention_days = config.memory.conversation_retention_days,
        history_retention_days = config.memory.history_retention_days,
        daily_archive_months = config.memory.daily_archive_months,
        auto_cleanup_checked = if config.memory.auto_cleanup {
            "checked"
        } else {
            ""
        },
        browser_headless_checked = if config.browser.headless {
            "checked"
        } else {
            ""
        },
        executable_path = config.browser.executable_path,
        browser_status = if config.browser.resolved_executable().is_some() {
            let path = config.browser.resolved_executable().unwrap();
            format!("Chrome found at {}", path.display())
        } else {
            "Chrome not found — install Chrome or set path below".to_string()
        },
        action_timeout_secs = config.browser.action_timeout_secs,
        navigation_timeout_secs = config.browser.navigation_timeout_secs,
        snapshot_limit = config.browser.snapshot_limit,
        theme_system = if config.ui.theme == "system" {
            "selected"
        } else {
            ""
        },
        theme_light = if config.ui.theme == "light" {
            "selected"
        } else {
            ""
        },
        theme_dark = if config.ui.theme == "dark" {
            "selected"
        } else {
            ""
        },
        providers_html = providers_html,
        catalog_modal_html = catalog_modal_html,
        channels_html = build_channels_cards_html(&config),
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

// ─── Automations ────────────────────────────────────────────────

async fn automations_page() -> Html<String> {
    let body = r#"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Automations</h1>
                        <span class="badge badge-info" id="automations-count">0</span>
                    </div>
                    <div class="actions">
                        <button class="btn btn-secondary btn-sm" id="btn-automations-refresh">Refresh</button>
                    </div>
                </div>

                <section class="section">
                    <h2>Create Automation</h2>
                    <form id="automation-create-form" class="form form--full">
                        <div class="form-row--2">
                            <div class="form-group">
                                <label for="automation-name">Name</label>
                                <input id="automation-name" class="input" type="text" maxlength="120" placeholder="Email digest">
                                <div class="form-hint">Short label shown in the command center.</div>
                            </div>
                            <div class="form-group">
                                <label for="automation-deliver-to">Deliver To</label>
                                <select id="automation-deliver-to" class="input"></select>
                                <div class="form-hint">Choose one of the configured channels.</div>
                            </div>
                        </div>

                        <div class="form-group">
                            <label for="automation-prompt">Prompt</label>
                            <textarea id="automation-prompt" class="input automation-textarea" rows="4" placeholder="Vai su Gmail, leggi le email non lette e fammi un riassunto."></textarea>
                            <div class="form-hint">Detailed instructions that the agent executes at run time.</div>
                        </div>

                        <div class="form-row--2">
                            <div class="form-group">
                                <label for="automation-schedule-mode">Schedule Type</label>
                                <select id="automation-schedule-mode" class="input">
                                    <option value="daily">Every day</option>
                                    <option value="weekdays">Weekdays (Mon-Fri)</option>
                                    <option value="weekly">Every week</option>
                                    <option value="interval">Every N hours</option>
                                    <option value="custom">Advanced (cron/every)</option>
                                </select>
                            </div>
                            <div class="form-group" id="automation-time-group">
                                <label for="automation-time">Time</label>
                                <input id="automation-time" class="input" type="time" value="09:00">
                                <div class="form-hint">Local time.</div>
                            </div>
                            <div class="form-group" id="automation-weekday-group" style="display:none;">
                                <label for="automation-weekday">Day of week</label>
                                <select id="automation-weekday" class="input">
                                    <option value="1">Monday</option>
                                    <option value="2">Tuesday</option>
                                    <option value="3">Wednesday</option>
                                    <option value="4">Thursday</option>
                                    <option value="5">Friday</option>
                                    <option value="6">Saturday</option>
                                    <option value="7">Sunday</option>
                                </select>
                            </div>
                            <div class="form-group" id="automation-interval-group" style="display:none;">
                                <label for="automation-interval-hours">Every (hours)</label>
                                <input id="automation-interval-hours" class="input" type="number" min="1" step="1" value="6">
                                <div class="form-hint">Example: every 6 hours.</div>
                            </div>
                            <div class="form-group" id="automation-custom-group" style="display:none;">
                                <label for="automation-custom-schedule">Custom schedule</label>
                                <input id="automation-custom-schedule" class="input" type="text" placeholder="cron:0 9 * * * or every:3600">
                                <div class="form-hint">Advanced format only.</div>
                            </div>
                        </div>

                        <div class="form-row--2">
                            <div class="form-group">
                                <label for="automation-trigger">Trigger</label>
                                <select id="automation-trigger" class="input">
                                    <option value="always">Always notify</option>
                                    <option value="on_change">Notify on change</option>
                                    <option value="contains">Notify when output contains text</option>
                                </select>
                            </div>
                            <div class="form-group" id="automation-trigger-value-group" style="display:none;">
                                <label for="automation-trigger-value">Trigger Value</label>
                                <input id="automation-trigger-value" class="input" type="text" placeholder="e.g. prezzo sceso">
                                <div class="form-hint">Used only with trigger=contains.</div>
                            </div>
                        </div>

                        <div class="actions">
                            <button class="btn btn-primary" type="submit">Create Automation</button>
                        </div>
                    </form>
                </section>

                <section class="section">
                    <h2>Automation List</h2>
                    <div id="automations-list" class="item-list">
                        <div class="empty-state">
                            <p>Loading automations...</p>
                        </div>
                    </div>
                </section>

                <section class="section">
                    <h2>Run History</h2>
                    <div id="automation-history" class="scrollable-list">
                        <div class="empty-state">
                            <p>Select an automation to load run history.</p>
                        </div>
                    </div>
                </section>
            </div>
        </main>"#;

    Html(page_html(
        "Automations",
        "automations",
        body,
        &["automations.js"],
    ))
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
        format!(
            r#"<div class="skill-source-chips" id="source-chips">{}</div>"#,
            source_chips.join("")
        )
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
                        <span id="logs-status" class="badge badge-warning">Connecting...</span>
                    </div>
                </div>
                <div class="logs-toolbar">
                    <label class="form-group logs-toolbar-item">
                        <span>Level</span>
                        <select id="logs-level" class="input">
                            <option value="trace">Trace+</option>
                            <option value="debug">Debug+</option>
                            <option value="info" selected>Info+</option>
                            <option value="warn">Warn+</option>
                            <option value="error">Error only</option>
                        </select>
                    </label>
                    <label class="checkbox-label logs-toolbar-item">
                        <input type="checkbox" id="logs-autoscroll" checked>
                        Auto-scroll
                    </label>
                    <button class="btn btn-secondary btn-sm logs-toolbar-item" id="logs-clear">Clear</button>
                    <span class="logs-count" id="logs-count">0 events</span>
                </div>
                <div class="log-viewer" id="log-viewer">
                    <div class="empty-state log-empty">
                        <p>Waiting for log events...</p>
                    </div>
                </div>
            </div>
        </main>"#;

    Html(page_html("Logs", "logs", body, &["logs.js"]))
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
        .map(|e| {
            e.filter_map(|f| f.ok())
                .filter(|f| f.path().extension().is_some_and(|ext| ext == "md"))
                .count()
        })
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
        file_count = [has_memory, has_instructions]
            .iter()
            .filter(|&&v| v)
            .count(),
        file_detail = {
            let mut parts = Vec::new();
            if has_memory {
                parts.push("MEMORY.md");
            }
            if has_instructions {
                parts.push("INSTRUCTIONS.md");
            }
            if parts.is_empty() {
                "no files yet".to_string()
            } else {
                parts.join(" + ")
            }
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

    let body = format!(
        r#"<main class="content">
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
        default_read_checked = if config.permissions.default.read {
            "checked"
        } else {
            ""
        },
        default_write_checked = if config.permissions.default.write {
            "checked"
        } else {
            ""
        },
        default_delete_checked = if config.permissions.default.delete {
            "checked"
        } else {
            ""
        },
    );

    Html(page_html(
        "Permissions",
        "permissions",
        &body,
        &["permissions.js"],
    ))
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
    if config.channels.email.enabled {
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
        (
            ICON_EMAIL,
            "Email",
            config.channels.email.enabled,
            "IMAP/SMTP",
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

/// Module for provider accordion HTML generation
mod build_providers_html {
    use crate::config::Config;

    pub struct ProvidersOutput {
        pub cards_html: String,
        pub catalog_modal_html: String,
    }

    /// Bundled parameters for `build_provider_card` (avoids clippy::too_many_arguments).
    struct ProviderCardData<'a> {
        name: &'a str,
        display_name: &'a str,
        description: &'a str,
        has_key: bool,
        has_url: bool,
        is_active: bool,
        api_key_mask: &'a str,
        api_base: &'a str,
        current_model: &'a str,
    }

    /// Provider display metadata: (display_name, description, needs_api_key, needs_base_url)
    fn get_provider_meta(name: &str) -> (&'static str, &'static str, bool, bool) {
        match name {
            "anthropic" => ("Anthropic", "Claude API (Sonnet, Opus, Haiku)", true, false),
            "openai" => ("OpenAI", "GPT-4o, o1, o3 series", true, false),
            "openrouter" => ("OpenRouter", "200+ models via unified API", true, false),
            "gemini" => ("Google Gemini", "Gemini 2.0 Flash, Pro", true, false),
            "ollama" => ("Ollama (local)", "Run models locally", false, true),
            "ollama_cloud" => ("Ollama Cloud", "Hosted Ollama models", true, false),
            "vllm" => ("vLLM", "Self-hosted vLLM server", false, true),
            "custom" => ("Custom", "Any OpenAI-compatible endpoint", false, true),
            "deepseek" => ("DeepSeek", "DeepSeek V3, R1, Coder", true, false),
            "groq" => ("Groq", "Ultra-fast inference", true, false),
            "mistral" => ("Mistral", "Mistral and Mixtral models", true, false),
            "xai" => ("xAI (Grok)", "Grok models by xAI", true, false),
            "together" => ("Together AI", "Open-source models at scale", true, false),
            "fireworks" => ("Fireworks AI", "Fast serverless inference", true, false),
            "perplexity" => ("Perplexity", "Sonar models with web search", true, false),
            "cohere" => ("Cohere", "Command R+, Command models", true, false),
            "venice" => ("Venice", "Privacy-focused AI inference", true, false),
            "aihubmix" => ("AiHubMix", "Multi-model aggregator", true, false),
            "vercel" => ("Vercel AI", "Vercel AI Gateway", true, false),
            "cloudflare" => ("Cloudflare AI", "Cloudflare AI Gateway", true, false),
            "copilot" => ("GitHub Copilot", "GitHub Copilot API", true, false),
            "bedrock" => ("AWS Bedrock", "Amazon Bedrock models", true, false),
            "minimax" => ("MiniMax", "MiniMax AI models", true, false),
            "dashscope" => ("DashScope", "Alibaba Qwen models", true, false),
            "moonshot" => ("Moonshot (Kimi)", "Moonshot AI models", true, false),
            "zhipu" => ("Zhipu AI (GLM)", "GLM models by Zhipu", true, false),
            _ => ("Unknown", "Unknown provider", true, false),
        }
    }

    pub fn get_provider_display_name(name: &str) -> &'static str {
        get_provider_meta(name).0
    }

    pub fn build(config: &Config) -> ProvidersOutput {
        let active_provider = config
            .resolve_provider(&config.agent.model)
            .map(|(name, _)| name.to_string())
            .unwrap_or_default();

        let mut cards_html = String::new();
        let mut catalog_items = Vec::new();

        for (name, pc) in config.providers.iter() {
            let configured = config.is_provider_configured(name);
            let is_active = name == active_provider;
            let (display_name, description, has_key, has_url) = get_provider_meta(name);

            if is_active || configured {
                let api_key_mask = if configured && has_key {
                    "••••••••"
                } else {
                    ""
                };
                cards_html.push_str(&build_provider_card(&ProviderCardData {
                    name,
                    display_name,
                    description,
                    has_key,
                    has_url,
                    is_active,
                    api_key_mask,
                    api_base: pc.api_base.as_deref().unwrap_or(""),
                    current_model: &config.agent.model,
                }));
            } else {
                catalog_items.push(build_catalog_card(
                    name,
                    display_name,
                    description,
                    has_key,
                    has_url,
                ));
            }
        }

        // Build catalog modal
        let mut catalog_html = String::from(
            r#"<div id="provider-catalog-modal" class="modal">
            <div class="modal-backdrop"></div>
            <div class="modal-content modal-content--wide">
                <div class="modal-header">
                    <h3 class="modal-title">Add Provider</h3>
                    <button class="modal-close catalog-modal-close" type="button">&times;</button>
                </div>
                <div class="modal-body">
                    <input type="text" class="input" id="catalog-search" placeholder="Search providers...">
                    <div class="catalog-grid" id="catalog-grid">"#,
        );
        for item in &catalog_items {
            catalog_html.push_str(item);
        }
        catalog_html.push_str(
            r#"</div>
                </div>
            </div>
        </div>"#,
        );

        ProvidersOutput {
            cards_html,
            catalog_modal_html: catalog_html,
        }
    }

    fn build_provider_card(p: &ProviderCardData<'_>) -> String {
        let is_active = p.is_active;
        let name = p.name;
        let display_name = p.display_name;
        let description = p.description;
        let has_key = p.has_key;
        let has_url = p.has_url;
        let api_key_mask = p.api_key_mask;
        let api_base = p.api_base;
        let current_model = p.current_model;

        let active_cls = if is_active {
            " provider-card--active"
        } else {
            ""
        };

        let active_badge = if is_active {
            r#"<span class="provider-active-badge">Active</span>"#
        } else {
            ""
        };

        // Credential fields
        let key_field = if has_key {
            format!(
                r#"<div class="form-group">
                    <label>API Key</label>
                    <div class="credential-row">
                        <input type="password" class="input provider-api-key" placeholder="{placeholder}" value="" data-mask="{api_key_mask}">
                        <button type="button" class="btn btn-secondary btn--sm provider-save-key">Save Key</button>
                    </div>
                    <div class="form-hint">Stored encrypted locally. Leave empty to keep current key.</div>
                </div>"#,
                placeholder = if api_key_mask.is_empty() {
                    "Enter API key..."
                } else {
                    "Configured — enter new key to replace"
                },
            )
        } else {
            String::new()
        };

        let url_field = if has_url {
            format!(
                r#"<div class="form-group">
                    <label>Base URL</label>
                    <div class="credential-row">
                        <input type="text" class="input provider-api-base" placeholder="http://localhost:11434/v1" value="{api_base}">
                        <button type="button" class="btn btn-secondary btn--sm provider-save-url">Save URL</button>
                    </div>
                    <div class="form-hint">API endpoint URL.</div>
                </div>"#,
            )
        } else {
            String::new()
        };

        let custom_hint = if name == "openrouter" {
            "Use the path from OpenRouter (e.g. anthropic/claude-sonnet-4). Prefix added automatically."
        } else if name == "ollama" || name == "ollama_cloud" {
            "Enter the model name (e.g. llama3.3, mistral). Prefix added automatically."
        } else {
            "Enter a model name. Provider prefix is added automatically."
        };

        let provider_prefix = format!("{name}/");
        let active_model_for_this_provider = if current_model.starts_with(&provider_prefix) {
            current_model.to_string()
        } else {
            String::new()
        };

        format!(
            r#"<div class="provider-card{active_cls}" data-provider="{name}" data-configured="true" data-has-key="{has_key}" data-has-url="{has_url}" data-active-model="{active_model_for_this_provider}">
                <div class="provider-card-header" role="button" tabindex="0" aria-expanded="false">
                    <div class="provider-card-left">
                        <span class="provider-card-status{}">&bull;</span>
                        <div class="provider-card-info">
                            <span class="provider-card-name">{display_name}</span>
                            <span class="provider-card-desc">{description}</span>
                        </div>
                    </div>
                    <div class="provider-card-right">
                        {active_badge}
                        <span class="provider-chevron">&#9662;</span>
                    </div>
                </div>
                <div class="provider-card-body" hidden>
                    <div class="provider-credentials">
                        {key_field}
                        {url_field}
                    </div>
                    <div class="provider-models" data-provider="{name}">
                        <label class="provider-models-label">Models</label>
                        <div class="provider-model-list">
                            <div class="form-hint">Loading models…</div>
                        </div>
                        <div class="custom-model-row">
                            <input type="text" class="input input--inline provider-custom-model" placeholder="Custom model name…">
                            <button type="button" class="btn btn-secondary btn--sm provider-use-custom">Use</button>
                        </div>
                        <div class="form-hint">{custom_hint}</div>
                    </div>
                    <button type="button" class="btn btn-ghost btn--sm provider-deactivate" style="margin-top:8px;color:var(--text-muted);">Remove credentials</button>
                </div>
            </div>"#,
            if is_active { " active" } else { " configured" },
        )
    }

    fn build_catalog_card(
        name: &str,
        display_name: &str,
        description: &str,
        has_key: bool,
        has_url: bool,
    ) -> String {
        format!(
            r#"<div class="catalog-card" data-provider="{name}" data-has-key="{has_key}" data-has-url="{has_url}">
                <div class="catalog-card-name">{display_name}</div>
                <div class="catalog-card-desc">{description}</div>
                <button type="button" class="btn btn-secondary btn--sm catalog-configure-btn">Configure</button>
            </div>"#,
        )
    }
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
            desc: "Discord bot via gateway connection",
            icon: ICON_DISCORD,
            has_token: true,
        },
        ChannelMeta {
            name: "slack",
            display: "Slack",
            desc: "Slack workspace integration via Web API",
            icon: ICON_SLACK,
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
            name: "email",
            display: "Email",
            desc: "IMAP/SMTP email integration",
            icon: ICON_EMAIL,
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
                "slack" => config.channels.slack.enabled,
                "whatsapp" => config.channels.whatsapp.enabled,
                "email" => config.channels.email.enabled,
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
                "slack" if configured => resolve_and_mask_token("slack", &config.channels.slack.token),
                _ => String::new(),
            };
            let allow_from = match ch.name {
                "telegram" => config.channels.telegram.allow_from.join(","),
                "discord" => config.channels.discord.allow_from.join(","),
                "slack" => config.channels.slack.allow_from.join(","),
                "whatsapp" => config.channels.whatsapp.allow_from.join(","),
                "email" => config.channels.email.allow_from.join(","),
                _ => String::new(),
            };
            let phone = &config.channels.whatsapp.phone_number;
            let discord_channel = &config.channels.discord.default_channel_id;
            let slack_channel = &config.channels.slack.channel_id;
            let web_host = &config.channels.web.host;
            let web_port = config.channels.web.port;

            // Email-specific data attributes
            let email_imap_host = &config.channels.email.imap_host;
            let email_imap_port = config.channels.email.imap_port;
            let email_smtp_host = &config.channels.email.smtp_host;
            let email_smtp_port = config.channels.email.smtp_port;
            let email_username = &config.channels.email.username;
            let email_from = &config.channels.email.from_address;

            // Mode/notify from emails.default (multi-account) if it exists
            let default_acc = config.channels.emails.get("default");
            let email_mode = default_acc
                .map(|a| match a.mode {
                    crate::config::EmailMode::Assisted => "assisted",
                    crate::config::EmailMode::Automatic => "automatic",
                    crate::config::EmailMode::OnDemand => "on_demand",
                })
                .unwrap_or("assisted");
            let email_notify_channel = default_acc
                .and_then(|a| a.notify_channel.as_deref())
                .unwrap_or("");
            let email_notify_chat_id = default_acc
                .and_then(|a| a.notify_chat_id.as_deref())
                .unwrap_or("");
            let email_trigger_word = default_acc
                .and_then(|a| a.trigger_word.as_deref())
                .unwrap_or("");

            format!(
                r##"<div class="{classes}" data-channel="{name}" data-display="{display}" data-configured="{configured}" data-enabled="{enabled}" data-has-token="{has_token}" data-token-mask="{token_mask}" data-allow-from="{allow_from}" data-phone="{phone}" data-discord-channel="{discord_channel}" data-slack-channel="{slack_channel}" data-web-host="{web_host}" data-web-port="{web_port}" data-is-web="{is_web}" data-email-imap-host="{email_imap_host}" data-email-imap-port="{email_imap_port}" data-email-smtp-host="{email_smtp_host}" data-email-smtp-port="{email_smtp_port}" data-email-username="{email_username}" data-email-from="{email_from}" data-email-mode="{email_mode}" data-email-notify-channel="{email_notify_channel}" data-email-notify-chat-id="{email_notify_chat_id}" data-email-trigger-word="{email_trigger_word}">
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
                slack_channel = slack_channel,
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

/// Build email account cards for the settings page.
fn build_email_accounts_html(config: &crate::config::Config) -> String {
    let mut html = String::new();

    for (name, acc) in &config.channels.emails {
        // Skip "default" — it's the primary email configured in Channels
        if name == "default" {
            continue;
        }
        let mode_label = match acc.mode {
            crate::config::EmailMode::Assisted => "Assisted",
            crate::config::EmailMode::Automatic => "Automatic",
            crate::config::EmailMode::OnDemand => "On-Demand",
        };
        let mode_badge_cls = match acc.mode {
            crate::config::EmailMode::Assisted => "badge-info",
            crate::config::EmailMode::Automatic => "badge-success",
            crate::config::EmailMode::OnDemand => "badge-neutral",
        };

        let status = if acc.enabled && acc.is_configured() {
            r#"<span class="provider-default-badge">Active</span>"#
        } else {
            ""
        };

        let _notify_info = match (&acc.notify_channel, &acc.notify_chat_id) {
            (Some(ch), Some(id)) => format!("{ch}:{id}"),
            _ => String::new(),
        };

        html.push_str(&format!(
            r##"<div class="provider-card email-account-card" data-email-name="{name}" data-enabled="{enabled}" data-configured="{configured}" data-mode="{mode}" data-imap-host="{imap_host}" data-imap-port="{imap_port}" data-imap-folder="{imap_folder}" data-smtp-host="{smtp_host}" data-smtp-port="{smtp_port}" data-smtp-tls="{smtp_tls}" data-username="{username}" data-from-address="{from_address}" data-idle-timeout="{idle_timeout}" data-allow-from="{allow_from}" data-notify-channel="{notify_channel}" data-notify-chat-id="{notify_chat_id}" data-trigger-word="{trigger_word}" data-batch-threshold="{batch_threshold}" data-batch-window="{batch_window}" data-send-delay="{send_delay}">
                <div class="provider-card-header">
                    <div class="provider-card-info">
                        <span class="channel-icon">{icon}</span>
                        <span class="provider-card-name">{name}</span>
                    </div>
                    <div class="provider-card-actions">
                        {status}
                        <span class="badge {mode_badge_cls}" style="margin-left:4px">{mode_label}</span>
                    </div>
                </div>
                <div class="provider-card-desc">{username} &bull; {imap_host}</div>
            </div>"##,
            name = name,
            enabled = acc.enabled,
            configured = acc.is_configured(),
            mode = mode_label.to_lowercase(),
            icon = ICON_EMAIL,
            imap_host = acc.imap_host,
            imap_port = acc.imap_port,
            imap_folder = acc.imap_folder,
            smtp_host = acc.smtp_host,
            smtp_port = acc.smtp_port,
            smtp_tls = acc.smtp_tls,
            username = acc.username,
            from_address = acc.from_address,
            idle_timeout = acc.idle_timeout_secs,
            allow_from = acc.allow_from.join(","),
            notify_channel = acc.notify_channel.as_deref().unwrap_or(""),
            notify_chat_id = acc.notify_chat_id.as_deref().unwrap_or(""),
            trigger_word = acc.trigger_word.as_deref().unwrap_or(""),
            batch_threshold = acc.batch_threshold,
            batch_window = acc.batch_window_secs,
            send_delay = acc.send_delay_secs,
            status = status,
            mode_badge_cls = mode_badge_cls,
            mode_label = mode_label,
        ));
    }

    html
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

async fn account_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let config = state.config.read().await;
    let email_accounts_html = build_email_accounts_html(&config);
    drop(config);

    let body = format!(
        r##"<main class="content">
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
                                    <input type="text" id="webhook-url-preview" class="input" readonly value="POST /api/v1/webhook/{{token}}">
                                </div>
                            </div>
                            <button type="submit" class="btn btn-primary btn-sm">Create Token</button>
                        </form>
                    </details>
                </section>

                <!-- Additional Email Accounts -->
                <section class="section" id="section-email-accounts">
                    <div class="section-header" style="display:flex;justify-content:space-between;align-items:center;">
                        <h2>Additional Email Accounts</h2>
                        <button class="btn btn-primary btn-sm" id="btn-add-email-account">+ Add Account</button>
                    </div>
                    <div class="form-hint" style="margin-bottom:12px;">Add extra email accounts beyond the primary one configured in Settings &rarr; Channels.</div>
                    <div class="provider-grid" id="email-accounts-grid">
                        {email_accounts_html}
                    </div>
                </section>

            </div>
        </main>

        <!-- Email Account Modal -->
        <div id="email-account-modal" class="modal">
            <div class="modal-backdrop"></div>
            <div class="modal-content modal-content--channel">
                <div class="modal-header-group">
                    <div class="modal-header">
                        <h3 class="modal-title" id="email-modal-title">Configure Email Account</h3>
                        <button class="modal-close ea-modal-close" type="button">&times;</button>
                    </div>
                    <p class="modal-subtitle">IMAP/SMTP account for receiving and responding to emails.</p>
                </div>
                <div class="modal-body">
                    <form id="email-account-form">
                        <div class="form-group">
                            <label for="ea-name">Account Name</label>
                            <input type="text" id="ea-name" name="name" class="input" placeholder="e.g. lavoro, personal" required>
                            <div class="form-hint">Unique identifier. Used in channel routing (email:name).</div>
                        </div>

                        <div class="modal-section-label">Mail Servers</div>
                        <div class="form-row--2">
                            <div class="form-group">
                                <label for="ea-imap-host">IMAP Server</label>
                                <input type="text" id="ea-imap-host" name="imap_host" class="input" placeholder="imap.gmail.com">
                            </div>
                            <div class="form-group">
                                <label for="ea-imap-port">IMAP Port</label>
                                <input type="number" id="ea-imap-port" name="imap_port" class="input" value="993">
                            </div>
                        </div>
                        <div class="form-row--2">
                            <div class="form-group">
                                <label for="ea-smtp-host">SMTP Server</label>
                                <input type="text" id="ea-smtp-host" name="smtp_host" class="input" placeholder="smtp.gmail.com">
                            </div>
                            <div class="form-group">
                                <label for="ea-smtp-port">SMTP Port</label>
                                <input type="number" id="ea-smtp-port" name="smtp_port" class="input" value="465">
                            </div>
                        </div>

                        <div class="modal-section-label">Credentials</div>
                        <div class="form-row--2">
                            <div class="form-group">
                                <label for="ea-username">Username</label>
                                <input type="text" id="ea-username" name="username" class="input" placeholder="bot@example.com">
                            </div>
                            <div class="form-group">
                                <label for="ea-password">Password</label>
                                <input type="password" id="ea-password" name="password" class="input" placeholder="App password (stored encrypted)">
                            </div>
                        </div>
                        <div class="form-group">
                            <label for="ea-from">From Address</label>
                            <input type="text" id="ea-from" name="from_address" class="input" placeholder="bot@example.com">
                        </div>

                        <div class="modal-section-label">Behavior</div>
                        <div class="form-row--2">
                            <div class="form-group">
                                <label for="ea-mode">Response Mode</label>
                                <select id="ea-mode" name="mode" class="input">
                                    <option value="assisted">Assisted (summary + approval)</option>
                                    <option value="automatic">Automatic (direct response)</option>
                                    <option value="on_demand">On-Demand (trigger word only)</option>
                                </select>
                                <div class="form-hint" id="ea-mode-hint">Generates summary and draft, sends to notification channel for approval.</div>
                            </div>
                            <div class="form-group" id="ea-trigger-field" style="display:none;">
                                <label for="ea-trigger-word">Trigger Word</label>
                                <input type="text" id="ea-trigger-word" name="trigger_word" class="input" placeholder="Auto-generated if empty">
                                <div class="form-hint">Include in subject/body to activate the bot.</div>
                            </div>
                        </div>

                        <div id="ea-notify-fields">
                            <div class="form-row--2">
                                <div class="form-group">
                                    <label for="ea-notify-channel">Notify Channel</label>
                                    <select id="ea-notify-channel" name="notify_channel" class="input">
                                        <option value="">None</option>
                                        <option value="telegram">Telegram</option>
                                        <option value="discord">Discord</option>
                                        <option value="slack">Slack</option>
                                        <option value="whatsapp">WhatsApp</option>
                                    </select>
                                </div>
                                <div class="form-group">
                                    <label for="ea-notify-chat-id">Notify Chat ID</label>
                                    <input type="text" id="ea-notify-chat-id" name="notify_chat_id" class="input" placeholder="User/Channel ID">
                                    <div class="form-hint form-hint--suggest" id="ea-notify-hint"></div>
                                </div>
                            </div>
                        </div>

                        <details style="margin-top:12px;">
                            <summary style="cursor:pointer;font-weight:500;color:var(--text-secondary);">Advanced (Batching &amp; Allow List)</summary>
                            <div style="padding-top:12px;">
                                <div class="form-group">
                                    <label for="ea-allow-from">Allow From</label>
                                    <input type="text" id="ea-allow-from" name="allow_from" class="input" placeholder="user@example.com, * for all">
                                    <div class="form-hint">Comma-separated. Empty = deny all, * = allow all.</div>
                                </div>
                                <div class="form-row--2">
                                    <div class="form-group">
                                        <label for="ea-batch-threshold">Batch Threshold</label>
                                        <input type="number" id="ea-batch-threshold" name="batch_threshold" class="input" value="3" min="1" max="50">
                                        <div class="form-hint">Emails before sending digest</div>
                                    </div>
                                    <div class="form-group">
                                        <label for="ea-batch-window">Batch Window (s)</label>
                                        <input type="number" id="ea-batch-window" name="batch_window_secs" class="input" value="120" min="10" max="3600">
                                        <div class="form-hint">Seconds to accumulate</div>
                                    </div>
                                </div>
                                <div class="form-group">
                                    <label for="ea-send-delay">Send Delay (s)</label>
                                    <input type="number" id="ea-send-delay" name="send_delay_secs" class="input" value="30" min="0" max="300">
                                    <div class="form-hint">Delay between successive responses</div>
                                </div>
                            </div>
                        </details>
                        <div id="ea-test-result" class="form-hint" style="margin-top:8px;"></div>
                    </form>
                </div>
                <div class="modal-footer">
                    <button type="button" class="btn btn-secondary ea-modal-cancel">Cancel</button>
                    <button type="button" id="btn-delete-email-account" class="btn btn-danger" style="display:none;">Delete</button>
                    <button type="button" id="btn-test-email-account" class="btn btn-secondary">Test IMAP</button>
                    <button type="submit" form="email-account-form" class="btn btn-primary">Save &amp; Enable</button>
                </div>
            </div>
        </div>"##,
        email_accounts_html = email_accounts_html,
    );

    let html = page_html("Account", "account", &body, &["account.js"]);
    Html(html)
}

// ═══════════════════════════════════════════════════════════════
// APPROVALS PAGE (P0-4)
// ═══════════════════════════════════════════════════════════════

async fn approvals_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let config = state.config.read().await;
    let level = format!("{:?}", config.permissions.approval.level).to_lowercase();
    drop(config);

    let body = format!(
        r#"
        <main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Approvals</h1>
                        <span class="badge badge-info" id="pending-count">0 pending</span>
                    </div>
                    <p class="page-desc">Manage command approval workflow for shell commands</p>
                </div>

                <div class="content-grid">
                    <!-- Approval Configuration -->
                    <section class="card">
                        <div class="card-header">
                        <h2>Configuration</h2>
                        </div>
                        <div class="card-body">
                        <div class="form-group">
                            <label>Autonomy Level</label>
                            <select id="approval-level" class="input">
                                <option value="full" {full_selected}>Full - No approval required</option>
                                <option value="supervised" {supervised_selected}>Supervised - Ask for unknown commands</option>
                                <option value="readonly" {readonly_selected}>ReadOnly - Ask for all commands</option>
                            </select>
                        </div>
                        <div class="form-group" style="margin-top:1rem">
                            <label>Auto-approve commands (comma-separated)</label>
                            <input type="text" id="auto-approve-list" class="input" placeholder="ls, cat, pwd">
                        </div>
                        <div class="form-group" style="margin-top:1rem">
                            <label>Always ask for (comma-separated)</label>
                            <input type="text" id="always-ask-list" class="input" placeholder="rm, sudo, chmod">
                        </div>
                        <button id="save-approval-config" class="btn btn-primary btn-sm" style="margin-top:1rem">Save Configuration</button>
                    </div>
                </section>

                    <!-- Pending Approvals -->
                    <section class="card">
                        <div class="card-header">
                            <h2>Pending Approvals</h2>
                            <span class="badge" id="pending-approvals-count">0</span>
                        </div>
                        <div class="card-body">
                            <div id="pending-approvals-list" class="scrollable-list">
                                <div class="empty-state">
                                    <p>No pending approvals</p>
                                    <p class="muted">Commands requiring approval will appear here</p>
                                </div>
                            </div>
                        </div>
                    </section>

                    <!-- Audit Log -->
                    <section class="card">
                        <div class="card-header">
                            <h2>Recent Activity</h2>
                        </div>
                        <div class="card-body">
                            <div id="approval-audit-log" class="scrollable-list">
                                <div class="empty-state">
                                    <p>No activity yet</p>
                                </div>
                            </div>
                        </div>
                    </section>
                </div>
            </div>
        </main>"#,
        full_selected = if level == "full" { "selected" } else { "" },
        supervised_selected = if level == "supervised" {
            "selected"
        } else {
            ""
        },
        readonly_selected = if level == "readonly" { "selected" } else { "" },
    );

    let html = page_html("Approvals", "approvals", &body, &["approvals.js"]);
    Html(html)
}
