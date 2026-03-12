//! Browser automation via Playwright MCP Server.
//!
//! The browser is managed as an MCP server (`@playwright/mcp`).
//! Configuration comes from the `[browser]` section of `config.toml`
//! and is auto-translated into an MCP server config at startup by
//! [`mcp_bridge::browser_mcp_server_config`].
//!
//! # Usage
//!
//! ```toml
//! [browser]
//! enabled = true
//! headless = true
//! ```

pub mod diff;
pub mod helpers;
pub mod mcp_bridge;

pub use helpers::{has_browser_tools, is_browser_tool};
pub use mcp_bridge::{browser_mcp_server_config, BROWSER_MCP_SERVER_NAME};

/// Quick runtime status check from config (no MCP connection needed).
pub fn browser_runtime_status_for_config(
    config: &crate::config::BrowserConfig,
) -> crate::config::BrowserRuntimeStatus {
    config.runtime_status()
}

/// Check current browser status from the global config.
pub fn current_browser_status() -> crate::config::BrowserRuntimeStatus {
    let config = crate::config::Config::load()
        .map(|c| c.browser)
        .unwrap_or_default();
    browser_runtime_status_for_config(&config)
}
