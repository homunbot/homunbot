//! Bridges the `[browser]` config section into an MCP server config.
//!
//! Instead of requiring users to manually configure `[mcp.servers.playwright]`,
//! this module reads the existing `BrowserConfig` and synthesises a virtual
//! `McpServerConfig` that starts the official `@playwright/mcp` server with
//! the right CLI flags.
//!
//! [`BrowserPool`] manages one MCP peer per profile, lazy-started on first use.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::RwLock;

use crate::config::{BrowserConfig, Config, McpServerConfig};
use crate::tools::mcp::McpPeer;

/// Canonical MCP server name used when injecting the browser server.
///
/// MCP tools will be registered as `playwright__browser_navigate`, etc.
pub const BROWSER_MCP_SERVER_NAME: &str = "playwright";

// ─── Config generation ──────────────────────────────────────────

/// Convert a `BrowserConfig` into an `McpServerConfig` for the default profile.
///
/// Returns `None` if the browser is disabled.
pub fn browser_mcp_server_config(browser: &BrowserConfig) -> Option<McpServerConfig> {
    browser_mcp_server_config_for_profile(browser, &browser.default_profile)
}

/// Convert a `BrowserConfig` into an `McpServerConfig` for a specific profile.
///
/// Uses profile-specific overrides for browser_type, headless, and user_data_dir.
/// Returns `None` if the browser is disabled or the profile doesn't exist.
pub fn browser_mcp_server_config_for_profile(
    browser: &BrowserConfig,
    profile_name: &str,
) -> Option<McpServerConfig> {
    if !browser.enabled {
        return None;
    }

    // Verify the profile exists
    if !browser.profiles.contains_key(profile_name) {
        tracing::warn!(profile = profile_name, "Browser profile not found in config");
        return None;
    }

    let mut args = vec!["-y".to_string(), "@playwright/mcp@latest".to_string()];

    // Browser type (chromium, firefox, webkit) — profile override or global
    let browser_type = browser
        .browser_type_for_profile(profile_name)
        .to_lowercase();
    if browser_type != "chromium" {
        args.push(format!("--browser={browser_type}"));
    }

    // Headless mode — profile override or global
    if browser.headless_for_profile(profile_name) {
        args.push("--headless".to_string());
    }

    // Browser executable path (use system Chrome instead of Playwright's bundled one)
    if let Some(executable) = browser.resolved_executable() {
        args.push(format!("--executable-path={}", executable.display()));
    }

    // User data directory — profile-specific isolation
    let user_data_dir = browser.profile_user_data_path(profile_name);
    args.push(format!("--user-data-dir={}", user_data_dir.display()));

    // Viewport
    args.push("--viewport-size=1280,720".to_string());

    // Profile-specific proxy
    if let Some(profile) = browser.profiles.get(profile_name) {
        if let Some(ref proxy) = profile.proxy {
            args.push(format!("--proxy-server={proxy}"));
        }
    }

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
        recipe_id: None,
        auth_env_key: None,
        discovered_tool_count: None,
    })
}

// ─── Browser Pool ───────────────────────────────────────────────

/// Manages one MCP Playwright peer per browser profile, lazy-started on first use.
///
/// The default profile peer is injected eagerly at startup via [`set_default_peer`].
/// Non-default profiles are started on-demand when the browser tool receives a
/// `profile` parameter.
pub struct BrowserPool {
    /// Running MCP peers keyed by profile name.
    peers: RwLock<HashMap<String, Arc<McpPeer>>>,
    /// Shared config for reading profile definitions at connect time.
    config: Arc<RwLock<Config>>,
}

