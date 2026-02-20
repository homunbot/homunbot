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
        .route("/logs", get(logs_page))
}

// ─── Shared layout pieces ───────────────────────────────────────

/// SVG icons used in the sidebar nav
const ICON_DASHBOARD: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="1" y="1" width="7" height="7" rx="1.5"/><rect x="10" y="1" width="7" height="4" rx="1.5"/><rect x="1" y="10" width="7" height="4" rx="1.5" transform="translate(0,3)"/><rect x="10" y="7" width="7" height="7" rx="1.5" transform="translate(0,3)"/></svg>"#;
const ICON_CHAT: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12.5V3.5A1.5 1.5 0 0 1 3.5 2h11A1.5 1.5 0 0 1 16 3.5v7a1.5 1.5 0 0 1-1.5 1.5H6L2 16V12.5z"/><line x1="6" y1="6" x2="12" y2="6"/><line x1="6" y1="9" x2="10" y2="9"/></svg>"#;
const ICON_SKILLS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 1L11.5 6.5 17 7.5 13 11.5 14 17 9 14.5 4 17 5 11.5 1 7.5 6.5 6.5z"/></svg>"#;
const ICON_SETTINGS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="9" r="2.5"/><path d="M14.7 11.1a1.2 1.2 0 0 0 .24 1.32l.04.04a1.44 1.44 0 1 1-2.04 2.04l-.04-.04a1.2 1.2 0 0 0-1.32-.24 1.2 1.2 0 0 0-.72 1.08v.12a1.44 1.44 0 0 1-2.88 0v-.06a1.2 1.2 0 0 0-.78-1.08 1.2 1.2 0 0 0-1.32.24l-.04.04a1.44 1.44 0 1 1-2.04-2.04l.04-.04a1.2 1.2 0 0 0 .24-1.32 1.2 1.2 0 0 0-1.08-.72h-.12a1.44 1.44 0 0 1 0-2.88h.06a1.2 1.2 0 0 0 1.08-.78 1.2 1.2 0 0 0-.24-1.32l-.04-.04a1.44 1.44 0 1 1 2.04-2.04l.04.04a1.2 1.2 0 0 0 1.32.24h.06a1.2 1.2 0 0 0 .72-1.08V2.88a1.44 1.44 0 0 1 2.88 0v.06a1.2 1.2 0 0 0 .72 1.08 1.2 1.2 0 0 0 1.32-.24l.04-.04a1.44 1.44 0 1 1 2.04 2.04l-.04.04a1.2 1.2 0 0 0-.24 1.32v.06a1.2 1.2 0 0 0 1.08.72h.12a1.44 1.44 0 0 1 0 2.88h-.06a1.2 1.2 0 0 0-1.08.72z"/></svg>"#;
const ICON_LOGS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 15V3a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v12"/><line x1="6" y1="6" x2="12" y2="6"/><line x1="6" y1="9" x2="12" y2="9"/><line x1="6" y1="12" x2="9" y2="12"/></svg>"#;

/// Channel icons — minimal stroke SVGs for dashboard/settings
const ICON_WEB: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="9" r="7.5"/><path d="M1.5 9h15"/><path d="M9 1.5a11.5 11.5 0 0 1 3 7.5 11.5 11.5 0 0 1-3 7.5"/><path d="M9 1.5a11.5 11.5 0 0 0-3 7.5 11.5 11.5 0 0 0 3 7.5"/></svg>"#;
const ICON_TELEGRAM: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M15.5 2.5L1.5 8l5 2m9-7.5L6.5 10m9-7.5l-3 13-5.5-5.5"/><path d="M6.5 10v4.5l2.5-2.5"/></svg>"#;
const ICON_DISCORD: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6.5 3C5 3 3 3.5 2 5c-1.5 3-.5 7.5 1 9.5.5.5 1.5 1.5 3 1.5s2-1 3-1 1.5 1 3 1 2.5-1 3-1.5c1.5-2 2.5-6.5 1-9.5-1-1.5-3-2-4.5-2"/><circle cx="6.5" cy="10" r="1"/><circle cx="11.5" cy="10" r="1"/></svg>"#;
const ICON_PHONE: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="1" width="10" height="16" rx="2"/><line x1="9" y1="14" x2="9" y2="14"/></svg>"#;

