use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use super::server::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(chat_page))
        .route("/dashboard", get(dashboard))
        .route("/setup", get(setup_page))
        .route("/appearance", get(appearance_page))
        .route("/channels", get(channels_page))
        .route("/browser", get(browser_page))
        .route("/chat", get(chat_page))
        .route("/automations", get(automations_page))
        .route("/workflows", get(workflows_page))
        .route("/skills", get(skills_page))
        .route("/mcp", get(mcp_page))
        .route(
            "/mcp/oauth/google/callback",
            get(mcp_google_oauth_callback_page),
        )
        .route(
            "/mcp/oauth/github/callback",
            get(mcp_github_oauth_callback_page),
        )
        .route("/memory", get(memory_page))
        .route("/knowledge", get(knowledge_page))
        .route("/vault", get(vault_page))
        .route("/file-access", get(file_access_page))
        .route("/shell", get(shell_page))
        .route("/sandbox", get(sandbox_page))
        .route(
            "/permissions",
            get(|| async { axum::response::Redirect::permanent("/file-access") }),
        )
        .route("/approvals", get(approvals_page))
        .route("/account", get(account_page))
        .route("/logs", get(logs_page))
}

// ─── Shared layout pieces ───────────────────────────────────────

/// SVG icons used in the sidebar nav
const ICON_DASHBOARD: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="1" y="1" width="7" height="7" rx="1.5"/><rect x="10" y="1" width="7" height="4" rx="1.5"/><rect x="1" y="10" width="7" height="4" rx="1.5" transform="translate(0,3)"/><rect x="10" y="7" width="7" height="7" rx="1.5" transform="translate(0,3)"/></svg>"#;
const ICON_CHAT: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12.5V3.5A1.5 1.5 0 0 1 3.5 2h11A1.5 1.5 0 0 1 16 3.5v7a1.5 1.5 0 0 1-1.5 1.5H6L2 16V12.5z"/><line x1="6" y1="6" x2="12" y2="6"/><line x1="6" y1="9" x2="10" y2="9"/></svg>"#;
const ICON_AUTOMATIONS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="9" r="6.5"/><path d="M9 5.5v4l2.8 1.8"/><path d="M9 1v1.5M9 15.5V17M1 9h1.5M15.5 9H17"/></svg>"#;
const ICON_SKILLS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 1L11.5 6.5 17 7.5 13 11.5 14 17 9 14.5 4 17 5 11.5 1 7.5 6.5 6.5z"/></svg>"#;
const ICON_MCP: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 9h4"/><path d="M11 9h4"/><circle cx="9" cy="9" r="2.5"/><path d="M7.2 7.2l-2-2M10.8 10.8l2 2M10.8 7.2l2-2M7.2 10.8l-2 2"/></svg>"#;
const ICON_SETTINGS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="9" r="2.5"/><path d="M14.7 11.1a1.2 1.2 0 0 0 .24 1.32l.04.04a1.44 1.44 0 1 1-2.04 2.04l-.04-.04a1.2 1.2 0 0 0-1.32-.24 1.2 1.2 0 0 0-.72 1.08v.12a1.44 1.44 0 0 1-2.88 0v-.06a1.2 1.2 0 0 0-.78-1.08 1.2 1.2 0 0 0-1.32.24l-.04.04a1.44 1.44 0 1 1-2.04-2.04l.04-.04a1.2 1.2 0 0 0 .24-1.32 1.2 1.2 0 0 0-1.08-.72h-.12a1.44 1.44 0 0 1 0-2.88h.06a1.2 1.2 0 0 0 1.08-.78 1.2 1.2 0 0 0-.24-1.32l-.04-.04a1.44 1.44 0 1 1 2.04-2.04l.04.04a1.2 1.2 0 0 0 1.32.24h.06a1.2 1.2 0 0 0 .72-1.08V2.88a1.44 1.44 0 0 1 2.88 0v.06a1.2 1.2 0 0 0 .72 1.08 1.2 1.2 0 0 0 1.32-.24l.04-.04a1.44 1.44 0 1 1 2.04 2.04l-.04.04a1.2 1.2 0 0 0-.24 1.32v.06a1.2 1.2 0 0 0 1.08.72h.12a1.44 1.44 0 0 1 0 2.88h-.06a1.2 1.2 0 0 0-1.08.72z"/></svg>"#;
const ICON_LOGS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 15V3a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v12"/><line x1="6" y1="6" x2="12" y2="6"/><line x1="6" y1="9" x2="12" y2="9"/><line x1="6" y1="12" x2="9" y2="12"/></svg>"#;
const ICON_MEMORY: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 2v14"/><path d="M3 9h12"/><circle cx="9" cy="9" r="3"/><circle cx="9" cy="9" r="7"/></svg>"#;
const ICON_VAULT: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="5" width="14" height="11" rx="1.5"/><path d="M5 5V4a4 4 0 0 1 8 0v1"/><circle cx="9" cy="11" r="1.5"/><path d="M9 12.5V14"/></svg>"#;
const ICON_PERMISSIONS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="1" y="4" width="16" height="12" rx="1.5"/><circle cx="9" cy="10" r="2"/><path d="M5 4V3a4 4 0 0 1 8 0v1"/></svg>"#;
const ICON_APPROVALS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 1v4M9 13v4M1 9h4M13 9h4"/><circle cx="9" cy="9" r="3"/><path d="M6 9l2 2 4-4"/></svg>"#;
const ICON_ACCOUNT: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="6" r="3.5"/><path d="M3 17c0-3.5 2.5-6 6-6s6 2.5 6 6"/></svg>"#;
const ICON_LOGOUT: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 15H3.5A1.5 1.5 0 0 1 2 13.5v-9A1.5 1.5 0 0 1 3.5 3H6"/><path d="M12 12l4-3-4-3"/><path d="M16 9H7"/></svg>"#;
const ICON_KNOWLEDGE: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 3h5l2 2h7v10H2z"/><path d="M6 9h6"/><path d="M6 12h4"/></svg>"#;
const ICON_WORKFLOWS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="5" cy="4" r="2"/><circle cx="13" cy="4" r="2"/><circle cx="9" cy="14" r="2"/><path d="M5 6v2a3 3 0 0 0 3 3h1"/><path d="M13 6v2a3 3 0 0 1-3 3h-1"/></svg>"#;

/// Channel icons — minimal stroke SVGs for dashboard/settings
const ICON_WEB: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="9" r="7.5"/><path d="M1.5 9h15"/><path d="M9 1.5a11.5 11.5 0 0 1 3 7.5 11.5 11.5 0 0 1-3 7.5"/><path d="M9 1.5a11.5 11.5 0 0 0-3 7.5 11.5 11.5 0 0 0 3 7.5"/></svg>"#;
const ICON_TELEGRAM: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15.5 2.5L1.5 8l5 2m9-7.5L6.5 10m9-7.5l-3 13-5.5-5.5"/><path d="M6.5 10v4.5l2.5-2.5"/></svg>"#;
const ICON_DISCORD: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6.5 3C5 3 3 3.5 2 5c-1.5 3-.5 7.5 1 9.5.5.5 1.5 1.5 3 1.5s2-1 3-1 1.5 1 3 1 2.5-1 3-1.5c1.5-2 2.5-6.5 1-9.5-1-1.5-3-2-4.5-2"/><circle cx="6.5" cy="10" r="1"/><circle cx="11.5" cy="10" r="1"/></svg>"#;
const ICON_SLACK: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0z"/><path d="M9 6a1.5 1.5 0 1 1 0-3 1.5 1.5 0 0 1 0 3z"/><path d="M15 9a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0z"/><path d="M9 15a1.5 1.5 0 1 1 0-3 1.5 1.5 0 0 1 0 3z"/><path d="M6 6v3m0 3v3"/><path d="M12 6v3m0 3v3"/><path d="M6 6h3m3 0h3"/><path d="M6 12h3m3 0h3"/></svg>"#;
const ICON_PHONE: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="1" width="10" height="16" rx="2"/><line x1="9" y1="14" x2="9" y2="14"/></svg>"#;
const ICON_EMAIL: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="1" y="3" width="16" height="12" rx="2"/><path d="M1 5l8 5 8-5"/></svg>"#;

/// Logo icon — serves the SVG logotype via <img> tag.
const LOGO_ICON: &str = r#"<div class="logo-icon" title="HOMUN"></div>"#;

/// Tools icon — wrench/gear for the Tools flyout trigger.
const ICON_TOOLS: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 1.5a4.5 4.5 0 0 0-3.6 7.2L2 14.1 3.9 16l5.4-5.4A4.5 4.5 0 1 0 11 1.5z"/></svg>"#;

/// Emergency stop icon — octagon with square stop symbol.
const ICON_ESTOP: &str = r#"<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="6,1 12,1 17,6 17,12 12,17 6,17 1,12 1,6"/><rect x="6.5" y="6.5" width="5" height="5" rx="0.8"/></svg>"#;

/// Pages that belong to the "Tools" sub-navigation group.
const TOOLS_PAGES: &[&str] = &[
    "automations",
    "workflows",
    "skills",
    "mcp",
    "memory",
    "knowledge",
    "vault",
];
/// Pages that belong to the "Settings" sub-navigation group.
const SETTINGS_PAGES: &[&str] = &[
    "settings",
    "appearance",
    "channels",
    "browser",
    "file-access",
    "shell",
    "sandbox",
    "approvals",
    "logs",
];