impl BrowserPool {
    /// Create a new empty pool.
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self {
            peers: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// Inject the eagerly-started default profile peer (from `take_browser_peer()`).
    pub async fn set_default_peer(&self, profile_name: &str, peer: Arc<McpPeer>) {
        let mut peers = self.peers.write().await;
        peers.insert(profile_name.to_string(), peer);
        tracing::info!(profile = profile_name, "Default browser peer registered in pool");
    }

    /// Get the MCP peer for a profile, starting a new process if needed.
    ///
    /// Returns an error if the profile doesn't exist in config or the MCP
    /// process fails to start.
    pub async fn get_or_start(&self, profile_name: &str) -> Result<Arc<McpPeer>> {
        // Fast path: peer already running
        {
            let peers = self.peers.read().await;
            if let Some(peer) = peers.get(profile_name) {
                return Ok(Arc::clone(peer));
            }
        }

        // Slow path: start a new MCP process for this profile
        let mcp_config = {
            let config = self.config.read().await;
            browser_mcp_server_config_for_profile(&config.browser, profile_name)
                .with_context(|| format!("Profile '{profile_name}' not found or browser disabled"))?
        };

        let sandbox_config = {
            let config = self.config.read().await;
            config.security.execution_sandbox.clone()
        };

        let server_name = format!("playwright-{profile_name}");
        tracing::info!(profile = profile_name, "Starting browser MCP peer for profile");

        let peer =
            crate::tools::McpManager::connect_peer(&server_name, &mcp_config, &sandbox_config)
                .await
                .with_context(|| {
                    format!("Failed to start browser for profile '{profile_name}'")
                })?;

        // Store and return
        let mut peers = self.peers.write().await;
        // Double-check: another task may have raced
        if let Some(existing) = peers.get(profile_name) {
            // Another task won the race — shut down our peer and use theirs
            peer.shutdown().await;
            return Ok(Arc::clone(existing));
        }
        peers.insert(profile_name.to_string(), Arc::clone(&peer));
        Ok(peer)
    }

    /// Shut down a specific profile's MCP process (for config changes or cleanup).
    pub async fn shutdown_profile(&self, profile_name: &str) {
        let peer = {
            let mut peers = self.peers.write().await;
            peers.remove(profile_name)
        };
        if let Some(peer) = peer {
            tracing::info!(profile = profile_name, "Shutting down browser profile peer");
            peer.shutdown().await;
        }
    }

    /// Shut down all MCP peers (for E-Stop or graceful shutdown).
    pub async fn shutdown_all(&self) {
        let peers: Vec<(String, Arc<McpPeer>)> = {
            let mut map = self.peers.write().await;
            map.drain().collect()
        };
        for (name, peer) in peers {
            tracing::info!(profile = %name, "Shutting down browser peer");
            peer.shutdown().await;
        }
    }

    /// List profile names that currently have a running MCP peer.
    pub async fn active_profiles(&self) -> Vec<String> {
        let peers = self.peers.read().await;
        peers.keys().cloned().collect()
    }
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

    #[test]
    fn profile_specific_config() {
        let mut config = BrowserConfig::default();
        config.enabled = true;
        config.browser_type = "chromium".to_string();
        config.headless = true;

        // Add a custom profile with overrides
        use crate::config::BrowserProfile;
        config.profiles.insert(
            "social".to_string(),
            BrowserProfile {
                name: "Social Media".to_string(),
                browser_type: Some("firefox".to_string()),
                headless: Some(false),
                proxy: Some("http://proxy:8080".to_string()),
                description: Some("For social media".to_string()),
                ..Default::default()
            },
        );

        // Default profile should use global settings
        let default_mcp = browser_mcp_server_config(&config).unwrap();
        assert!(default_mcp.args.contains(&"--headless".to_string()));
        assert!(!default_mcp.args.iter().any(|a| a.contains("firefox")));

        // Social profile should use its overrides
        let social_mcp =
            browser_mcp_server_config_for_profile(&config, "social").unwrap();
        assert!(!social_mcp.args.contains(&"--headless".to_string()));
        assert!(social_mcp.args.contains(&"--browser=firefox".to_string()));
        assert!(social_mcp
            .args
            .iter()
            .any(|a| a.contains("--proxy-server=http://proxy:8080")));
        assert!(social_mcp
            .args
            .iter()
            .any(|a| a.contains("browser-profiles/social")));
    }

    #[test]
    fn unknown_profile_returns_none() {
        let mut config = BrowserConfig::default();
        config.enabled = true;
        assert!(browser_mcp_server_config_for_profile(&config, "nonexistent").is_none());
    }
}
