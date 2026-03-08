//! Bridges the `[browser]` config section into an MCP server config.
//!
//! Instead of requiring users to manually configure `[mcp.servers.playwright]`,
//! this module reads the existing `BrowserConfig` and synthesises a virtual
//! `McpServerConfig` that starts the official `@playwright/mcp` server with
//! the right CLI flags.

use std::collections::HashMap;

use crate::config::{BrowserConfig, McpServerConfig};

/// Canonical MCP server name used when injecting the browser server.
///
/// MCP tools will be registered as `playwright__browser_navigate`, etc.
pub const BROWSER_MCP_SERVER_NAME: &str = "playwright";

/// Convert a `BrowserConfig` into an `McpServerConfig` for `@playwright/mcp`.
///
/// Returns `None` if the browser is disabled.
pub fn browser_mcp_server_config(browser: &BrowserConfig) -> Option<McpServerConfig> {
    if !browser.enabled {
        return None;
    }

    let mut args = vec!["-y".to_string(), "@playwright/mcp@latest".to_string()];

    // Browser type (chromium, firefox, webkit)
    let browser_type = browser
        .browser_type_for_profile(&browser.default_profile)
        .to_lowercase();
    if browser_type != "chromium" {
        args.push(format!("--browser={browser_type}"));
    }

    // Headless mode
    if browser.headless_for_profile(&browser.default_profile) {
        args.push("--headless".to_string());
    }

    // Browser executable path (use system Chrome instead of Playwright's bundled one)
    if let Some(executable) = browser.resolved_executable() {
        args.push(format!(
            "--executable-path={}",
            executable.display()
        ));
    }

    // User data directory for persistent profiles (cookies, sessions, etc.)
    let user_data_dir = browser.profile_user_data_path(&browser.default_profile);
    args.push(format!("--user-data-dir={}", user_data_dir.display()));

    // Viewport
    args.push("--viewport-size=1280,720".to_string());

    let mut env = HashMap::new();
    // Suppress Playwright download prompts — we use the system browser
    env.insert(
        "PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD".to_string(),
        "1".to_string(),
    );

    Some(McpServerConfig {
        transport: "stdio".to_string(),
        command: Some("npx".to_string()),
        args,
        url: None,
        env,
        capabilities: vec!["browser".to_string()],
        enabled: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_browser_returns_none() {
        let mut config = BrowserConfig::default();
        config.enabled = false;
        assert!(browser_mcp_server_config(&config).is_none());
    }

    #[test]
    fn enabled_browser_generates_mcp_config() {
        let mut config = BrowserConfig::default();
        config.enabled = true;
        config.headless = true;
        config.browser_type = "chromium".to_string();

        let mcp = browser_mcp_server_config(&config);
        assert!(mcp.is_some());

        let mcp = mcp.unwrap();
        assert_eq!(mcp.transport, "stdio");
        assert_eq!(mcp.command, Some("npx".to_string()));
        assert!(mcp.args.contains(&"@playwright/mcp@latest".to_string()));
        assert!(mcp.args.contains(&"--headless".to_string()));
        assert!(mcp.args.iter().any(|a| a.starts_with("--user-data-dir=")));
        assert!(mcp.enabled);
    }

    #[test]
    fn non_headless_omits_flag() {
        let mut config = BrowserConfig::default();
        config.enabled = true;
        config.headless = false;

        let mcp = browser_mcp_server_config(&config).unwrap();
        assert!(!mcp.args.contains(&"--headless".to_string()));
    }

    #[test]
    fn firefox_browser_type() {
        let mut config = BrowserConfig::default();
        config.enabled = true;
        config.browser_type = "firefox".to_string();

        let mcp = browser_mcp_server_config(&config).unwrap();
        assert!(mcp.args.contains(&"--browser=firefox".to_string()));
    }
}