/// Build the sidebar navigation HTML.
/// Renders 5 main icons + 2 static sub-navigation panels (Tools, Settings).
/// Sub-nav panels are shown/hidden purely server-side via `.is-open` class.
fn sidebar(active: &str) -> String {
    let a = |page: &str| -> &str {
        if active == page {
            " active"
        } else {
            ""
        }
    };

    let is_tools = TOOLS_PAGES.contains(&active);
    let is_settings = SETTINGS_PAGES.contains(&active);

    let tools_active = if is_tools { " active" } else { "" };
    let settings_active = if is_settings { " active" } else { "" };

    let tools_open = if is_tools { " is-open" } else { "" };
    let settings_open = if is_settings { " is-open" } else { "" };

    format!(
        r##"<nav class="sidebar">
            <div class="sidebar-header">
                <a href="/" class="logo-link">{logo}</a>
            </div>
            <div class="nav">
                <div class="nav-group nav-group-featured">
                    <a href="/chat" class="nav-link{chat_a}" data-label="Chat">
                        <span class="nav-icon">{ic_chat}</span>
                    </a>
                </div>
                <a href="/dashboard" class="nav-link{dash_a}" data-label="Dashboard">
                    <span class="nav-icon">{ic_dash}</span>
                </a>
                <a href="/automations" class="nav-link{tools_a}" data-label="Tools">
                    <span class="nav-icon">{ic_tools}</span>
                </a>
                <button type="button" class="nav-link nav-estop" id="nav-estop-btn" data-label="Stop" title="Emergency Stop">
                    <span class="nav-icon">{ic_estop}</span>
                </button>
            </div>
            <div class="nav-bottom">
                <a href="/account" class="nav-link{account_a}" data-label="Account">
                    <span class="nav-icon">{ic_account}</span>
                </a>
                <a href="/setup" class="nav-link{settings_a}" data-label="Settings">
                    <span class="nav-icon">{ic_settings}</span>
                </a>
                <button type="button" class="nav-link nav-logout" id="nav-logout-btn" data-label="Logout" title="Sign out" onclick="fetch('/api/auth/logout',{{method:'POST'}}).then(()=>location.href='/login')">
                    <span class="nav-icon">{ic_logout}</span>
                </button>
            </div>
            <div class="sidebar-subnav{tools_open}" id="tools-subnav">
                <div class="sidebar-subnav-header">Tools</div>
                <a href="/automations" class="sidebar-subnav-link{automations_a}">Automations</a>
                <a href="/workflows" class="sidebar-subnav-link{workflows_a}">Workflows</a>
                <a href="/skills" class="sidebar-subnav-link{skills_a}">Skills</a>
                <a href="/mcp" class="sidebar-subnav-link{mcp_a}">MCP Servers</a>
                <a href="/memory" class="sidebar-subnav-link{memory_a}">Memory</a>
                <a href="/knowledge" class="sidebar-subnav-link{knowledge_a}">Knowledge</a>
                <a href="/vault" class="sidebar-subnav-link{vault_a}">Vault</a>
            </div>
            <div class="sidebar-subnav{settings_open}" id="settings-subnav">
                <div class="sidebar-subnav-header">Settings</div>
                <a href="/setup" class="sidebar-subnav-link{setup_a}">Model &amp; Providers</a>
                <a href="/appearance" class="sidebar-subnav-link{appearance_a}">Appearance</a>
                <a href="/channels" class="sidebar-subnav-link{channels_a}">Channels</a>
                <a href="/browser" class="sidebar-subnav-link{browser_a}">Browser</a>
                <a href="/file-access" class="sidebar-subnav-link{file_access_a}">File Access</a>
                <a href="/shell" class="sidebar-subnav-link{shell_a}">Shell</a>
                <a href="/sandbox" class="sidebar-subnav-link{sandbox_a}">Sandbox</a>
                <a href="/approvals" class="sidebar-subnav-link{approvals_a}">Approvals</a>
                <a href="/logs" class="sidebar-subnav-link{logs_a}">Logs</a>
            </div>
        </nav>"##,
        logo = LOGO_ICON,
        // Main icons
        chat_a = a("chat"),
        dash_a = a("dashboard"),
        account_a = a("account"),
        tools_a = tools_active,
        settings_a = settings_active,
        ic_chat = ICON_CHAT,
        ic_dash = ICON_DASHBOARD,
        ic_account = ICON_ACCOUNT,
        ic_tools = ICON_TOOLS,
        ic_estop = ICON_ESTOP,
        ic_settings = ICON_SETTINGS,
        ic_logout = ICON_LOGOUT,
        // Tools subnav
        tools_open = tools_open,
        automations_a = a("automations"),
        workflows_a = a("workflows"),
        skills_a = a("skills"),
        mcp_a = a("mcp"),
        memory_a = a("memory"),
        knowledge_a = a("knowledge"),
        vault_a = a("vault"),
        // Settings subnav
        settings_open = settings_open,
        setup_a = a("settings"),
        appearance_a = a("appearance"),
        channels_a = a("channels"),
        browser_a = a("browser"),
        file_access_a = a("file-access"),
        shell_a = a("shell"),
        sandbox_a = a("sandbox"),
        approvals_a = a("approvals"),
        logs_a = a("logs"),
    )
}

/// HTML document skeleton
fn page_html(title: &str, active: &str, body: &str, scripts: &[&str]) -> String {
    let sidebar_html = sidebar(active);
    let script_tags: String = scripts
        .iter()
        .map(|s| format!(r#"<script src="/static/js/{s}"></script>"#))
        .collect::<String>();

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
        const accent = localStorage.getItem('homun-accent') || 'moss';
        const configuredLanguage = localStorage.getItem('homun-language') || 'system';
        const resolvedLanguage = configuredLanguage === 'system'
            ? ((navigator.language || 'en').split('-')[0] || 'en')
            : configuredLanguage;
        document.documentElement.lang = resolvedLanguage;
        if (theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)) {{
            document.documentElement.classList.add('dark');
        }}
        if (accent && accent.startsWith('#')) {{
            // Custom color — derive accent family inline to avoid flash
            var h, s, l;
            (function(hex) {{
                var r = parseInt(hex.slice(1,3),16)/255, g = parseInt(hex.slice(3,5),16)/255, b = parseInt(hex.slice(5,7),16)/255;
                var mx = Math.max(r,g,b), mn = Math.min(r,g,b); l = (mx+mn)/2;
                if (mx===mn) {{ h=s=0; }} else {{
                    var d=mx-mn; s = l>0.5 ? d/(2-mx-mn) : d/(mx+mn);
                    if (mx===r) h=((g-b)/d+(g<b?6:0))/6;
                    else if (mx===g) h=((b-r)/d+2)/6;
                    else h=((r-g)/d+4)/6;
                }}
                h=Math.round(h*360); s=Math.round(s*100); l=Math.round(l*100);
            }})(accent);
            function hx(hh,ss,ll) {{
                ss/=100; ll/=100; var a=ss*Math.min(ll,1-ll);
                function f(n) {{ var k=(n+hh/30)%12; return Math.round(255*(ll-a*Math.max(Math.min(k-3,9-k,1),-1))).toString(16).padStart(2,'0'); }}
                return '#'+f(0)+f(8)+f(4);
            }}
            var isDk = document.documentElement.classList.contains('dark');
            var st = document.documentElement.style;
            st.setProperty('--accent', accent);
            st.setProperty('--accent-hover', hx(h, s, isDk ? Math.min(l+8,80) : Math.max(l-8,20)));
            st.setProperty('--accent-active', hx(h, s, isDk ? l : Math.max(l-14,15)));
            st.setProperty('--accent-light', hx(h, isDk ? Math.max(s-30,10) : Math.min(s+5,40), isDk ? 18 : 90));
            st.setProperty('--accent-border', hx(h, isDk ? Math.max(s-15,15) : Math.min(s,35), isDk ? 30 : 75));
            st.setProperty('--accent-text', isDk ? hx(h, Math.min(s+10,100), Math.min(l+15,85)) : accent);
            st.setProperty('--focus-ring', hx(h, Math.min(s+5,60), isDk ? Math.min(l+10,70) : Math.min(l+10,55)));
            st.setProperty('--selection-bg', hx(h, isDk ? 20 : 25, isDk ? 22 : 82));
            st.setProperty('--chart-primary', accent);
        }} else if (accent && accent !== 'moss') {{
            document.documentElement.setAttribute('data-accent', accent);
        }}
    }})();
    </script>
</head>
<body>
    <div class="app">
        {sidebar_html}
        {body}
    </div>
    <div class="estop-modal-backdrop" id="estop-modal" hidden>
        <div class="estop-modal" role="dialog" aria-modal="true">
            <div class="estop-modal-icon">&#x26D4;</div>
            <h3 class="estop-modal-title">Emergency Stop</h3>
            <p class="estop-modal-copy">This will immediately:</p>
            <ul class="estop-modal-list">
                <li>Stop the agent loop</li>
                <li>Take the network offline</li>
                <li>Close the browser</li>
                <li>Shut down MCP servers</li>
                <li>Cancel all subagents</li>
            </ul>
            <div class="estop-modal-actions">
                <button type="button" class="btn btn-ghost btn-sm" id="estop-modal-cancel">Cancel</button>
                <button type="button" class="btn estop-modal-confirm" id="estop-modal-confirm">Stop Everything</button>
            </div>
        </div>
    </div>
    {script_tags}
    <script>
    (function() {{
        var btn = document.getElementById('nav-estop-btn');
        var modal = document.getElementById('estop-modal');
        var confirmBtn = document.getElementById('estop-modal-confirm');
        var cancelBtn = document.getElementById('estop-modal-cancel');
        if (!btn || !modal) return;

        function showModal() {{ modal.hidden = false; }}
        function hideModal() {{ modal.hidden = true; }}

        btn.addEventListener('click', showModal);
        cancelBtn.addEventListener('click', hideModal);
        modal.addEventListener('click', function(e) {{
            if (e.target === modal) hideModal();
        }});

        confirmBtn.addEventListener('click', async function() {{
            hideModal();
            btn.disabled = true;
            btn.classList.add('is-stopped');
            confirmBtn.disabled = true;
            confirmBtn.textContent = 'Stopping\u2026';
            try {{
                var res = await fetch('/api/v1/emergency-stop', {{ method: 'POST' }});
                var report = await res.json();
                var parts = [];
                if (report.browser_closed) parts.push('Browser closed');
                if (report.mcp_shutdown) parts.push('MCP shut down');
                if (report.subagents_cancelled > 0) parts.push(report.subagents_cancelled + ' subagents cancelled');
                parts.push('Network offline');
                confirmBtn.textContent = 'Stopped';
                // Show brief toast-style feedback
                var toast = document.createElement('div');
                toast.className = 'estop-toast';
                toast.textContent = 'Emergency Stop: ' + parts.join(', ');
                document.body.appendChild(toast);
                setTimeout(function() {{ toast.remove(); }}, 5000);
            }} catch (e) {{
                btn.disabled = false;
                btn.classList.remove('is-stopped');
                confirmBtn.disabled = false;
                confirmBtn.textContent = 'Stop Everything';
            }}
        }});
    }})();
    </script>
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
                    <button class="btn estop-btn" id="estop-btn" title="Emergency Stop — kill all agent activity">
                        &#x26A0; Emergency Stop
                    </button>
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

                <section class="section setup-wizard-section" id="setup-wizard-section">
                    <h2>Guided Setup</h2>
                    <div class="setup-wizard" id="setup-wizard">
                        <div class="setup-wizard-steps">
                            <div class="setup-step" id="wizard-step-provider">
                                <span class="setup-step-dot"></span>
                                <div class="setup-step-content">
                                    <div class="setup-step-title">1. Configure a provider</div>
                                    <div class="setup-step-desc">Add API key or base URL for at least one provider.</div>
                                </div>
                            </div>
                            <div class="setup-step" id="wizard-step-model">
                                <span class="setup-step-dot"></span>
                                <div class="setup-step-content">
                                    <div class="setup-step-title">2. Select your active model</div>
                                    <div class="setup-step-desc">Choose a model from the provider card or enter a custom one.</div>
                                </div>
                            </div>
                            <div class="setup-step" id="wizard-step-test">
                                <span class="setup-step-dot"></span>
                                <div class="setup-step-content">
                                    <div class="setup-step-title">3. Run a connection test</div>
                                    <div class="setup-step-desc">Validate that provider credentials and endpoint work.</div>
                                </div>
                            </div>
                        </div>
                        <div class="setup-wizard-actions">
                            <button type="button" class="btn btn-primary btn-sm" id="wizard-next-step">Next step</button>
                            <button type="button" class="btn btn-secondary btn-sm" id="wizard-test-active-provider">Test active provider</button>
                            <button type="button" class="btn btn-ghost btn-sm" id="wizard-hide">Hide</button>
                        </div>
                        <div class="form-hint" id="wizard-status">Wizard ready.</div>
                    </div>
                </section>

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

            </div>
        </main>

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
        providers_html = providers_html,
        catalog_modal_html = catalog_modal_html,
    );

    Html(page_html("Settings", "settings", &body, &["setup.js"]))
}