/// Logo icon — references the homun brand PNG (includes name + mascot).
const LOGO_ICON: &str = r#"<img class="logo-icon" src="/static/img/logo.png" alt="HOMUN">"#;


/// Build the sidebar navigation HTML
fn sidebar(active: &str) -> String {
    let nav_items = [
        ("dashboard", "/", "Dashboard", ICON_DASHBOARD),
        ("chat", "/chat", "Chat", ICON_CHAT),
        ("skills", "/skills", "Skills", ICON_SKILLS),
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

    if provider == "none" {
        return axum::response::Redirect::to("/setup").into_response();
    }

    let skills_count = crate::skills::SkillInstaller::list_installed()
        .await
        .map(|s| s.len())
        .unwrap_or(0);

    let channels_html = build_channels_html(&config);
    let uptime_display = format_uptime(uptime);
    let channel_count = count_active_channels(&config);

    let body = format!(
        r#"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Dashboard</h1>
                        <span class="badge badge-success">Running</span>
                    </div>
                </div>

                <div class="stats-grid">
                    <div class="stat-card" data-editable data-key="agent.model">
                        <div class="stat-label">Model</div>
                        <div class="stat-value">{model}</div>
                        <div class="stat-sub">via {provider}</div>
                        <div class="inline-edit">
                            <input type="text" class="inline-input" value="{model}">
                            <div class="inline-actions">
                                <button class="btn btn-save btn-sm">Save</button>
                                <button class="btn btn-cancel btn-sm">Cancel</button>
                            </div>
                        </div>
                    </div>
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
    );

    Html(page_html("Dashboard", "dashboard", &body, &["dashboard.js"])).into_response()
}

// ─── Settings ───────────────────────────────────────────────────

async fn setup_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let config = state.config.read().await;
    let providers_html = build_providers_html(&config);

    let body = format!(
        r#"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Settings</h1>
                    </div>
                </div>

                <section class="section">
                    <h2>Agent Configuration</h2>
                    <form class="form" id="agent-form">
                        <div class="form-group model-selector-section">
                            <label class="model-selector-label">Model</label>
                            <div class="model-select-wrap" id="model-select-wrap">
                                <select id="model-select" class="input">
                                    <option value="">Loading models…</option>
                                </select>
                                <input type="text" id="model-custom" class="input model-custom-input" placeholder="e.g. ollama/my-model:latest">
                            </div>
                            <input type="hidden" name="model" id="model-value" value="{model}">
                            <div class="form-hint">Select a model from configured providers, or choose "Custom model…" to type one.</div>
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

                        <!-- Ollama Models Selector -->
                        <div class="form-group" id="ollama-models-group" style="display:none;">
                            <label>Available Models</label>
                            <div id="ollama-models-loading" class="form-hint">Loading models...</div>
                            <div id="ollama-models-error" class="form-hint" style="color: var(--err);"></div>
                            <select id="ollama-model-select" name="model" class="input" style="display:none;">
                                <option value="">Select a model...</option>
                            </select>
                            <button type="button" id="refresh-ollama-models" class="btn btn-secondary btn-sm" style="margin-top:8px;">Refresh Models</button>
                        </div>

                        <div class="modal-actions">
                            <button type="button" class="btn btn-secondary modal-cancel">Cancel</button>
                            <button type="submit" class="btn btn-primary">Save Configuration</button>
                            <button type="button" id="btn-activate" class="btn btn-success" style="display:none;">Activate Provider</button>
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
        providers_html = providers_html,
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
                </div>
                <div class="chat-messages" id="messages"></div>
                <form class="chat-input" id="chat-form">
                    <input type="text" id="chat-text" placeholder="Send a message…" autocomplete="off" class="input">
                    <button type="submit" class="btn btn-primary">Send</button>
                </form>
            </div>
        </main>"#;

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
                } else {
                    "github"
                };
                format!(
                    r#"<div class="skill-card" data-skill-name="{name}">
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
                    source_label = if source == "clawhub" { "ClawHub" } else { "GitHub" },
                )
            })
            .collect()
    };

    let body = format!(
        r#"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Skills</h1>
                        <span class="badge badge-info" id="installed-count">{count} installed</span>
                    </div>
                </div>

                <div class="skills-search">
                    <svg class="skills-search-icon" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5"><circle cx="7.5" cy="7.5" r="5.5"/><path d="M12 12l4.5 4.5"/></svg>
                    <input type="text" id="skill-search-input" class="input skills-search-input" placeholder="Search ClawHub &amp; GitHub, or enter owner/repo to install..." autocomplete="off">
                    <div class="skills-search-spinner" id="search-spinner" style="display:none"></div>
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
            </div>
        </main>"#,
        count = installed.len(),
        installed_html = installed_html,
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
    let active = config
        .resolve_provider(&config.agent.model)
        .map(|(n, _)| n.to_string());

    /// Provider display metadata: (display_name, description, needs_api_key, needs_base_url)
    fn get_provider_meta(name: &str) -> (&'static str, &'static str, bool, bool) {
        match name {
            "anthropic" => ("Anthropic", "Claude API (claude-3.5-sonnet, etc.)", true, true),
            "openai" => ("OpenAI", "GPT-4, GPT-4o, GPT-3.5", true, true),
            "openrouter" => ("OpenRouter", "Access to 100+ models via unified API", true, true),
            "ollama" => ("Ollama", "Run models locally (llama3, mistral, etc.)", false, true),
            "gemini" => ("Google Gemini", "Gemini 1.5 Pro, Gemini 2.0 Flash", true, true),
            "deepseek" => ("DeepSeek", "DeepSeek Chat, DeepSeek Coder", true, true),
            "groq" => ("Groq", "Fast inference (llama, mixtral)", true, true),
            "moonshot" => ("Moonshot", "Moonshot AI models", true, true),
            "zhipu" => ("Zhipu AI", "GLM models (Chinese)", true, true),
            "dashscope" => ("DashScope", "Alibaba Qwen models", true, true),
            "aihubmix" => ("AiHubMix", "Multi-model aggregator", true, true),
            "minimax" => ("MiniMax", "MiniMax AI models", true, true),
            "vllm" => ("vLLM", "Self-hosted vLLM server", false, true),
            "custom" => ("Custom", "Any OpenAI-compatible API", false, true),
            _ => ("Unknown", "Unknown provider", true, true),
        }
    }

    config
        .providers
        .iter()
        .map(|(name, pc)| {
            let configured = config.is_provider_configured(name);
            let is_active = active.as_deref() == Some(name);
            let is_default = is_active;

            let (display_name, description, has_key, has_url) = get_provider_meta(name);
            let is_ollama = name == "ollama";

            // Build CSS class list for the card
            let mut card_classes = String::from("provider-card");
            if configured {
                card_classes.push_str(" is-configured");
            }
            if is_default {
                card_classes.push_str(" is-default");
            }

            // API key mask — check encrypted storage first, then plaintext
            let api_key_mask = if configured && has_key {
                // Don't leak actual key content; just show it's present
                "••••••••".to_string()
            } else {
                String::new()
            };

            // Default badge OR "set default" link
            let default_badge = if is_default {
                r#"<span class="provider-default-badge">Default</span>"#
            } else if configured {
                r#"<a class="provider-set-default" href="javascript:void(0)">Set default</a>"#
            } else {
                ""
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
                            {default_badge}
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