// ─── Appearance ────────────────────────────────────────────────

async fn appearance_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let config = state.config.read().await;

    let body = format!(
        r##"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Appearance</h1>
                    </div>
                </div>

                <section class="section" id="section-theme">
                    <form class="form" id="appearance-form">
                        <div class="form-row--2">
                            <div class="form-group">
                                <label>Theme</label>
                                <select name="theme" class="input" id="theme-select">
                                    <option value="system" {theme_system}>System</option>
                                    <option value="light" {theme_light}>Light</option>
                                    <option value="dark" {theme_dark}>Dark</option>
                                </select>
                                <div class="form-hint">Choose the interface color scheme.</div>
                            </div>
                            <div class="form-group">
                                <label>Assistant Language</label>
                                <select name="language" class="input" id="language-select">
                                    <option value="system" {language_system}>System</option>
                                    <option value="it" {language_it}>Italiano</option>
                                    <option value="en" {language_en}>English</option>
                                </select>
                                <div class="form-hint">Used for guided explanations and assistant text in the Web UI.</div>
                            </div>
                        </div>
                        <div class="form-group" style="margin-top:16px;">
                            <label>Accent Color</label>
                            <div class="accent-picker" id="accent-picker">
                                <button type="button" class="accent-swatch" data-accent="moss" title="Moss (default)"><span style="background:#628A4A"></span></button>
                                <button type="button" class="accent-swatch" data-accent="terracotta" title="Terracotta"><span style="background:#B85C38"></span></button>
                                <button type="button" class="accent-swatch" data-accent="plum" title="Plum"><span style="background:#7A5C68"></span></button>
                                <button type="button" class="accent-swatch" data-accent="stone" title="Stone"><span style="background:#7A7268"></span></button>
                                <label class="accent-swatch accent-custom-label" title="Custom color">
                                    <input type="color" id="accent-custom-input" value="#628A4A">
                                    <span class="accent-custom-preview"></span>
                                </label>
                            </div>
                            <div class="form-hint">Choose a preset or pick your own accent color.</div>
                        </div>
                        <button type="submit" class="btn btn-primary">Save Appearance</button>
                    </form>
                </section>

                <div id="appearance-toast"></div>
            </div>
        </main>"##,
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
        language_system = if config.ui.language == "system" {
            "selected"
        } else {
            ""
        },
        language_it = if config.ui.language == "it" {
            "selected"
        } else {
            ""
        },
        language_en = if config.ui.language == "en" {
            "selected"
        } else {
            ""
        },
    );

    Html(page_html(
        "Appearance",
        "appearance",
        &body,
        &["appearance.js"],
    ))
}

// ─── Channels ──────────────────────────────────────────────────

async fn channels_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let config = state.config.read().await;
    let channels_html = build_channels_cards_html(&config);

    let body = format!(
        r##"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Channels</h1>
                    </div>
                </div>

                <section class="section" id="section-channels">
                    <div class="provider-grid" id="channel-grid">
                        {channels_html}
                    </div>
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
        </div>"##,
        channels_html = channels_html,
    );

    Html(page_html("Channels", "channels", &body, &["setup.js"]))
}

// ─── Browser ──────────────────────────────────────────────────

async fn browser_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let config = state.config.read().await;

    let body = format!(
        r##"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Browser Automation</h1>
                    </div>
                </div>

                <section class="section" id="section-browser">
                    <div class="form-hint" style="margin-bottom:12px;">{browser_status}</div>
                    <form class="form" id="browser-form">
                        <div class="setting-toggle-row">
                            <div class="setting-toggle-info">
                                <span class="setting-toggle-name">Enable Browser</span>
                                <span class="setting-toggle-desc">Register the browser tool and use it for dynamic web tasks</span>
                            </div>
                            <div class="toggle-wrap">
                                <input type="checkbox" id="browser-enabled" name="enabled" class="toggle-input" {browser_enabled_checked}>
                                <label class="toggle-label" for="browser-enabled"></label>
                            </div>
                        </div>
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
                        <div class="form-hint" style="margin-top:10px;">Timeouts and snapshot settings are managed by the Playwright MCP server.</div>
                        <div class="form-actions">
                            <button type="submit" class="btn btn-primary">Save Browser Config</button>
                            <button type="button" class="btn btn-secondary" id="btn-test-browser">Test Connection</button>
                        </div>
                        <div id="browser-result" class="form-hint" style="margin-top:10px;"></div>
                    </form>
                </section>

                <section class="section" id="section-web-search">
                    <h2>Web Search</h2>
                    <form class="form" id="web-search-form">
                        <div class="form-row--2">
                            <div class="form-group">
                                <label>Search Provider</label>
                                <select id="search-provider" name="provider" class="input">
                                    <option value="brave" {search_brave}>Brave Search</option>
                                    <option value="tavily" {search_tavily}>Tavily</option>
                                </select>
                                <div class="form-hint">Search engine used by the web_search tool.</div>
                            </div>
                            <div class="form-group">
                                <label>Max Results</label>
                                <input type="number" id="search-max-results" name="max_results" value="{search_max_results}" min="1" max="20" class="input">
                                <div class="form-hint">Number of search results returned per query.</div>
                            </div>
                        </div>
                        <div class="form-group">
                            <label>API Key</label>
                            <input type="password" id="search-api-key" name="api_key" value="{search_api_key}" class="input" placeholder="Enter your search API key">
                            <div class="form-hint">Brave: <a href="https://api-dashboard.search.brave.com/app/keys" target="_blank">api-dashboard.search.brave.com</a> · Tavily: <a href="https://tavily.com" target="_blank">tavily.com</a></div>
                        </div>
                        <div class="form-actions">
                            <button type="submit" class="btn btn-primary">Save Search Config</button>
                        </div>
                    </form>
                </section>
            </div>
        </main>"##,
        browser_enabled_checked = if config.browser.enabled {
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
        search_brave = if config.tools.web_search.provider == "brave" {
            "selected"
        } else {
            ""
        },
        search_tavily = if config.tools.web_search.provider == "tavily" {
            "selected"
        } else {
            ""
        },
        search_api_key = config.tools.web_search.api_key,
        search_max_results = config.tools.web_search.max_results,
        browser_status = {
            let status = config.browser.runtime_status();
            let enabled = if status.enabled {
                "Enabled"
            } else {
                "Disabled"
            };
            let availability = if status.available {
                "available"
            } else {
                "unavailable"
            };
            let executable = status
                .executable_path
                .map(|path| format!("Chrome: {}", path))
                .unwrap_or_else(|| "Chrome: not detected".to_string());
            match status.reason {
                Some(reason) => format!(
                    "{} • MCP (Playwright) • {}. {} {}",
                    enabled, availability, executable, reason
                ),
                None => format!(
                    "{} • MCP (Playwright) • {}. {}",
                    enabled, availability, executable
                ),
            }
        },
    );

    Html(page_html("Browser", "browser", &body, &["setup.js"]))
}

// ─── Chat ───────────────────────────────────────────────────────

async fn chat_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let config = state.config.read().await;
    let current_model = config.agent.model.clone();
    let current_vision_model = if config.agent.vision_model.trim().is_empty() {
        current_model.clone()
    } else {
        config.agent.vision_model.clone()
    };
    drop(config);

    let body = format!(
        r#"<main class="content chat-layout">
            <div class="content-inner">
                <div class="chat-shell">
                    <aside class="chat-sidebar">
                        <div class="chat-sidebar-header">
                            <span class="chat-sidebar-title">Conversations</span>
                            <div class="chat-sidebar-actions">
                                <button class="btn-icon" id="btn-chat-search" title="Search">
                                    <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="7.5" cy="7.5" r="5.5"/><path d="M12 12l4.5 4.5"/></svg>
                                </button>
                                <button class="btn-icon" id="btn-new-chat" title="New conversation">
                                    <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="9" y1="3" x2="9" y2="15"/><line x1="3" y1="9" x2="15" y2="9"/></svg>
                                </button>
                            </div>
                        </div>
                        <div class="chat-conversation-list" id="chat-conversation-list"></div>
                        <div class="chat-bulk-actions" id="chat-bulk-actions" hidden>
                            <span class="chat-bulk-count" id="chat-bulk-count">0 selected</span>
                            <div class="chat-bulk-buttons">
                                <button class="btn-icon" id="btn-bulk-archive" title="Archive selected">
                                    <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="14" height="3" rx="1"/><path d="M3 6v8a1 1 0 001 1h10a1 1 0 001-1V6"/><path d="M7 10h4"/></svg>
                                </button>
                                <button class="btn-icon is-danger" id="btn-bulk-delete" title="Delete selected">
                                    <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 5h12"/><path d="M7 5V3h4v2"/><path d="M5 5v10a1 1 0 001 1h6a1 1 0 001-1V5"/></svg>
                                </button>
                                <button class="btn-icon" id="btn-bulk-cancel" title="Cancel selection">
                                    <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="5" y1="5" x2="13" y2="13"/><line x1="13" y1="5" x2="5" y2="13"/></svg>
                                </button>
                            </div>
                        </div>
                    </aside>
                    <section class="chat-main">
                        <div class="chat-topbar">
                            <div class="chat-topbar-leading">
                                <div class="chat-topbar-meta">
                                    <div class="chat-topbar-title" id="chat-conversation-title">New conversation</div>
                                    <div class="chat-topbar-statusline">
                                        <span class="chat-connection" id="ws-status">Connecting…</span>
                                        <span class="chat-run-model" id="chat-run-model" hidden></span>
                                        <span class="chat-run-badge is-idle is-dot-only" id="chat-run-badge" aria-label="idle"></span>
                                    </div>
                                </div>
                            </div>
                            <div class="chat-actions">
                                <button class="btn btn-ghost btn-sm" id="btn-new-chat-topbar" title="New conversation">
                                <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="9" y1="3" x2="9" y2="15"/><line x1="3" y1="9" x2="15" y2="9"/></svg>
                                </button>
                                <button class="btn btn-ghost btn-sm" id="btn-clear-chat" title="Clear screen">
                                <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 4 9 9 14 4"/><polyline points="4 14 9 9 14 14"/></svg>
                                </button>
                                <button class="btn btn-ghost btn-sm chat-sidebar-toggle-btn" id="btn-chat-sidebar" title="Toggle sidebar" aria-label="Toggle sidebar">
                                    <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="14" height="12" rx="1.5"/><line x1="7" y1="3" x2="7" y2="15"/><line x1="10.5" y1="6" x2="13" y2="6"/><line x1="10.5" y1="9" x2="13" y2="9"/><line x1="10.5" y1="12" x2="13" y2="12"/></svg>
                                </button>
                            </div>
                        </div>
                        <div class="chat-thread-wrap">
                            <div class="chat-empty-state" id="chat-empty-state">
                                <div class="chat-empty-kicker">Homun is ready</div>
                                <h2>Ask, search, inspect tools, or connect services.</h2>
                                <p>The chat will show reasoning blocks, tool activity and formatted answers in one continuous workspace.</p>
                            </div>
                            <div class="chat-messages" id="messages"></div>
                        </div>
                        <div class="chat-composer-dock">
                            <section class="chat-plan-panel collapsed" id="chat-plan-panel" hidden>
                                <button type="button" class="chat-plan-header" id="chat-plan-toggle" aria-expanded="false">
                                    <span class="chat-plan-header-copy">
                                        <span class="chat-plan-status-icon">&#9776;</span>
                                        <span class="chat-plan-summary" id="chat-plan-summary"></span>
                                    </span>
                                    <span class="chat-plan-toggle-icon">›</span>
                                </button>
                                <ol class="chat-plan-tasklist" id="chat-plan-tasklist"></ol>
                            </section>
                            <div class="chat-composer-shell" id="chat-config" data-model="{current_model}" data-vision-model="{current_vision_model}">
                                <form class="chat-input" id="chat-form">
                                    <textarea id="chat-text" placeholder="Reply to Homun…" autocomplete="off" class="input chat-textarea" rows="1" autofocus></textarea>
                                    <div class="chat-attachment-strip" id="chat-attachment-strip" hidden></div>
                                    <div class="chat-input-bottom">
                                        <div class="chat-composer-footer">
                                            <div class="chat-model-selector">
                                                <div class="chat-model-pill" id="chat-model-pill">
                                                    <span class="chat-model-pill-name" id="chat-model-pill-name">{current_model}</span>
                                                    <svg class="chat-model-pill-arrow" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 7l4 4 4-4"/></svg>
                                                </div>
                                                <select id="chat-model-select" class="chat-model-select-hidden" aria-label="Model">
                                                    <option value="{current_model}">{current_model}</option>
                                                </select>
                                            </div>
                                            <div class="chat-model-capabilities" id="chat-model-capabilities" hidden></div>
                                        </div>
                                        <div class="chat-input-actions">
                                            <div class="chat-plus-wrap">
                                                <button type="button" class="chat-plus-btn" id="btn-chat-plus" title="Add attachments or services">+</button>
                                                <div class="chat-plus-menu" id="chat-plus-menu" hidden>
                                                    <button type="button" class="chat-plus-item" id="btn-chat-upload-image">Add image</button>
                                                    <button type="button" class="chat-plus-item" id="btn-chat-upload-doc">Add document</button>
                                                    <button type="button" class="chat-plus-item" id="btn-chat-open-mcp">Open MCP</button>
                                                </div>
                                                <div class="chat-mcp-picker" id="chat-mcp-picker" hidden>
                                                    <div class="chat-mcp-picker-header">
                                                        <input type="text" id="chat-mcp-search" class="input chat-mcp-search" placeholder="Search MCP servers" autocomplete="off">
                                                        <a href="/mcp" class="chat-mcp-manage-link">Manage</a>
                                                    </div>
                                                    <div class="chat-mcp-picker-list" id="chat-mcp-picker-list"></div>
                                                </div>
                                            </div>
                                            <button type="button" class="chat-send-btn" id="btn-send" aria-label="Send message">
                                                <span class="chat-send-spinner" aria-hidden="true"></span>
                                                <svg class="chat-send-icon chat-send-icon--send" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M3 9h9"/><path d="M9 3l6 6-6 6"/></svg>
                                                <svg class="chat-send-icon chat-send-icon--stop" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><rect x="5.5" y="5.5" width="7" height="7" rx="1.2"/></svg>
                                            </button>
                                        </div>
                                    </div>
                                </form>
                            </div>
                        </div>
                        <input type="file" id="chat-image-input" accept="image/*" multiple hidden>
                        <input type="file" id="chat-doc-input" accept=".pdf,.md,.txt,.doc,.docx" multiple hidden>
                        <div class="chat-modal-backdrop" id="chat-modal-backdrop" hidden>
                            <div class="chat-modal" role="dialog" aria-modal="true" aria-labelledby="chat-modal-title">
                                <div class="chat-modal-header">
                                    <h3 id="chat-modal-title">Confirm action</h3>
                                </div>
                                <p class="chat-modal-copy" id="chat-modal-copy"></p>
                                <div class="chat-modal-actions">
                                    <button type="button" class="btn btn-ghost btn-sm" id="chat-modal-cancel">Cancel</button>
                                    <button type="button" class="btn btn-primary btn-sm" id="chat-modal-confirm">Confirm</button>
                                </div>
                            </div>
                        </div>
                    </section>
                </div>
            </div>
        </main>
        <!-- Search Modal -->
        <div id="chat-search-modal" class="chat-search-modal" hidden>
            <div class="chat-search-modal-backdrop"></div>
            <div class="chat-search-modal-content">
                <div class="chat-search-modal-header">
                    <svg class="chat-search-modal-icon" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="7.5" cy="7.5" r="5.5"/><path d="M12 12l4.5 4.5"/></svg>
                    <input type="text" id="chat-search-input" class="chat-search-modal-input" placeholder="Search conversations…" autocomplete="off">
                    <button class="chat-search-modal-close" id="btn-chat-search-close">&times;</button>
                </div>
                <div class="chat-search-modal-options">
                    <label class="chat-search-option">
                        <input type="checkbox" id="chat-search-include-archived"> Show archived
                    </label>
                </div>
                <div class="chat-search-modal-results" id="chat-search-results"></div>
            </div>
        </div>
        <script src="https://cdn.jsdelivr.net/npm/marked/marked.min.js"></script>
        <script src="https://cdn.jsdelivr.net/npm/dompurify/dist/purify.min.js"></script>"#,
        current_model = current_model,
        current_vision_model = current_vision_model,
    );

    Html(page_html("Chat", "chat", &body, &["chat.js"]))
}

// ─── Automations ────────────────────────────────────────────────

async fn automations_page() -> Html<String> {
    let body = r#"<main class="content">
            <div class="content-inner" id="automations-list-view">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Automations</h1>
                        <span class="badge badge-info" id="automations-count">0</span>
                    </div>
                    <div class="actions">
                        <button class="btn btn-primary btn-sm" id="btn-create-automation">
                            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" style="width:14px;height:14px;vertical-align:-2px;margin-right:4px"><path d="M8 3v10M3 8h10"/></svg>Create Automation
                        </button>
                        <button class="btn btn-secondary btn-sm" id="btn-automations-refresh">Refresh</button>
                    </div>
                </div>

                <section class="section">
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

            <!-- N8N Style Builder View -->
            <div id="automations-builder-view" style="display: none; position: absolute; top: 0; left: 0; right: 0; bottom: 0; background: var(--surface); z-index: 50; flex-direction: column;">
                <div class="builder-header" style="height: 60px; border-bottom: 1px solid var(--border); display: flex; align-items: center; padding: 0 20px; justify-content: space-between; background: var(--surface);">
                    <div style="display: flex; align-items: center; gap: 15px;">
                        <button class="btn btn-secondary btn-sm" id="btn-builder-back">
                            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" style="width:14px;height:14px;vertical-align:-2px;margin-right:4px"><path d="M10 12L6 8l4-4"/></svg>Back
                        </button>
                        <input type="text" id="builder-automation-name" class="input" placeholder="My New Automation" style="width: 300px; border: 1px solid transparent; background: transparent; font-size: 16px; font-weight: 600; padding: 4px 8px;">
                    </div>
                    <div style="display: flex; align-items: center; gap: 10px;">
                        <span id="builder-status" style="font-size: 12px; color: var(--t4);"></span>
                        <button class="btn btn-primary" id="btn-builder-save">Save Automation</button>
                    </div>
                </div>
                
                <div class="builder-body" style="flex: 1; display: flex; overflow: hidden; position: relative;">
                    <!-- Node Palette (populated by JS from NODE_KINDS) -->
                    <div class="builder-palette" style="width: 240px; border-right: 1px solid var(--border); background: var(--surface); display: flex; flex-direction: column;">
                        <div style="padding: 15px; border-bottom: 1px solid var(--border);">
                            <h3 style="font-size: 11px; text-transform: uppercase; color: var(--t3); letter-spacing: 0.05em; margin: 0;">Add Node</h3>
                        </div>
                        <div id="builder-palette-items" style="padding: 10px; overflow-y: auto; flex: 1; display: flex; flex-direction: column; gap: 4px;">
                            <!-- Generated by Builder.buildPalette() -->
                        </div>
                    </div>

                    <!-- Canvas + Prompt bar wrapper -->
                    <div style="flex: 1; display: flex; flex-direction: column; overflow: hidden;">
                        <!-- Interactive Canvas -->
                        <div id="builder-canvas" style="flex: 1; position: relative; overflow: hidden; background-color: var(--bg-subtle); outline: none;" tabindex="0">
                            <svg width="100%" height="100%" style="position: absolute; top: 0; left: 0; pointer-events: none;">
                                <defs>
                                    <pattern id="builder-grid" width="20" height="20" patternUnits="userSpaceOnUse">
                                        <circle cx="2" cy="2" r="0.8" fill="var(--accent-border)" opacity="0.4" />
                                    </pattern>
                                </defs>
                                <rect width="100%" height="100%" fill="url(#builder-grid)" />
                                <g id="builder-canvas-edges"></g>
                            </svg>
                            <div id="builder-canvas-nodes" style="position: absolute; top: 0; left: 0; width: 0; height: 0; overflow: visible;"></div>
                        </div>

                        <!-- Prompt bar for natural language automation creation -->
                        <div class="builder-prompt-bar" id="builder-prompt-bar">
                            <div class="builder-prompt-inner">
                                <textarea id="builder-prompt-input" class="input" rows="1" placeholder="Describe your automation in natural language... (e.g. Every morning check Gmail, summarize, send to Telegram)"></textarea>
                                <button class="btn btn-primary btn-sm" id="btn-builder-generate">
                                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" style="width:14px;height:14px;vertical-align:-2px"><path d="M14 2L2 8.5l4.5 1.8L9 14.5z"/></svg>
                                    Generate
                                </button>
                            </div>
                            <div id="builder-prompt-status" class="builder-prompt-status"></div>
                        </div>
                    </div>

                    <!-- Inspector Panel -->
                    <div id="builder-inspector" style="width: 320px; border-left: 1px solid var(--border); background: var(--surface); display: none; flex-direction: column; z-index: 10;">
                        <div style="padding: 15px; border-bottom: 1px solid var(--border); display: flex; justify-content: space-between; align-items: center;">
                            <h3 id="inspector-title" style="font-size: 13px; font-weight: 600; color: var(--t1); margin: 0;">Properties</h3>
                            <button class="btn-icon" id="btn-inspector-close" style="background:none; border:none; color:var(--t4); cursor:pointer;">
                                <svg viewBox="0 0 16 16" width="14" height="14" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M3 3l10 10M13 3L3 13"/></svg>
                            </button>
                        </div>
                        <div id="inspector-body" style="padding: 15px; overflow-y: auto; flex: 1; display: flex; flex-direction: column; gap: 15px;">
                            <!-- Dynamically populated -->
                        </div>
                    </div>
                </div>
            </div>
        </main>"#;

    Html(page_html(
        "Automations",
        "automations",
        body,
        &["flow-renderer.js", "schema-form.js", "automations.js"],
    ))
}

// ─── Workflows ──────────────────────────────────────────────────

async fn workflows_page() -> Html<String> {
    let body = r#"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Workflows</h1>
                        <span class="badge badge-info" id="workflows-count">0</span>
                    </div>
                    <div class="actions">
                        <button class="btn btn-primary btn-sm" id="wf-create-toggle">
                            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" style="width:14px;height:14px;vertical-align:-2px;margin-right:4px"><path d="M8 3v10M3 8h10"/></svg>Create Workflow
                        </button>
                        <button class="btn btn-secondary btn-sm" id="btn-workflows-refresh">Refresh</button>
                    </div>
                </div>

                <section class="section wf-creator-panel" id="wf-creator-panel" style="display:none">
                    <h2>New Workflow</h2>
                    <p class="form-hint" style="margin-bottom:16px">Define an objective and break it into steps. Each step is executed sequentially by the agent.</p>
                    <form id="workflow-create-form" class="form form--full">
                        <div class="form-row--2">
                            <div class="form-group">
                                <label for="wf-name">Name</label>
                                <input id="wf-name" class="input" type="text" maxlength="120" placeholder="Research & Report">
                            </div>
                            <div class="form-group">
                                <label for="wf-deliver-to">Deliver To</label>
                                <select id="wf-deliver-to" class="input">
                                    <option value="web:web">Web UI</option>
                                </select>
                            </div>
                        </div>
                        <div class="form-group">
                            <label for="wf-objective">Objective</label>
                            <textarea id="wf-objective" class="input" rows="2" placeholder="Describe the high-level goal of this workflow."></textarea>
                            <div class="form-hint">The objective is shared with every step for context.</div>
                        </div>

                        <div class="form-group">
                            <label>Steps</label>
                            <div id="wf-steps-container"></div>
                            <button type="button" class="btn btn-secondary btn-sm" id="wf-add-step" style="margin-top:0.5rem;">+ Add Step</button>
                        </div>

                        <div class="form-actions">
                            <button class="btn btn-primary" type="submit">Create Workflow</button>
                            <button type="button" class="btn btn-secondary" id="wf-create-cancel">Cancel</button>
                        </div>
                    </form>
                </section>

                <div class="stats-grid" style="grid-template-columns:repeat(4,1fr)">
                    <div class="stat-card">
                        <div class="stat-label">Total</div>
                        <div class="stat-value" id="stat-total">0</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-label">Running</div>
                        <div class="stat-value" id="stat-running">0</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-label">Completed</div>
                        <div class="stat-value" id="stat-completed">0</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-label">Failed</div>
                        <div class="stat-value" id="stat-failed">0</div>
                    </div>
                </div>

                <section class="section">
                    <div id="workflows-list" class="item-list">
                        <div class="empty-state">
                            <p>Loading workflows...</p>
                        </div>
                    </div>
                </section>

                <section class="section" id="workflow-detail-section" style="display:none;">
                    <h2>Workflow Detail</h2>
                    <div id="workflow-detail"></div>
                </section>
            </div>
        </main>"#;

    Html(page_html("Workflows", "workflows", body, &["workflows.js"]))
}

// ─── Skills ─────────────────────────────────────────────────────

async fn skills_page() -> Html<String> {
    let installed = crate::skills::SkillInstaller::list_installed()
        .await
        .unwrap_or_default();

    let installed_html: String = if installed.is_empty() {
        r#"<div class="empty-state" id="installed-empty">
                <svg class="empty-state-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 2L15 8.5 22 9.5 17 14.5 18 22 12 19 6 22 7 14.5 2 9.5 9 8.5z"/></svg>
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
                    <button type="button" class="btn btn-primary btn-sm" id="create-skill-toggle-btn">
                        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" style="width:14px;height:14px;vertical-align:-2px;margin-right:4px"><path d="M8 3v10M3 8h10"/></svg>Create Skill
                    </button>
                </div>

                <section class="section skill-creator-panel" id="skill-creator-panel" style="display:none">
                    <h2>Create a New Skill</h2>
                    <p class="form-hint" style="margin-bottom:16px">Describe what the skill should do. Homun will generate the SKILL.md, script, run a security scan, and install it.</p>
                    <div class="form-group">
                        <label class="form-label" for="creator-prompt">What should this skill do?</label>
                        <textarea class="input" id="creator-prompt" rows="3" placeholder="e.g. Check disk space and report usage in a formatted table"></textarea>
                    </div>
                    <div class="form-row--2">
                        <div class="form-group">
                            <label class="form-label" for="creator-name">Skill name <span class="form-hint">(optional, auto-generated)</span></label>
                            <input class="input" id="creator-name" type="text" placeholder="my-skill-name">
                        </div>
                        <div class="form-group">
                            <label class="form-label" for="creator-language">Script language</label>
                            <select class="input" id="creator-language">
                                <option value="">Auto-detect</option>
                                <option value="python">Python</option>
                                <option value="bash">Bash</option>
                                <option value="javascript">JavaScript</option>
                            </select>
                        </div>
                    </div>
                    <div class="form-group">
                        <label class="checkbox-label"><input type="checkbox" id="creator-overwrite"> Replace existing skill with same name</label>
                    </div>
                    <div class="form-actions">
                        <button type="button" class="btn btn-primary btn-sm" id="creator-submit-btn">Create Skill</button>
                        <button type="button" class="btn btn-secondary btn-sm" id="creator-cancel-btn">Cancel</button>
                        <span class="skills-search-spinner" id="creator-spinner" style="display:none"></span>
                    </div>
                    <div id="creator-result" style="display:none"></div>
                </section>

                <section class="section">
                    <div class="mcp-sandbox-panel">
                        <div class="permission-mode-header" style="margin-bottom: 0.8rem;">
                            <span class="permission-mode-name">Execution Sandbox (Skill scripts)</span>
                            <span class="badge badge-neutral" id="skills-sandbox-runtime-badge">checking...</span>
                        </div>
                        <p class="form-hint" id="skills-sandbox-runtime-text">Checking sandbox runtime status...</p>
                        <div class="form-actions">
                            <button type="button" class="btn btn-secondary btn-sm" id="skills-refresh-sandbox-status-btn">Refresh Sandbox Status</button>
                            <a href="/permissions" class="btn btn-secondary btn-sm">Open Permissions</a>
                        </div>
                    </div>
                </section>

                <div class="skills-search">
                    <svg class="skills-search-icon" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2"><circle cx="7.5" cy="7.5" r="5.5"/><path d="M12 12l4.5 4.5"/></svg>
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

// ─── MCP ────────────────────────────────────────────────────────

async fn mcp_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let config = state.config.read().await;
    let ui_language = config.ui.language.clone();
    let body = format!(
        r#"<main class="content">
            <div class="content-inner">
                <input type="hidden" id="mcp-ui-language" value="{ui_language}">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">MCP Servers</h1>
                        <span class="badge badge-info" id="mcp-server-count">Loading...</span>
                    </div>
                    <div class="conn-view-toggle" id="conn-view-toggle">
                        <button class="conn-view-tab active" data-view="connections">Connect Services</button>
                        <button class="conn-view-tab" data-view="advanced">Advanced MCP</button>
                    </div>
                </div>

                <div id="connections-view">
                    <section class="section">
                        <div class="skills-search">
                            <svg class="skills-search-icon" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2"><circle cx="7.5" cy="7.5" r="5.5"/><path d="M12 12l4.5 4.5"/></svg>
                            <input type="text" id="conn-search-input" class="input skills-search-input" placeholder="Search services..." autocomplete="off">
                        </div>
                    </section>
                    <section class="section">
                        <div class="skills-results-header">
                            <h2>Services</h2>
                            <span class="badge badge-neutral" id="conn-count"></span>
                        </div>
                        <div id="conn-category-chips" class="mcp-category-chips"></div>
                        <div class="skill-list mcp-skill-list" id="conn-grid">
                            <div class="empty-state"><p>Loading services...</p></div>
                        </div>
                    </section>
                </div>

                <div id="mcp-advanced-view" style="display:none;">

                <section class="section">
                    <div class="mcp-sandbox-panel">
                        <div class="permission-mode-header" style="margin-bottom: 0.8rem;">
                            <span class="permission-mode-name">Execution Sandbox (MCP stdio)</span>
                            <span class="badge badge-neutral" id="mcp-sandbox-runtime-badge">checking...</span>
                        </div>
                        <p class="form-hint" id="mcp-sandbox-runtime-text">Checking sandbox runtime status...</p>
                        <div class="form-actions">
                            <button type="button" class="btn btn-secondary btn-sm" id="mcp-refresh-sandbox-status-btn">Refresh Sandbox Status</button>
                            <a href="/permissions" class="btn btn-secondary btn-sm">Open Permissions</a>
                        </div>
                    </div>
                </section>

                <section class="section">
                    <div class="skills-search mcp-search-shell">
                        <svg class="skills-search-icon" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2"><circle cx="7.5" cy="7.5" r="5.5"/><path d="M12 12l4.5 4.5"/></svg>
                        <input type="text" id="mcp-suggest-input" class="input skills-search-input" placeholder="Search MCP servers or describe what you need..." autocomplete="off">
                        <div class="skills-search-spinner" id="mcp-search-spinner" style="display:none"></div>
                    </div>
                    <div class="catalog-stats mcp-catalog-stats" id="mcp-suggest-status">Search the official MCP registry, then install from a guided template.</div>
                </section>

                <section class="section" id="mcp-configured-section" style="display:none;">
                    <div class="skills-results-header">
                        <h2>Configured Servers</h2>
                        <span class="badge badge-neutral" id="mcp-configured-count">Loading...</span>
                    </div>
                    <div class="skill-list mcp-skill-list" id="mcp-servers-list">
                        <div class="empty-state"><p>Loading servers...</p></div>
                    </div>
                </section>

                <section class="section skills-results-section">
                    <div class="skills-results-header">
                        <h2>Connect Services</h2>
                        <span class="badge badge-neutral" id="mcp-catalog-count"></span>
                    </div>
                    <div id="mcp-category-chips" class="mcp-category-chips"></div>
                    <div class="skill-list mcp-skill-list" id="mcp-catalog-grid">
                        <div class="empty-state"><p>Loading catalog...</p></div>
                    </div>
                </section>

                <section class="section" id="mcp-install-section">
                    <div class="skills-results-header">
                        <h2>Manual Installer</h2>
                        <button type="button" class="btn btn-secondary btn-sm" id="mcp-toggle-install-btn" aria-expanded="false">Open manual installer</button>
                    </div>
                    <div class="form-hint mcp-install-summary">Only needed for advanced/manual setup. Guided install opens directly from an MCP card.</div>
                    <div id="mcp-install-panel-home" style="display:none;">
                        <div id="mcp-install-panel" class="mcp-install-shell">
                            <div class="mcp-install-intro">
                                <div class="mcp-install-intro-copy">
                                    <div class="mcp-install-intro-title">Reusable installer</div>
                                    <div class="form-hint" id="mcp-install-hint">Select a card with <strong>Install (guided)</strong> to prefill this form.</div>
                                </div>
                            </div>
                            <div id="mcp-install-assistant" class="mcp-install-assistant" style="display:none;"></div>
                            <div id="mcp-oauth-helper" class="mcp-oauth-helper" style="display:none;"></div>
                            <form class="form form--full" id="mcp-manual-form">
                                <div class="form-row--2">
                                    <div class="form-group">
                                        <label for="mcp-name">Server Name</label>
                                        <input id="mcp-name" name="name" class="input" type="text" placeholder="my-server" required>
                                        <div class="form-hint">Unique identifier used in tool names (server__tool).</div>
                                    </div>
                                    <div class="form-group">
                                        <label for="mcp-transport">Transport</label>
                                        <select id="mcp-transport" name="transport" class="input">
                                            <option value="stdio" selected>stdio</option>
                                            <option value="http">http</option>
                                        </select>
                                    </div>
                                </div>
                                <div class="form-row--2" id="mcp-stdio-group">
                                    <div class="form-group">
                                        <label for="mcp-command">Command</label>
                                        <input id="mcp-command" name="command" class="input" type="text" placeholder="npx">
                                    </div>
                                    <div class="form-group">
                                        <label for="mcp-args">Args (space-separated)</label>
                                        <input id="mcp-args" name="args" class="input" type="text" placeholder="-y @modelcontextprotocol/server-fetch">
                                    </div>
                                </div>
                                <div class="form-group" id="mcp-http-group" style="display:none;">
                                    <label for="mcp-url">Server URL</label>
                                    <input id="mcp-url" name="url" class="input" type="url" placeholder="https://example.com/mcp">
                                </div>
                                <div class="form-group">
                                    <label for="mcp-env">Environment Variables (KEY=VALUE, one per line)</label>
                                    <textarea id="mcp-env" name="env" class="input" rows="5" placeholder="API_TOKEN=vault://mcp.custom.token"></textarea>
                                    <div class="form-hint">For secrets, prefer vault references: <code>vault://my_key</code>.</div>
                                </div>
                                <div class="form-group">
                                    <label for="mcp-capabilities">Capabilities (comma-separated)</label>
                                    <input id="mcp-capabilities" name="capabilities" class="input" type="text" placeholder="image-analysis, ocr">
                                    <div class="form-hint">Used for automatic attachment fallback routing.</div>
                                </div>
                                <div class="form-actions">
                                    <button class="btn btn-primary" type="submit">Save Server</button>
                                </div>
                            </form>
                        </div>
                    </div>
                </section>

                </div><!-- /mcp-advanced-view -->

                <div class="skill-modal-overlay" id="mcp-modal-overlay">
                    <div class="skill-modal" id="mcp-modal">
                        <div class="skill-modal-header">
                            <div>
                                <div class="skill-modal-title" id="mcp-modal-title"></div>
                                <div class="skill-modal-subtitle" id="mcp-modal-subtitle"></div>
                            </div>
                            <button class="skill-modal-close" id="mcp-modal-close">&times;</button>
                        </div>
                        <div class="skill-modal-meta" id="mcp-modal-meta"></div>
                        <div class="skill-modal-body">
                            <div class="skill-modal-content" id="mcp-modal-content"></div>
                        </div>
                        <div class="skill-modal-footer" id="mcp-modal-footer"></div>
                    </div>
                </div>
            </div>
        </main>"#
    );

    Html(page_html(
        "MCP",
        "mcp",
        &body,
        &["connections.js", "mcp.js"],
    ))
}

#[derive(serde::Deserialize)]
struct McpGoogleOauthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn mcp_google_oauth_callback_page(
    Query(query): Query<McpGoogleOauthCallbackQuery>,
) -> Html<String> {
    render_mcp_oauth_callback_page(
        "google",
        "Google",
        query.code.unwrap_or_default(),
        query.state.unwrap_or_default(),
        query.error.unwrap_or_default(),
        query.error_description.unwrap_or_default(),
    )
}

#[derive(Debug, Deserialize, Default)]
struct McpGitHubOauthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

async fn mcp_github_oauth_callback_page(
    Query(query): Query<McpGitHubOauthCallbackQuery>,
) -> Html<String> {
    render_mcp_oauth_callback_page(
        "github",
        "GitHub",
        query.code.unwrap_or_default(),
        query.state.unwrap_or_default(),
        query.error.unwrap_or_default(),
        query.error_description.unwrap_or_default(),
    )
}

fn render_mcp_oauth_callback_page(
    provider_id: &str,
    provider_label: &str,
    code: String,
    state: String,
    error: String,
    error_description: String,
) -> Html<String> {
    let title = if error.is_empty() {
        format!("{provider_label} OAuth completed")
    } else {
        format!("{provider_label} OAuth failed")
    };
    let message = if error.is_empty() {
        "The authorization code has been captured. You can return to the MCP setup window."
            .to_string()
    } else {
        format!(
            "{provider_label} returned an OAuth error. Review the message below and retry the consent flow."
        )
    };

    Html(format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{title}</title>
    <link rel="stylesheet" href="/static/css/style.css">
</head>
<body class="oauth-callback-page">
    <main class="oauth-callback-shell">
        <div class="oauth-callback-card">
            <span class="badge {badge_class}">{badge_text}</span>
            <h1>{title}</h1>
            <p>{message}</p>
            <div class="oauth-callback-code-block">
                <label for="oauth-code">Authorization Code</label>
                <textarea id="oauth-code" readonly>{code}</textarea>
            </div>
            {error_block}
            <div class="form-actions">
                <button type="button" class="btn btn-primary" id="oauth-copy-btn">Copy code</button>
                <button type="button" class="btn btn-secondary" onclick="window.close()">Close window</button>
            </div>
        </div>
    </main>
    <script>
    (function() {{
        const payload = {{
            type: 'homun-mcp-oauth-code',
            provider: {provider_json},
            code: {code_json},
            state: {state_json},
            error: {error_json},
            error_description: {error_description_json}
        }};
        if (window.opener && window.location.origin) {{
            window.opener.postMessage(payload, window.location.origin);
        }}
        const copyBtn = document.getElementById('oauth-copy-btn');
        const codeEl = document.getElementById('oauth-code');
        if (copyBtn && codeEl) {{
            copyBtn.addEventListener('click', function() {{
                navigator.clipboard.writeText(codeEl.value || '').then(function() {{
                    copyBtn.textContent = 'Copied';
                }});
            }});
        }}
    }})();
    </script>
</body>
</html>"#,
        title = title,
        message = message,
        badge_class = if error.is_empty() {
            "badge-success"
        } else {
            "badge-error"
        },
        badge_text = if error.is_empty() { "Ready" } else { "Error" },
        code = code,
        provider_json = serde_json::to_string(provider_id).unwrap_or_else(|_| "\"\"".to_string()),
        code_json = serde_json::to_string(&code).unwrap_or_else(|_| "\"\"".to_string()),
        state_json = serde_json::to_string(&state).unwrap_or_else(|_| "\"\"".to_string()),
        error_json = serde_json::to_string(&error).unwrap_or_else(|_| "\"\"".to_string()),
        error_description_json =
            serde_json::to_string(&error_description).unwrap_or_else(|_| "\"\"".to_string()),
        error_block = if error.is_empty() {
            String::new()
        } else {
            format!(
                r#"<div class="oauth-callback-error"><strong>{}</strong><div>{}</div></div>"#,
                error, error_description
            )
        },
    ))
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
                            <option value="debug" selected>Debug+</option>
                            <option value="info">Info+</option>
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
                    <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" style="width:20px;height:20px;flex-shrink:0">
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

// ─── File Access ─────────────────────────────────────────────────

async fn file_access_page(State(state): State<Arc<AppState>>) -> Html<String> {
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
                        <h1 class="page-title">File Access</h1>
                        <span class="badge badge-info">{acl_count} ACL rules</span>
                    </div>
                </div>

                <div class="permissions-notice">
                    <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" style="width:20px;height:20px;flex-shrink:0">
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

                <div id="file-access-toast" class="skill-toast" style="display:none"></div>
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
        "File Access",
        "file-access",
        &body,
        &["file-access.js"],
    ))
}

// ─── Shell ───────────────────────────────────────────────────────

async fn shell_page(State(_state): State<Arc<AppState>>) -> Html<String> {
    let body = r#"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Shell</h1>
                    </div>
                </div>

                <div class="permissions-notice">
                    <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" style="width:20px;height:20px;flex-shrink:0">
                        <path d="M3 15V3a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v12"/>
                        <line x1="6" y1="6" x2="12" y2="6"/>
                        <line x1="6" y1="9" x2="12" y2="9"/>
                        <line x1="6" y1="12" x2="9" y2="12"/>
                    </svg>
                    <div>
                        <strong>Shell Permissions</strong><br>
                        OS-specific command restrictions for the shell tool. Changes take effect immediately.
                    </div>
                </div>

                <section class="section">
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

                <div id="shell-toast" class="skill-toast" style="display:none"></div>
            </div>
        </main>"#;

    Html(page_html("Shell", "shell", body, &["shell.js"]))
}

// ─── Sandbox ─────────────────────────────────────────────────────

async fn sandbox_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let config = state.config.read().await;

    let body = format!(
        r#"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Execution Sandbox</h1>
                        <span class="badge {sandbox_badge_class}" id="sandbox-current-badge">{sandbox_badge_text}</span>
                    </div>
                </div>

                <section class="section" id="sandbox-docker-status-section">
                    <h2>Docker Status</h2>
                    <div class="sandbox-docker-status" id="sandbox-docker-status">
                        <div class="sandbox-docker-status-icon" id="sandbox-docker-status-icon">⏳</div>
                        <div class="sandbox-docker-status-info">
                            <div class="sandbox-docker-status-text" id="sandbox-docker-status-text">Checking Docker availability...</div>
                            <div class="sandbox-docker-status-detail" id="sandbox-docker-status-detail"></div>
                        </div>
                        <button type="button" class="btn btn-secondary btn-sm" id="btn-refresh-docker-status">Refresh</button>
                    </div>
                </section>

                <section class="section" id="sandbox-recommendation-section" style="display:none">
                    <div class="sandbox-recommendation" id="sandbox-recommendation">
                        <div class="sandbox-recommendation-text" id="sandbox-recommendation-text"></div>
                        <button type="button" class="btn btn-primary btn-sm" id="btn-apply-sandbox-recommended">Apply Recommended</button>
                    </div>
                </section>

                <section class="section">
                    <h2>Profile</h2>
                    <div class="sandbox-profile-grid">
                        <button type="button" class="sandbox-profile-card" data-sandbox-profile="safe">
                            <div class="sandbox-profile-header">
                                <span class="sandbox-profile-title">Safe</span>
                                <span class="badge badge-neutral" id="sandbox-profile-safe-badge">Fallback allowed</span>
                            </div>
                            <p class="sandbox-profile-desc" id="sandbox-profile-safe-desc">Prefers isolation, but keeps working if Docker is unavailable.</p>
                        </button>
                        <button type="button" class="sandbox-profile-card" data-sandbox-profile="strict">
                            <div class="sandbox-profile-header">
                                <span class="sandbox-profile-title">Strict</span>
                                <span class="badge badge-warning" id="sandbox-profile-strict-badge">Blocks on failure</span>
                            </div>
                            <p class="sandbox-profile-desc" id="sandbox-profile-strict-desc">Requires Docker. Blocks execution if sandbox backend is unavailable.</p>
                        </button>
                        <button type="button" class="sandbox-profile-card" data-sandbox-profile="disabled">
                            <div class="sandbox-profile-header">
                                <span class="sandbox-profile-title">Disabled</span>
                                <span class="badge badge-neutral">Native execution</span>
                            </div>
                            <p class="sandbox-profile-desc">No sandbox wrapper. Processes run natively on the host.</p>
                        </button>
                    </div>
                </section>

                <section class="section" id="sandbox-image-section">
                    <h2>Runtime Image</h2>
                    <div class="sandbox-image-status" id="sandbox-image-status">
                        <div class="sandbox-image-info">
                            <code id="sandbox-image-name">{sandbox_docker_image}</code>
                            <span class="badge badge-neutral" id="sandbox-image-status-badge">checking...</span>
                        </div>
                        <div class="form-actions">
                            <button type="button" class="btn btn-primary btn-sm" id="btn-pull-sandbox-image">Pull Image</button>
                        </div>
                    </div>
                </section>

                <section class="section">
                    <details class="sandbox-advanced" id="sandbox-advanced">
                        <summary>Advanced Settings</summary>
                        <div class="shell-profile-content">
                            <div class="form-group">
                                <label class="checkbox-label">
                                    <input type="checkbox" id="sandbox-enabled" {sandbox_enabled_checked}>
                                    <span>Enable sandbox wrapper</span>
                                </label>
                                <div class="form-hint">When enabled, process execution is wrapped by the selected backend.</div>
                            </div>
                            <div class="form-group">
                                <label>Backend</label>
                                <select id="sandbox-backend" class="input">
                                    <option value="auto">auto</option>
                                    <option value="docker">docker</option>
                                    <option value="linux_native">linux_native</option>
                                    <option value="windows_native">windows_native</option>
                                    <option value="none">none</option>
                                </select>
                            </div>
                            <div class="form-group">
                                <label class="checkbox-label">
                                    <input type="checkbox" id="sandbox-strict" {sandbox_strict_checked}>
                                    <span>Strict mode</span>
                                </label>
                                <div class="form-hint">Fail execution if backend is unavailable instead of falling back.</div>
                            </div>
                            <div id="sandbox-docker-fields">
                                <div class="form-group">
                                    <label>Docker image</label>
                                    <input type="text" id="sandbox-docker-image" class="input" value="{sandbox_docker_image}">
                                </div>
                                <div class="form-group">
                                    <label>Docker network</label>
                                    <select id="sandbox-docker-network" class="input">
                                        <option value="none">none</option>
                                        <option value="bridge">bridge</option>
                                        <option value="host">host</option>
                                    </select>
                                </div>
                                <div class="form-group">
                                    <label>Memory limit (MB)</label>
                                    <input type="number" min="0" step="64" id="sandbox-docker-memory" class="input" value="{sandbox_docker_memory}">
                                </div>
                                <div class="form-group">
                                    <label>CPU limit</label>
                                    <input type="number" min="0" step="0.1" id="sandbox-docker-cpus" class="input" value="{sandbox_docker_cpus}">
                                </div>
                                <div class="form-group">
                                    <label class="checkbox-label">
                                        <input type="checkbox" id="sandbox-docker-readonly" {sandbox_docker_readonly_checked}>
                                        <span>Read-only root filesystem</span>
                                    </label>
                                </div>
                                <div class="form-group">
                                    <label class="checkbox-label">
                                        <input type="checkbox" id="sandbox-docker-mount-workspace" {sandbox_docker_mount_workspace_checked}>
                                        <span>Mount workspace to /workspace</span>
                                    </label>
                                </div>
                            </div>
                            <div class="form-group">
                                <button class="btn btn-primary btn-sm" id="btn-save-sandbox">Save Settings</button>
                            </div>
                        </div>
                    </details>
                </section>

                <div id="sandbox-toast" class="skill-toast" style="display:none"></div>
            </div>
        </main>"#,
        sandbox_badge_class = if config.security.execution_sandbox.enabled {
            "badge-success"
        } else {
            "badge-neutral"
        },
        sandbox_badge_text = if config.security.execution_sandbox.enabled {
            "Enabled"
        } else {
            "Disabled"
        },
        sandbox_enabled_checked = if config.security.execution_sandbox.enabled {
            "checked"
        } else {
            ""
        },
        sandbox_strict_checked = if config.security.execution_sandbox.strict {
            "checked"
        } else {
            ""
        },
        sandbox_docker_image = config.security.execution_sandbox.docker_image,
        sandbox_docker_memory = config.security.execution_sandbox.docker_memory_mb,
        sandbox_docker_cpus = config.security.execution_sandbox.docker_cpus,
        sandbox_docker_readonly_checked =
            if config.security.execution_sandbox.docker_read_only_rootfs {
                "checked"
            } else {
                ""
            },
        sandbox_docker_mount_workspace_checked =
            if config.security.execution_sandbox.docker_mount_workspace {
                "checked"
            } else {
                ""
            },
    );

    Html(page_html("Sandbox", "sandbox", &body, &["sandbox.js"]))
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
                    <div class="provider-card-tools">
                        <button type="button" class="btn btn-secondary btn--sm provider-test-connection">Test Connection</button>
                        <span class="form-hint provider-test-result"></span>
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
                            <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" style="width:32px;height:32px">
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

// ─── Knowledge Page ────────────────────────────────────────────────

async fn knowledge_page(State(_state): State<Arc<AppState>>) -> Html<String> {
    let body = r##"<main class="content">
        <div class="content-inner">
            <div class="page-header">
                <div>
                    <h1>Knowledge Base</h1>
                    <p class="page-subtitle">Personal document knowledge base — upload files and search across your documents.</p>
                </div>
            </div>

            <div class="stats-grid" style="grid-template-columns:repeat(3,1fr)" id="knowledge-stats">
                <div class="stat-card">
                    <div class="stat-label">Sources</div>
                    <div class="stat-value" id="stat-sources">—</div>
                </div>
                <div class="stat-card">
                    <div class="stat-label">Chunks</div>
                    <div class="stat-value" id="stat-chunks">—</div>
                </div>
                <div class="stat-card">
                    <div class="stat-label">Vectors</div>
                    <div class="stat-value" id="stat-vectors">—</div>
                </div>
            </div>

            <div class="knowledge-grid">
                <div class="knowledge-panel">
                    <h2>Upload Files</h2>
                    <div class="upload-zone" id="upload-zone">
                        <div class="upload-icon">
                            <svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                                <polyline points="14 2 14 8 20 8"/>
                                <line x1="12" y1="18" x2="12" y2="12"/>
                                <line x1="9" y1="15" x2="12" y2="12"/>
                                <line x1="15" y1="15" x2="12" y2="12"/>
                            </svg>
                        </div>
                        <p>Drag & drop files here or <label for="file-input" class="upload-label">browse</label></p>
                        <p class="upload-hint">Supports: .md, .txt, .pdf, .docx, .xlsx, .rs, .py, .js, .ts, .go, .java, .toml, .yaml, .json, .html, .css, .sh, .sql</p>
                        <input type="file" id="file-input" multiple style="display:none"
                            accept=".md,.markdown,.txt,.log,.rs,.py,.js,.ts,.go,.java,.c,.cpp,.h,.hpp,.toml,.yaml,.yml,.json,.html,.htm,.css,.sh,.bash,.zsh,.sql,.xml,.csv,.ini,.cfg,.conf,.env,.pdf,.docx,.xlsx,.xls,.xlsm,.odt">
                    </div>
                    <div id="upload-progress" class="upload-progress" style="display:none"></div>

                    <h3 style="margin-top:1.25rem;margin-bottom:0.5rem;font-size:0.85rem;font-weight:600;letter-spacing:0.02em;opacity:0.7">Index Folder</h3>
                    <div class="folder-index-form" style="display:flex;gap:0.5rem;align-items:center;flex-wrap:wrap">
                        <input type="text" id="folder-path" placeholder="/path/to/folder" class="search-input" style="flex:1;min-width:200px">
                        <label style="display:flex;align-items:center;gap:0.25rem;font-size:0.8rem;white-space:nowrap">
                            <input type="checkbox" id="folder-recursive" checked> Recursive
                        </label>
                        <button id="index-folder-btn" class="btn btn-primary">Index</button>
                    </div>
                    <div id="folder-progress" class="upload-progress" style="display:none"></div>
                </div>

                <div class="knowledge-panel">
                    <h2>Search</h2>
                    <div class="search-bar">
                        <input type="text" id="knowledge-search" placeholder="Search your knowledge base..." class="search-input">
                        <button id="search-btn" class="btn btn-primary">Search</button>
                    </div>
                    <div id="search-results" class="search-results"></div>
                </div>
            </div>

            <div class="knowledge-panel">
                <h2>Indexed Sources</h2>
                <div id="sources-list" class="sources-list">
                    <p class="empty-state">Loading sources...</p>
                </div>
            </div>
        </div>
    </main>"##;

    let html = page_html("Knowledge", "knowledge", body, &["knowledge.js"]);
    Html(html)
}

// ─── Business ──────────────────────────────────────────────────

async fn business_page() -> Html<String> {
    let body = r##"<main class="content">
            <div class="content-inner">
                <div class="page-header">
                    <div class="page-title-group">
                        <h1 class="page-title">Business</h1>
                        <span class="badge badge-info" id="biz-count">0</span>
                    </div>
                    <div class="actions">
                        <button class="btn btn-secondary btn-sm" id="btn-biz-refresh">Refresh</button>
                    </div>
                </div>

                <div class="stats-grid" style="grid-template-columns:repeat(4,1fr)">
                    <div class="stat-card">
                        <div class="stat-label">Active</div>
                        <div class="stat-value" id="stat-biz-active">0</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-label">Total Revenue</div>
                        <div class="stat-value" id="stat-biz-revenue">0.00</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-label">Profit</div>
                        <div class="stat-value" id="stat-biz-profit">0.00</div>
                    </div>
                    <div class="stat-card">
                        <div class="stat-label">Products</div>
                        <div class="stat-value" id="stat-biz-products">0</div>
                    </div>
                </div>

                <section class="section">
                    <h2>Launch Business</h2>
                    <form id="biz-create-form" class="form form--full">
                        <div class="form-row--2">
                            <div class="form-group">
                                <label for="biz-name">Name</label>
                                <input id="biz-name" class="input" type="text" maxlength="120" placeholder="My AI Business">
                            </div>
                            <div class="form-group">
                                <label for="biz-autonomy">Autonomy</label>
                                <select id="biz-autonomy" class="input">
                                    <option value="semi">Semi-autonomous</option>
                                    <option value="budget">Budget-limited</option>
                                    <option value="full">Full autonomous</option>
                                </select>
                            </div>
                        </div>
                        <div class="form-row--2">
                            <div class="form-group">
                                <label for="biz-budget">Budget (optional)</label>
                                <input id="biz-budget" class="input" type="number" step="0.01" min="0" placeholder="100.00">
                            </div>
                            <div class="form-group">
                                <label for="biz-currency">Currency</label>
                                <input id="biz-currency" class="input" type="text" maxlength="3" value="EUR" placeholder="EUR">
                            </div>
                        </div>
                        <div class="form-group">
                            <label for="biz-description">Description</label>
                            <textarea id="biz-description" class="input" rows="2" placeholder="What this business does..."></textarea>
                        </div>
                        <div class="form-row--2">
                            <div class="form-group">
                                <label for="biz-deliver-to">Deliver To</label>
                                <select id="biz-deliver-to" class="input"></select>
                            </div>
                            <div class="form-group" style="display:flex;align-items:flex-end">
                                <button type="submit" class="btn btn-primary">Launch</button>
                            </div>
                        </div>
                    </form>
                </section>

                <section class="section">
                    <h2>Businesses</h2>
                    <div id="biz-list" class="card-grid">
                        <p class="empty-state">No businesses yet. Launch one above.</p>
                    </div>
                </section>

                <!-- Detail Panel -->
                <section class="section" id="biz-detail-panel" style="display:none">
                    <div class="page-header">
                        <h2 id="biz-detail-name">Business Detail</h2>
                        <div class="actions">
                            <button class="btn btn-secondary btn-sm" id="btn-biz-pause">Pause</button>
                            <button class="btn btn-secondary btn-sm" id="btn-biz-resume" style="display:none">Resume</button>
                            <button class="btn btn-danger btn-sm" id="btn-biz-close">Close</button>
                        </div>
                    </div>
                    <div class="stats-grid" style="grid-template-columns:repeat(3,1fr)">
                        <div class="stat-card">
                            <div class="stat-label">Revenue</div>
                            <div class="stat-value" id="biz-d-revenue">0.00</div>
                        </div>
                        <div class="stat-card">
                            <div class="stat-label">Expenses</div>
                            <div class="stat-value" id="biz-d-expenses">0.00</div>
                        </div>
                        <div class="stat-card">
                            <div class="stat-label">Profit</div>
                            <div class="stat-value" id="biz-d-profit">0.00</div>
                        </div>
                    </div>
                    <div id="biz-detail-info" class="detail-info"></div>

                    <h3>Strategies</h3>
                    <div id="biz-strategies-list" class="items-list">
                        <p class="empty-state">No strategies yet.</p>
                    </div>

                    <h3>Products</h3>
                    <div id="biz-products-list" class="items-list">
                        <p class="empty-state">No products yet.</p>
                    </div>

                    <h3>Recent Transactions</h3>
                    <div id="biz-transactions-list" class="items-list">
                        <p class="empty-state">No transactions yet.</p>
                    </div>
                </section>
            </div>
        </main>"##;

    let html = page_html("Business", "business", body, &["business.js"]);
    Html(html)
}

// ─── Auth Pages (standalone, no sidebar) ───────────────────────

/// Standalone page wrapper without sidebar (for login, setup).
fn standalone_page(title: &str, body: &str) -> String {
    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{title} — Homun</title>
    <link rel="icon" href="/static/img/favicon/favicon.ico" sizes="any">
    <link rel="icon" href="/static/img/favicon.svg" type="image/svg+xml">
    <link rel="stylesheet" href="/static/css/style.css">
    <script>
    (function() {{
        var theme = localStorage.getItem('homun-theme') || 'system';
        if (theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)) {{
            document.documentElement.classList.add('dark');
        }}
    }})();
    </script>
    <style>
        body {{ display: flex; justify-content: center; align-items: center; min-height: 100vh; background: var(--bg-primary); }}
        .auth-card {{ background: var(--bg-secondary); border: 1px solid var(--border); border-radius: 12px; padding: 2rem; width: 100%; max-width: 400px; box-shadow: 0 4px 24px rgba(0,0,0,0.2); }}
        .auth-card h1 {{ font-size: 1.5rem; margin: 0 0 0.5rem; text-align: center; }}
        .auth-card p {{ color: var(--text-secondary); font-size: 0.875rem; text-align: center; margin: 0 0 1.5rem; }}
        .auth-card label {{ display: block; font-size: 0.8125rem; font-weight: 500; margin-bottom: 0.375rem; color: var(--text-secondary); }}
        .auth-card input[type="text"], .auth-card input[type="password"] {{ width: 100%; padding: 0.625rem 0.75rem; border: 1px solid var(--border); border-radius: 8px; background: var(--bg-primary); color: var(--text-primary); font-size: 0.875rem; box-sizing: border-box; margin-bottom: 1rem; }}
        .auth-card input:focus {{ outline: none; border-color: var(--accent); box-shadow: 0 0 0 2px rgba(99,102,241,0.2); }}
        .auth-card button {{ width: 100%; padding: 0.75rem; border: none; border-radius: 8px; background: var(--accent); color: white; font-size: 0.875rem; font-weight: 600; cursor: pointer; transition: opacity 0.15s; }}
        .auth-card button:hover {{ opacity: 0.9; }}
        .auth-card button:disabled {{ opacity: 0.5; cursor: not-allowed; }}
        .auth-error {{ color: var(--danger, #ef4444); font-size: 0.8125rem; text-align: center; min-height: 1.25rem; margin-bottom: 0.5rem; }}
        .auth-logo {{ text-align: center; margin-bottom: 1.5rem; }}
        .auth-logo img {{ height: 40px; width: auto; }}
        .auth-logo-dark {{ display: none; }}
        .dark .auth-logo-light {{ display: none; }}
        .dark .auth-logo-dark {{ display: inline; }}
    </style>
</head>
<body>
    {body}
</body>
</html>"##
    )
}

/// GET /login — standalone login page
pub async fn login_page() -> Html<String> {
    let body = r##"
    <div class="auth-card">
        <div class="auth-logo">
            <img src="/static/img/homun.png" alt="Homun" class="auth-logo-light">
            <img src="/static/img/homun_white.png" alt="Homun" class="auth-logo-dark">
        </div>
        <p>Sign in to access your assistant</p>
        <div class="auth-error" id="error-msg"></div>
        <form id="login-form" onsubmit="return handleLogin(event)">
            <label for="username">Username</label>
            <input type="text" id="username" name="username" required autocomplete="username" autofocus>
            <label for="password">Password</label>
            <input type="password" id="password" name="password" required autocomplete="current-password">
            <button type="submit" id="login-btn">Sign In</button>
        </form>
    </div>
    <script>
    async function handleLogin(e) {
        e.preventDefault();
        const btn = document.getElementById('login-btn');
        const err = document.getElementById('error-msg');
        btn.disabled = true;
        err.textContent = '';
        try {
            const res = await fetch('/api/auth/login', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    username: document.getElementById('username').value,
                    password: document.getElementById('password').value
                })
            });
            const data = await res.json();
            if (data.success && data.redirect) {
                window.location.href = data.redirect;
            } else {
                err.textContent = data.error || 'Login failed';
            }
        } catch (ex) {
            err.textContent = 'Network error';
        }
        btn.disabled = false;
        return false;
    }
    </script>
    "##;

    Html(standalone_page("Login", body))
}

/// GET /setup-wizard — first-run admin account creation
pub async fn setup_wizard_page() -> Html<String> {
    let body = r##"
    <div class="auth-card">
        <div class="auth-logo">
            <img src="/static/img/homun.png" alt="Homun" class="auth-logo-light">
            <img src="/static/img/homun_white.png" alt="Homun" class="auth-logo-dark">
        </div>
        <p>Create your admin account to get started</p>
        <div class="auth-error" id="error-msg"></div>
        <form id="setup-form" onsubmit="return handleSetup(event)">
            <label for="username">Username</label>
            <input type="text" id="username" name="username" required autocomplete="username" autofocus>
            <label for="password">Password</label>
            <input type="password" id="password" name="password" required autocomplete="new-password" minlength="6">
            <label for="confirm">Confirm Password</label>
            <input type="password" id="confirm" name="confirm" required autocomplete="new-password" minlength="6">
            <button type="submit" id="setup-btn">Create Account</button>
        </form>
    </div>
    <script>
    async function handleSetup(e) {
        e.preventDefault();
        const btn = document.getElementById('setup-btn');
        const err = document.getElementById('error-msg');
        const pw = document.getElementById('password').value;
        const confirm = document.getElementById('confirm').value;
        btn.disabled = true;
        err.textContent = '';

        if (pw !== confirm) {
            err.textContent = 'Passwords do not match';
            btn.disabled = false;
            return false;
        }
        if (pw.length < 6) {
            err.textContent = 'Password must be at least 6 characters';
            btn.disabled = false;
            return false;
        }

        try {
            const res = await fetch('/api/auth/setup', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    username: document.getElementById('username').value,
                    password: pw
                })
            });
            const data = await res.json();
            if (data.success && data.redirect) {
                window.location.href = data.redirect;
            } else {
                err.textContent = data.error || 'Setup failed';
            }
        } catch (ex) {
            err.textContent = 'Network error';
        }
        btn.disabled = false;
        return false;
    }
    </script>
    "##;

    Html(standalone_page("Setup", body))
}
