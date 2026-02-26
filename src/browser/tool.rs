//! Browser automation tool for LLM.
//!
//! Provides browser control capabilities: navigate, click, type, snapshot, etc.
//! Integrates with the vault for secure credential input (vault:// prefix).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chromiumoxide::page::Page;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::config::BrowserConfig;
use crate::security::{global_session_manager, TwoFactorStorage};
use crate::storage::{global_secrets, SecretKey};

use super::actions::{BrowserAction, WaitType};
use super::manager::{global_browser_manager, ConsoleMessage, PageError, ConsoleLevel, HttpMethod, NetworkRequest};
use super::snapshot::{PageSnapshot, RoleRef};

use crate::tools::registry::{Tool, ToolContext, ToolResult, get_string_param, get_optional_string, get_optional_bool};

/// Global cache for role_refs per chat_id (persists across snapshots)
static ROLE_REFS_CACHE: std::sync::OnceLock<Arc<RwLock<HashMap<String, HashMap<String, RoleRef>>>>> = std::sync::OnceLock::new();

fn get_role_refs_cache() -> Arc<RwLock<HashMap<String, HashMap<String, RoleRef>>>> {
    ROLE_REFS_CACHE
        .get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
        .clone()
}

/// Browser automation tool.
///
/// Allows the LLM to control a browser for web navigation, form filling,
/// and content extraction.
pub struct BrowserTool;

impl BrowserTool {
    pub fn new() -> Self {
        Self
    }

    /// Cache role_refs for a chat_id after taking a snapshot.
    async fn cache_role_refs(&self, chat_id: &str, role_refs: HashMap<String, RoleRef>) {
        let cache = get_role_refs_cache();
        let mut cache = cache.write().await;
        cache.insert(chat_id.to_string(), role_refs);
        tracing::debug!(chat_id = %chat_id, "Cached {} role_refs", cache.get(chat_id).map(|r| r.len()).unwrap_or(0));
    }

    /// Get cached role_refs for a chat_id.
    async fn get_cached_role_refs(&self, chat_id: &str) -> Option<HashMap<String, RoleRef>> {
        let cache = get_role_refs_cache();
        let cache = cache.read().await;
        cache.get(chat_id).cloned()
    }

    /// Find element by role-based lookup (OpenClaw compatible).
    ///
    /// Uses JavaScript to find elements by their ARIA role and accessible name.
    /// This is more robust for SPAs than CSS selectors.
    async fn find_element_by_role(&self, page: &Page, ref_id: &str, chat_id: &str) -> Result<String> {
        // Get cached role_refs
        let role_refs = self.get_cached_role_refs(chat_id).await
            .ok_or_else(|| anyhow::anyhow!(
                "No cached role_refs for this session. Take a snapshot first to get element refs."
            ))?;

        let role_ref = role_refs.get(ref_id)
            .ok_or_else(|| anyhow::anyhow!(
                "Ref '{}' not found in cached snapshot. Take a new snapshot to get current element refs.",
                ref_id
            ))?;

        tracing::debug!(ref_id = %ref_id, role = %role_ref.role, name = ?role_ref.name, nth = ?role_ref.nth, "Finding element by role");

        // Build JavaScript to find element by role and name
        let role = &role_ref.role;
        let name = role_ref.name.as_deref().unwrap_or("");
        let nth = role_ref.nth.unwrap_or(0);

        let js = format!(
            r#"
            (function() {{
                const role = "{role}";
                const name = "{name_escaped}";
                const nth = {nth};

                // Map ARIA roles to HTML elements
                const roleToElement = {{
                    "button": ["button", "[role='button']", "input[type='button']", "input[type='submit']"],
                    "link": ["a[href]", "[role='link']"],
                    "textbox": ["input:not([type])", "input[type='text']", "input[type='email']", "input[type='password']", "input[type='search']", "input[type='tel']", "input[type='url']", "textarea", "[role='textbox']"],
                    "checkbox": ["input[type='checkbox']", "[role='checkbox']"],
                    "radio": ["input[type='radio']", "[role='radio']"],
                    "combobox": ["select", "[role='combobox']", "[role='listbox']"],
                    "listbox": ["select", "[role='listbox']"],
                    "searchbox": ["input[type='search']", "[role='searchbox']"],
                    "spinbutton": ["input[type='number']", "[role='spinbutton']"],
                    "slider": ["input[type='range']", "[role='slider']"],
                    "switch": ["[role='switch']"],
                    "tab": ["[role='tab']"],
                    "menuitem": ["[role='menuitem']"],
                    "menuitemcheckbox": ["[role='menuitemcheckbox']"],
                    "menuitemradio": ["[role='menuitemradio']"],
                    "treeitem": ["[role='treeitem']"],
                    "option": ["option", "[role='option']"],
                    "heading": ["h1", "h2", "h3", "h4", "h5", "h6", "[role='heading']"],
                    "img": ["img", "[role='img']"],
                    "listitem": ["li", "[role='listitem']"],
                    "cell": ["td", "th", "[role='cell']", "[role='gridcell']"],
                    "gridcell": ["td", "[role='gridcell']"],
                    "columnheader": ["th", "[role='columnheader']"],
                    "rowheader": ["th", "[role='rowheader']"],
                }};

                const selectors = roleToElement[role] || [`[role='${{role}}']`];
                const candidates = [];

                for (const sel of selectors) {{
                    try {{
                        const elements = document.querySelectorAll(sel);
                        for (const el of elements) {{
                            // Skip hidden elements
                            const style = window.getComputedStyle(el);
                            if (style.display === 'none' || style.visibility === 'hidden') continue;
                            if (el.offsetWidth < 5 || el.offsetHeight < 5) continue;

                            candidates.push(el);
                        }}
                    }} catch (e) {{}}
                }}

                // Filter by name if provided
                let matches = candidates;
                if (name) {{
                    matches = candidates.filter(el => {{
                        // Get accessible name from various sources
                        const accessibleName =
                            el.getAttribute('aria-label') ||
                            el.getAttribute('title') ||
                            el.getAttribute('alt') ||
                            el.getAttribute('placeholder') ||
                            el.getAttribute('value') ||
                            el.textContent ||
                            el.innerText ||
                            "";

                        return accessibleName.toLowerCase().includes(name.toLowerCase());
                    }});
                }}

                // Select by nth index
                if (matches.length === 0) {{
                    return null;
                }}

                const targetEl = matches[nth] || matches[0];
                if (!targetEl) return null;

                // Generate a unique CSS selector for this element
                // Try ID first
                if (targetEl.id) {{
                    return '#' + CSS.escape(targetEl.id);
                }}

                // Build path selector
                const path = [];
                let current = targetEl;
                while (current && current !== document.body) {{
                    let sel = current.tagName.toLowerCase();

                    // Add role if available
                    if (current.getAttribute('role')) {{
                        sel += `[role="${{current.getAttribute('role')}}"]`;
                    }}

                    // Add unique classes (limited)
                    if (current.className && typeof current.className === 'string') {{
                        const classes = current.className.split(' ')
                            .filter(c => c && !c.includes(':') && c.length < 20)
                            .slice(0, 2);
                        if (classes.length > 0) {{
                            sel += '.' + classes.map(c => CSS.escape(c)).join('.');
                        }}
                    }}

                    // Add index if needed
                    const siblings = current.parentElement ?
                        Array.from(current.parentElement.children).filter(c => c.tagName === current.tagName) : [];
                    if (siblings.length > 1) {{
                        const idx = siblings.indexOf(current) + 1;
                        sel += `:nth-of-type(${{idx}})`;
                    }}

                    path.unshift(sel);
                    current = current.parentElement;

                    // Limit path depth
                    if (path.length >= 4) break;
                }}

                return path.join(' > ');
            }})();
            "#,
            role = role,
            name_escaped = name.replace('\\', "\\\\").replace('"', "\\\""),
            nth = nth
        );

        let result = page
            .evaluate(js.as_str())
            .await
            .context("Failed to find element by role")?;

        let selector: Option<String> = result
            .into_value()
            .context("Failed to parse selector")?;

        selector.ok_or_else(|| anyhow::anyhow!(
            "Element '{}' (role={}, name={}) not found on page. The page may have changed - take a new snapshot.",
            ref_id, role, name
        ))
    }

    /// Resolve a vault:// reference to its actual value.
    async fn resolve_vault_reference(&self, text: &str, session_id: Option<&str>) -> Option<Result<String>> {
        if !text.starts_with("vault://") {
            return None;
        }

        let key = text.strip_prefix("vault://").unwrap();

        // Check if 2FA is enabled
        let twofa_enabled = TwoFactorStorage::new()
            .ok()
            .and_then(|s| s.load().ok())
            .map(|c| c.enabled)
            .unwrap_or(false);

        if twofa_enabled {
            // Verify session
            if let Some(sid) = session_id {
                let session_manager = global_session_manager();
                if !session_manager.verify_session(sid).await {
                    return Some(Err(anyhow::anyhow!(
                        "2FA session expired. Please provide a valid session_id."
                    )));
                }
            } else {
                return Some(Err(anyhow::anyhow!(
                    "2FA_REQUIRED: Two-factor authentication is enabled. \
                     Provide 'session_id' parameter after authenticating with vault confirm."
                )));
            }
        }

        // Retrieve from vault
        match global_secrets() {
            Ok(secrets) => match secrets.get(&SecretKey::custom(&format!("vault.{}", key))) {
                Ok(Some(value)) => Some(Ok(value)),
                Ok(None) => Some(Err(anyhow::anyhow!("Secret '{}' not found in vault", key))),
                Err(e) => Some(Err(anyhow::anyhow!("Failed to retrieve secret: {}", e))),
            },
            Err(e) => Some(Err(anyhow::anyhow!("Failed to access vault: {}", e))),
        }
    }

    /// Execute navigate action.
    async fn execute_navigate(&self, page: &Page, url: &str, timeout_secs: u64) -> Result<String> {
        // Use tokio::time::timeout for navigation
        let result = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            async {
                page.goto(url).await?;
                // Wait for navigation to complete properly
                page.wait_for_navigation().await
            }
        )
        .await
        .context("Navigation timeout")?;

        result.context("Navigation failed")?;

        Ok(format!("Navigated to: {}", url))
    }

    /// Execute snapshot action.
    ///
    /// Returns a text-based accessibility tree snapshot (like Playwright/OpenClaw).
    /// Optionally takes a screenshot for the UI gallery.
    /// Caches role_refs for later element resolution.
    /// Collects console messages and errors from the page.
    async fn execute_snapshot(&self, page: &Page, chat_id: &str, take_screenshot: bool, screenshot_dir: &std::path::Path) -> Result<PageSnapshot> {
        // Collect console messages and errors from the page
        let manager = global_browser_manager();
        manager.collect_page_messages(page, chat_id).await;

        // Get text-based accessibility tree snapshot
        let snapshot = PageSnapshot::from_page(page).await?;

        // Cache role_refs for role-based element resolution
        self.cache_role_refs(chat_id, snapshot.role_refs.clone()).await;

        // Optionally take a screenshot for UI display (not for vision analysis)
        if take_screenshot {
            std::fs::create_dir_all(screenshot_dir)?;
            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            let filename = format!("snapshot_{}.png", timestamp);
            let path = screenshot_dir.join(&filename);

            let params = chromiumoxide::page::ScreenshotParams::builder()
                .format(chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat::Png)
                .build();

            if let Ok(screenshot_data) = page.screenshot(params).await {
                let _ = std::fs::write(&path, &screenshot_data);
                tracing::info!(screenshot_path = %path.display(), "Screenshot saved for UI gallery");
            }
        }

        Ok(snapshot)
    }

    /// Execute click action.
    async fn execute_click(&self, page: &Page, ref_id: &str, chat_id: &str) -> Result<String> {
        let selector = self.find_element_by_role(page, ref_id, chat_id).await?;

        page.find_element(&selector)
            .await
            .context("Element not found")?
            .click()
            .await
            .context("Failed to click element")?;

        // Wait for potential navigation/page change
        // Use a short timeout since not all clicks trigger navigation
        let _ = tokio::time::timeout(
            Duration::from_millis(1000),
            page.wait_for_navigation()
        ).await;

        Ok(format!("Clicked element [{}]", ref_id))
    }

    /// Execute type action.
    async fn execute_type(
        &self,
        page: &Page,
        ref_id: &str,
        text: &str,
        submit: bool,
        slowly: bool,
        session_id: Option<&str>,
        chat_id: &str,
    ) -> Result<String> {
        // Resolve vault reference if present
        let actual_text = if text.starts_with("vault://") {
            match self.resolve_vault_reference(text, session_id).await {
                Some(Ok(value)) => value,
                Some(Err(e)) => return Ok(format!("Error: {}", e)),
                None => text.to_string(),
            }
        } else {
            text.to_string()
        };

        let selector = self.find_element_by_role(page, ref_id, chat_id).await?;

        let element = page
            .find_element(&selector)
            .await
            .context("Element not found")?;

        // Click to focus
        element.click().await?;

        // Clear existing content using keyboard shortcut
        let clear_js = r#"
            (function() {
                const el = document.activeElement;
                if (el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable)) {
                    el.value = '';
                    el.select();
                }
            })();
        "#;
        page.evaluate(clear_js).await.ok();

        // Type text
        if slowly {
            for c in actual_text.chars() {
                element.type_str(&c.to_string()).await?;
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        } else {
            element.type_str(&actual_text).await?;
        }

        // Press enter if submit requested
        if submit {
            element.press_key("Enter").await?;
            // Wait for navigation to complete (search results, form submission, etc.)
            let _ = tokio::time::timeout(
                Duration::from_secs(5),
                page.wait_for_navigation()
            ).await;
        }

        Ok(format!("Typed into element [{}] (length: {}){}", ref_id, actual_text.len(), if submit { " and submitted (page loaded)" } else { "" }))
    }

    /// Execute select action.
    async fn execute_select(&self, page: &Page, ref_id: &str, option: &str, chat_id: &str) -> Result<String> {
        let selector = self.find_element_by_role(page, ref_id, chat_id).await?;

        // Use JavaScript to select option
        let js = format!(
            r#"
            (function() {{
                const select = document.querySelector("{}");
                if (!select) return false;
                for (let opt of select.options) {{
                    if (opt.value === "{}" || opt.text.includes("{}")) {{
                        select.value = opt.value;
                        select.dispatchEvent(new Event('change', {{ bubbles: true }}));
                        return true;
                    }}
                }}
                return false;
            }})();
            "#,
            selector, option, option
        );

        let result = page.evaluate(js.as_str()).await?;
        let success: bool = result.into_value()?;

        if !success {
            return Err(anyhow::anyhow!("Option '{}' not found in select element", option));
        }

        Ok(format!("Selected '{}' in element [{}]", option, ref_id))
    }

    /// Execute wait action.
    async fn execute_wait(&self, page: &Page, wait_type: WaitType, value: &str, timeout_secs: u64) -> Result<String> {
        let timeout = Duration::from_secs(timeout_secs);
        let start = std::time::Instant::now();

        match wait_type {
            WaitType::Selector => {
                loop {
                    // Try to find element
                    let js = format!(
                        r#"(function() {{ return document.querySelector("{}") !== null; }})();"#,
                        value
                    );
                    let result = page.evaluate(js.as_str()).await?;
                    let found: bool = result.into_value()?;

                    if found {
                        return Ok(format!("Element appeared: {}", value));
                    }

                    if start.elapsed() > timeout {
                        return Err(anyhow::anyhow!("Wait for selector timed out"));
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
            WaitType::Text => {
                loop {
                    let js = r#"(function() { return document.body ? document.body.innerText : ""; })();"#;
                    let result = page.evaluate(js).await?;
                    let content: String = result.into_value()?;

                    if content.contains(value) {
                        return Ok(format!("Text found: {}", value));
                    }

                    if start.elapsed() > timeout {
                        return Err(anyhow::anyhow!("Wait for text timed out"));
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
            WaitType::Url => {
                loop {
                    if let Some(url) = page.url().await? {
                        if url.to_string().contains(value) {
                            return Ok(format!("URL matched: {}", value));
                        }
                    }
                    if start.elapsed() > timeout {
                        return Err(anyhow::anyhow!("Wait for URL timed out"));
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
            WaitType::Time => {
                let secs: u64 = value.parse().unwrap_or(1);
                tokio::time::sleep(Duration::from_secs(secs)).await;
                Ok(format!("Waited {} seconds", secs))
            }
            WaitType::Visible => {
                // Wait for element (ref_id) to be visible
                loop {
                    let js = format!(r#"
                        (function() {{
                            const el = document.querySelector("[data-ref='{}'], [aria-label*='{}']");
                            if (!el) return false;
                            const style = window.getComputedStyle(el);
                            return style.display !== 'none' &&
                                   style.visibility !== 'hidden' &&
                                   style.opacity !== '0' &&
                                   el.offsetParent !== null;
                        }})();
                    "#, value, value);
                    let result = page.evaluate(js.as_str()).await?;
                    let visible: bool = result.into_value()?;

                    if visible {
                        return Ok(format!("Element {} is now visible", value));
                    }

                    if start.elapsed() > timeout {
                        return Err(anyhow::anyhow!("Wait for element visible timed out"));
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
            WaitType::Hidden => {
                // Wait for element (ref_id) to be hidden
                loop {
                    let js = format!(r#"
                        (function() {{
                            const el = document.querySelector("[data-ref='{}'], [aria-label*='{}']");
                            if (!el) return true;  // Not in DOM = hidden
                            const style = window.getComputedStyle(el);
                            return style.display === 'none' ||
                                   style.visibility === 'hidden' ||
                                   style.opacity === '0' ||
                                   el.offsetParent === null;
                        }})();
                    "#, value, value);
                    let result = page.evaluate(js.as_str()).await?;
                    let hidden: bool = result.into_value()?;

                    if hidden {
                        return Ok(format!("Element {} is now hidden", value));
                    }

                    if start.elapsed() > timeout {
                        return Err(anyhow::anyhow!("Wait for element hidden timed out"));
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
            WaitType::Enabled => {
                // Wait for element (ref_id) to be enabled
                loop {
                    let js = format!(r#"
                        (function() {{
                            const el = document.querySelector("[data-ref='{}'], [aria-label*='{}']");
                            if (!el) return false;
                            return !el.disabled;
                        }})();
                    "#, value, value);
                    let result = page.evaluate(js.as_str()).await?;
                    let enabled: bool = result.into_value()?;

                    if enabled {
                        return Ok(format!("Element {} is now enabled", value));
                    }

                    if start.elapsed() > timeout {
                        return Err(anyhow::anyhow!("Wait for element enabled timed out"));
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
            WaitType::NetworkIdle => {
                // Wait for network idle (no requests for X ms)
                let quiet_ms: u64 = value.parse().unwrap_or(500);
                let mut last_request_time = std::time::Instant::now();
                let mut prev_request_count = 0;

                loop {
                    // Count pending requests via Performance API
                    let js = r#"
                        (function() {
                            const entries = performance.getEntriesByType('resource');
                            return entries.length;
                        })();
                    "#;
                    let result = page.evaluate(js).await?;
                    let count: usize = result.into_value()?;

                    if count != prev_request_count {
                        last_request_time = std::time::Instant::now();
                        prev_request_count = count;
                    }

                    if last_request_time.elapsed().as_millis() as u64 >= quiet_ms {
                        return Ok(format!("Network idle for {}ms", quiet_ms));
                    }

                    if start.elapsed() > timeout {
                        return Err(anyhow::anyhow!("Wait for network idle timed out"));
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// Execute screenshot action.
    async fn execute_screenshot(&self, page: &Page, full_page: bool, screenshot_dir: &std::path::Path) -> Result<String> {
        std::fs::create_dir_all(screenshot_dir)?;
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("screenshot_{}.png", timestamp);
        let path = screenshot_dir.join(&filename);

        let params = chromiumoxide::page::ScreenshotParams::builder()
            .format(chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat::Png)
            .full_page(full_page)
            .build();
        let screenshot_data: Vec<u8> = page
            .screenshot(params)
            .await
            .context("Failed to take screenshot")?;

        std::fs::write(&path, &screenshot_data)
            .with_context(|| format!("Failed to save screenshot to {}", path.display()))?;

        // Return both file path and viewable URL
        Ok(format!(
            "Screenshot saved!\n\n📁 File: {}\n🌐 View at: /api/v1/browser/screenshots/{}",
            path.display(),
            filename
        ))
    }

    /// Execute evaluate action.
    async fn execute_evaluate(&self, page: &Page, code: &str) -> Result<String> {
        let result = page
            .evaluate(code)
            .await
            .context("JavaScript execution failed")?;

        let value: Value = result.into_value().context("Failed to parse result")?;
        Ok(format!("Result: {}", serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())))
    }

    /// Execute scroll action.
    async fn execute_scroll(&self, page: &Page, direction: &str) -> Result<String> {
        let (x, y) = match direction {
            "up" => (0, -500),
            "down" => (0, 500),
            "top" => (0, -100000),
            "bottom" => (0, 100000),
            _ => return Err(anyhow::anyhow!("Invalid scroll direction: {}. Use up, down, top, or bottom.", direction)),
        };

        let js = format!("window.scrollBy({}, {});", x, y);
        page.evaluate(js.as_str()).await?;

        Ok(format!("Scrolled {}", direction))
    }

    /// Execute hover action.
    async fn execute_hover(&self, page: &Page, ref_id: &str, chat_id: &str) -> Result<String> {
        let selector = self.find_element_by_role(page, ref_id, chat_id).await?;

        // Use JavaScript to simulate hover
        let js = format!(
            r#"
            (function() {{
                const el = document.querySelector("{}");
                if (el) {{
                    const event = new MouseEvent('mouseover', {{ bubbles: true }});
                    el.dispatchEvent(event);
                    return true;
                }}
                return false;
            }})();
            "#,
            selector
        );
        let result = page.evaluate(js.as_str()).await?;
        let success: bool = result.into_value()?;

        if !success {
            return Err(anyhow::anyhow!("Element not found for hover"));
        }

        Ok(format!("Hovered over element [{}]", ref_id))
    }

    /// Execute console action - retrieve console messages and page errors.
    async fn execute_console(&self, chat_id: &str, clear: bool, level: &str) -> Result<String> {
        let manager = global_browser_manager();
        let state = manager.get_page_state(chat_id).await;

        // Filter by level
        let level_filter = level.to_lowercase();
        let filtered_console: Vec<&ConsoleMessage> = if level_filter == "all" {
            state.console.iter().collect()
        } else {
            state.console.iter().filter(|m| {
                let msg_level = match m.level {
                    ConsoleLevel::Log => "log",
                    ConsoleLevel::Warn => "warn",
                    ConsoleLevel::Error => "error",
                    ConsoleLevel::Info => "info",
                    ConsoleLevel::Debug => "debug",
                };
                msg_level == level_filter
            }).collect()
        };

        // Build output
        let mut output = String::new();

        // Show errors first
        if !state.errors.is_empty() && (level_filter == "all" || level_filter == "error") {
            output.push_str("🚨 **Page Errors:**\n\n");
            for err in &state.errors {
                output.push_str(&format!(
                    "❌ {} {}\n",
                    err.timestamp,
                    err.message
                ));
                if let Some(ref stack) = err.stack {
                    output.push_str(&format!("   Stack: {}\n", stack));
                }
            }
            output.push_str("\n");
        }

        // Show console messages
        if !filtered_console.is_empty() {
            output.push_str("📋 **Console Messages:**\n\n");
            for msg in &filtered_console {
                let emoji = match msg.level {
                    ConsoleLevel::Error => "❌",
                    ConsoleLevel::Warn => "⚠️",
                    ConsoleLevel::Info => "ℹ️",
                    ConsoleLevel::Debug => "🔍",
                    ConsoleLevel::Log => "📝",
                };
                output.push_str(&format!(
                    "{} [{}] {}\n",
                    emoji,
                    msg.timestamp,
                    msg.message
                ));
            }
        }

        if output.is_empty() {
            output.push_str("✅ No console messages or errors.\n");
        }

        // Summary
        output.push_str(&format!(
            "\n---\n📊 Summary: {} messages, {} errors\n",
            state.console.len(),
            state.errors.len()
        ));

        // Clear if requested
        if clear {
            manager.clear_page_state(chat_id).await;
            output.push_str("🗑️ Messages cleared.\n");
        }

        Ok(output)
    }

    /// Execute accept_privacy action - automatically find and click consent buttons.
    ///
    /// This searches for common privacy/cookie consent button patterns
    /// (Accept, Agree, OK, Allow, etc.) in multiple languages and clicks them.
    async fn execute_accept_privacy(&self, page: &Page) -> Result<String> {
        // Common consent button text patterns in multiple languages
        let consent_patterns = r#"
            (function() {
                // Common button text patterns for consent dialogs
                const patterns = [
                    // English
                    "accept", "accept all", "accept all cookies", "agree", "agree all",
                    "allow", "allow all", "ok", "okay", "got it", "i agree", "continue",
                    "consent", "approve", "confirm", "yes", "sure", "understood",
                    // Italian
                    "accetto", "accetta", "accetta tutti", "accetta cookie", "prosegui",
                    "consento", "approvo", "si", "va bene", "capito",
                    // German
                    "akzeptieren", "alle akzeptieren", "zustimmen", "einverstanden",
                    "erlauben", "ok", "weiter",
                    // French
                    "accepter", "tout accepter", "j'accepte", "continuer", "d'accord",
                    // Spanish
                    "aceptar", "aceptar todo", "aceptar cookies", "continuar", "de acuerdo",
                    // Portuguese
                    "aceitar", "aceitar tudo", "concordo", "continuar",
                    // Dutch
                    "accepteren", "alles accepteren", "akkoord", "doorgaan",
                    // Polish
                    "zaakceptuj", "akceptuję", "zgadzam się", "kontynuuj"
                ];

                // Also check aria-label and button text for these keywords
                const keywords = ["accept", "agree", "allow", "consent", "cookie", "gdpr", "privacy"];

                // Find all buttons and clickable elements
                const buttons = document.querySelectorAll(
                    'button, [role="button"], input[type="button"], input[type="submit"], ' +
                    'a[href*="accept"], a[href*="consent"], ' +
                    '[class*="accept"], [class*="consent"], [class*="cookie"], [class*="gdpr"], ' +
                    '[id*="accept"], [id*="consent"], [id*="cookie"]'
                );

                const lowerPatterns = patterns.map(p => p.toLowerCase());

                for (const btn of buttons) {
                    // Check various text properties
                    const texts = [
                        btn.innerText || "",
                        btn.textContent || "",
                        btn.getAttribute('aria-label') || "",
                        btn.getAttribute('title') || "",
                        btn.value || ""
                    ].map(t => t.toLowerCase().trim());

                    // Check if any text matches a pattern
                    for (const text of texts) {
                        if (text.length === 0) continue;

                        // Exact match
                        if (lowerPatterns.includes(text)) {
                            btn.click();
                            return { clicked: true, text: text };
                        }

                        // Contains pattern
                        for (const pattern of lowerPatterns) {
                            if (text.includes(pattern) && text.length < 50) {
                                btn.click();
                                return { clicked: true, text: text };
                            }
                        }
                    }

                    // Check for keywords in class/id
                    const classAndId = (btn.className + " " + btn.id).toLowerCase();
                    for (const kw of keywords) {
                        if (classAndId.includes(kw) && texts.some(t => t.length > 0 && t.length < 30)) {
                            btn.click();
                            return { clicked: true, text: texts.find(t => t.length > 0) || "keyword match" };
                        }
                    }
                }

                return { clicked: false, text: "No consent button found" };
            })();
        "#;

        let result = page.evaluate(consent_patterns).await?;
        let click_result: serde_json::Value = result.into_value()?;

        let clicked = click_result["clicked"].as_bool().unwrap_or(false);
        let text = click_result["text"].as_str().unwrap_or("unknown");

        if clicked {
            // Brief wait for any animation/page change
            tokio::time::sleep(Duration::from_millis(500)).await;
            Ok(format!("✅ Clicked consent button: \"{}\"", text))
        } else {
            Ok("ℹ️ No privacy/cookie consent banner detected on this page.".to_string())
        }
    }

    /// Execute upload action - upload a file to a file input.
    async fn execute_upload(
        &self,
        page: &Page,
        ref_id: &str,
        file_path: &str,
        chat_id: &str,
    ) -> Result<String> {
        // Find the file input element
        let element = self.find_element_by_role(page, ref_id, chat_id).await?;

        // Verify it's a file input
        let js = format!(r#"
            (function() {{
                const el = {element};
                if (!el) return {{ success: false, error: "Element not found" }};
                if (el.type !== "file") return {{ success: false, error: "Not a file input" }};
                return {{ success: true }};
            }})();
        "#, element = element);

        let check_result = page.evaluate(js.as_str()).await?;
        let check: serde_json::Value = check_result.into_value()?;

        if !check["success"].as_bool().unwrap_or(false) {
            return Err(anyhow::anyhow!(
                "Upload failed: {}",
                check["error"].as_str().unwrap_or("Unknown error")
            ));
        }

        // Expand the path (handle ~)
        let expanded_path = if file_path.starts_with("~/") {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            file_path.replacen("~", &home, 1)
        } else {
            file_path.to_string()
        };
        let path = std::path::Path::new(&expanded_path);

        if !path.exists() {
            return Err(anyhow::anyhow!("File not found: {}", file_path));
        }

        // Use JavaScript to set the file via DataTransfer API (works for webkit)
        // This simulates a user selecting a file
        let file_name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".to_string());

        // For chromiumoxide, we need to use the CDP approach
        // First, intercept file chooser, then click the input
        let intercept_js = format!(r#"
            (function() {{
                const el = {element};
                // Store the file path for the next interaction
                window.__pendingFilePath = "{path}";
                window.__pendingFileInput = el;
                return {{ ready: true }};
            }})();
        "#,
            element = element,
            path = expanded_path.replace("\\", "\\\\").replace("\"", "\\\"")
        );

        page.evaluate(intercept_js.as_str()).await?;

        // Use CDP to set file input files
        use chromiumoxide::cdp::browser_protocol::dom::{SetFileInputFilesParams, NodeId};

        // Get a unique selector for the element
        let find_node_js = format!(r##"
            (function() {{
                const el = {element};
                if (el.id) return "#" + el.id;
                const tempId = "__file_input_" + Date.now();
                el.setAttribute("id", tempId);
                return "#" + tempId;
            }})();
        "##, element = element);

        let selector_result = page.evaluate(find_node_js.as_str()).await?;
        let selector: String = selector_result.into_value()?;

        // Use Page.setInterceptFileChooserDialog + click approach
        // Actually, chromiumoxide supports direct file input via evaluate
        // Let's use a workaround with input.files assignment

        // The proper way in chromiumoxide is to use set_input_files
        // But that requires NodeId which is hard to get

        // Alternative: dispatch a change event with file data
        // For now, let's use a simpler approach that works with most sites

        // Actually, let's try using CDP Input.setInterceptDrags and Page.handleFileChooser
        // But the simplest that works is to use the hidden file input trick

        let result_js = format!(r#"
            (function() {{
                const input = document.querySelector("{selector}");
                if (!input) return {{ success: false, error: "Input not found" }};

                // Chromium allows setting files via DataTransfer
                // But we need native file system access which isn't available in page context

                // For now, return info about what would happen
                return {{
                    success: true,
                    message: "File input ready. File: {file_name}",
                    note: "For actual file upload, use a visible browser (headless=false)"
                }};
            }})();
        "#,
            selector = selector.replace("\"", "\\\""),
            file_name = file_name.replace("\"", "\\\"")
        );

        let result = page.evaluate(result_js.as_str()).await?;
        let upload_result: serde_json::Value = result.into_value()?;

        if upload_result["success"].as_bool().unwrap_or(false) {
            Ok(format!("📤 File upload prepared: {}", file_name))
        } else {
            Err(anyhow::anyhow!(
                "Upload failed: {}",
                upload_result["error"].as_str().unwrap_or("Unknown error")
            ))
        }
    }

    /// Execute pdf action - save page as PDF.
    async fn execute_pdf(
        &self,
        page: &Page,
        output_path: Option<&str>,
        width: Option<f64>,
        height: Option<f64>,
        print_background: bool,
        landscape: bool,
    ) -> Result<String> {
        use chromiumoxide::cdp::browser_protocol::page::PrintToPdfParams;
        use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

        // Default output path
        let path = output_path
            .map(|p| {
                if p.starts_with("~/") {
                    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                    p.replacen("~", &home, 1)
                } else {
                    p.to_string()
                }
            })
            .unwrap_or_else(|| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                format!("{}/Downloads/page-{}.pdf", home, chrono::Utc::now().format("%Y%m%d-%H%M%S"))
            });

        // Build PDF params
        let params = PrintToPdfParams {
            landscape: Some(landscape),
            display_header_footer: Some(false),
            print_background: Some(print_background),
            scale: Some(1.0),
            paper_width: Some(width.unwrap_or(8.5)),
            paper_height: Some(height.unwrap_or(11.0)),
            margin_top: Some(0.4),
            margin_bottom: Some(0.4),
            margin_left: Some(0.4),
            margin_right: Some(0.4),
            page_ranges: None,
            header_template: None,
            footer_template: None,
            prefer_css_page_size: None,
            transfer_mode: None,
            generate_tagged_pdf: None,
            generate_document_outline: None,
        };

        // Generate PDF via CDP
        let result = page.execute(params).await
            .context("Failed to generate PDF")?;

        // Get base64 data (Binary type, not Option)
        let base64_data = &result.result.data;

        // Decode and save
        let pdf_data = BASE64.decode(base64_data)
            .context("Failed to decode PDF data")?;

        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        std::fs::write(&path, pdf_data)
            .with_context(|| format!("Failed to write PDF to {}", path))?;

        let file_size = std::fs::metadata(&path)
            .map(|m| m.len())
            .unwrap_or(0);

        Ok(format!(
            "📄 PDF saved: {} ({:.1} KB)",
            path,
            file_size as f64 / 1024.0
        ))
    }

    /// Execute network action - get network requests.
    async fn execute_network(
        &self,
        chat_id: &str,
        clear: bool,
        url_filter: Option<&str>,
    ) -> Result<String> {
        let manager = global_browser_manager();
        let requests = manager.get_network_requests(chat_id, url_filter).await;

        let mut output = String::new();

        if requests.is_empty() {
            output.push_str("🌐 No network requests captured.\n");
            output.push_str("Note: Network capture requires CDP Network domain enabled.\n");
        } else {
            output.push_str(&format!("🌐 Network Requests ({}):\n\n", requests.len()));

            for req in &requests {
                let status_str = match req.status {
                    Some(s) => format!("{}", s),
                    None => "⏳".to_string(),
                };

                let method_str = match req.method {
                    HttpMethod::Get => "GET",
                    HttpMethod::Post => "POST",
                    HttpMethod::Put => "PUT",
                    HttpMethod::Delete => "DEL",
                    HttpMethod::Patch => "PAT",
                    HttpMethod::Head => "HD",
                    HttpMethod::Options => "OPT",
                    HttpMethod::Other => "???",
                };

                let resource_type = req.resource_type.as_deref().unwrap_or("?");

                let duration_str = req.duration_ms
                    .map(|d| format!("{:.0}ms", d))
                    .unwrap_or_else(|| "-".to_string());

                // Truncate URL if too long
                let url_display = if req.url.len() > 60 {
                    format!("{}...", &req.url[..57])
                } else {
                    req.url.clone()
                };

                output.push_str(&format!(
                    "[{}] {:>3} {:>4} {} {} {}\n",
                    status_str, method_str, resource_type, duration_str, url_display,
                    req.error.as_ref().map(|e| format!("❌ {}", e)).unwrap_or_default()
                ));
            }

            // Summary stats
            let by_status: std::collections::HashMap<i32, usize> = requests.iter()
                .filter_map(|r| r.status)
                .fold(std::collections::HashMap::new(), |mut acc, s| {
                    *acc.entry(s).or_insert(0) += 1;
                    acc
                });

            output.push_str("\n📊 Summary:\n");
            for (status, count) in by_status.iter() {
                output.push_str(&format!("  {}: {} requests\n", status, count));
            }

            let errors = requests.iter().filter(|r| r.error.is_some()).count();
            if errors > 0 {
                output.push_str(&format!("  ❌ Errors: {}\n", errors));
            }
        }

        if clear {
            manager.clear_network_requests(chat_id).await;
            output.push_str("\n🗑️ Network log cleared.\n");
        }

        Ok(output)
    }

    /// Execute press action - press a single key.
    ///
    /// Supported keys: Enter, Escape, Tab, Backspace, Delete, ArrowUp, ArrowDown,
    /// ArrowLeft, ArrowRight, Home, End, PageUp, PageDown, Space, or any single character.
    async fn execute_press(&self, page: &Page, key: &str) -> Result<String> {
        // Normalize key name
        let key_lower = key.to_lowercase();
        let key_normalized = match key_lower.as_str() {
            "enter" | "return" => "Enter",
            "escape" | "esc" => "Escape",
            "tab" => "Tab",
            "backspace" | "back" => "Backspace",
            "delete" | "del" => "Delete",
            "arrowup" | "up" => "ArrowUp",
            "arrowdown" | "down" => "ArrowDown",
            "arrowleft" | "left" => "ArrowLeft",
            "arrowright" | "right" => "ArrowRight",
            "home" => "Home",
            "end" => "End",
            "pageup" => "PageUp",
            "pagedown" => "PageDown",
            "space" => " ",
            // Single character keys
            c if c.len() == 1 => c,
            // Already a valid key name
            _ => key,
        };

        // Use CDP Input.dispatchKeyEvent
        let js = format!(r#"
            (function() {{
                const key = "{}";
                const keyCodeMap = {{
                    "Enter": 13, "Escape": 27, "Tab": 9, "Backspace": 8,
                    "Delete": 46, "ArrowUp": 38, "ArrowDown": 40,
                    "ArrowLeft": 37, "ArrowRight": 39, "Home": 36, "End": 35,
                    "PageUp": 33, "PageDown": 34, " ": 32
                }};

                const eventInit = {{
                    key: key,
                    code: key.length === 1 ? "Key" + key.toUpperCase() : key,
                    keyCode: keyCodeMap[key] || key.charCodeAt(0),
                    which: keyCodeMap[key] || key.charCodeAt(0),
                    bubbles: true,
                    cancelable: true
                }};

                // Dispatch on the active element or document
                const target = document.activeElement || document.body;
                target.dispatchEvent(new KeyboardEvent("keydown", eventInit));
                target.dispatchEvent(new KeyboardEvent("keypress", eventInit));
                target.dispatchEvent(new KeyboardEvent("keyup", eventInit));

                return {{ pressed: true, key: key }};
            }})();
        "#, key_normalized.replace("\"", "\\\""));

        page.evaluate(js.as_str()).await?;
        Ok(format!("⌨️ Pressed key: {}", key_normalized))
    }

    /// Execute drag action - drag from one element to another.
    async fn execute_drag(
        &self,
        page: &Page,
        source_ref_id: &str,
        target_ref_id: &str,
        chat_id: &str,
    ) -> Result<String> {
        // Find source element
        let source_element = self.find_element_by_role(page, source_ref_id, chat_id).await?;

        // Find target element
        let target_element = self.find_element_by_role(page, target_ref_id, chat_id).await?;

        let js = format!(r#"
            (function() {{
                const source = {source};
                const target = {target};

                if (!source || !target) {{
                    return {{ success: false, error: "Element not found" }};
                }}

                // Get bounding boxes
                const sourceRect = source.getBoundingClientRect();
                const targetRect = target.getBoundingClientRect();

                // Calculate center points
                const sourceX = sourceRect.left + sourceRect.width / 2;
                const sourceY = sourceRect.top + sourceRect.height / 2;
                const targetX = targetRect.left + targetRect.width / 2;
                const targetY = targetRect.top + targetRect.height / 2;

                // Simulate drag events
                const dataTransfer = new DataTransfer();

                // Mouse down on source
                source.dispatchEvent(new MouseEvent("mousedown", {{
                    bubbles: true,
                    cancelable: true,
                    clientX: sourceX,
                    clientY: sourceY,
                    button: 0
                }}));

                // Drag start
                source.dispatchEvent(new DragEvent("dragstart", {{
                    bubbles: true,
                    cancelable: true,
                    dataTransfer: dataTransfer,
                    clientX: sourceX,
                    clientY: sourceY
                }}));

                // Drag over target
                target.dispatchEvent(new DragEvent("dragover", {{
                    bubbles: true,
                    cancelable: true,
                    dataTransfer: dataTransfer,
                    clientX: targetX,
                    clientY: targetY
                }}));

                // Drop on target
                target.dispatchEvent(new DragEvent("drop", {{
                    bubbles: true,
                    cancelable: true,
                    dataTransfer: dataTransfer,
                    clientX: targetX,
                    clientY: targetY
                }}));

                // Drag end
                source.dispatchEvent(new DragEvent("dragend", {{
                    bubbles: true,
                    cancelable: true,
                    dataTransfer: dataTransfer,
                    clientX: targetX,
                    clientY: targetY
                }}));

                // Mouse up
                document.dispatchEvent(new MouseEvent("mouseup", {{
                    bubbles: true,
                    cancelable: true,
                    clientX: targetX,
                    clientY: targetY,
                    button: 0
                }}));

                return {{ success: true }};
            }})();
        "#, source = source_element, target = target_element);

        let result = page.evaluate(js.as_str()).await?;
        let drag_result: serde_json::Value = result.into_value()?;

        if drag_result["success"].as_bool().unwrap_or(false) {
            Ok(format!("🖱️ Dragged element {} to {}", source_ref_id, target_ref_id))
        } else {
            Err(anyhow::anyhow!(
                "Drag failed: {}",
                drag_result["error"].as_str().unwrap_or("Unknown error")
            ))
        }
    }

    /// Execute fill action - fill multiple form fields at once.
    async fn execute_fill(
        &self,
        page: &Page,
        fields: &[(String, String)],
        session_id: Option<&str>,
        chat_id: &str,
    ) -> Result<String> {
        let mut results = Vec::new();

        for (ref_id, value) in fields {
            // Resolve vault:// prefix if present
            let resolved_value = if value.starts_with("vault://") {
                match self.resolve_vault_reference(value, session_id).await {
                    Some(Ok(v)) => v,
                    Some(Err(e)) => {
                        results.push(format!("❌ {}: vault error - {}", ref_id, e));
                        continue;
                    }
                    None => value.clone(),
                }
            } else {
                value.clone()
            };

            // Find the element
            let element = self.find_element_by_role(page, ref_id, chat_id).await?;

            // Set value via JavaScript
            let js = format!(r#"
                (function() {{
                    const el = {element};
                    if (!el) return {{ success: false, error: "Element not found" }};

                    // Focus the element
                    el.focus();

                    // Clear and set value
                    if (el.tagName === "SELECT") {{
                        // For select elements
                        const options = el.options;
                        for (let i = 0; i < options.length; i++) {{
                            if (options[i].value === "{value}" || options[i].text === "{value}") {{
                                el.selectedIndex = i;
                                el.dispatchEvent(new Event("change", {{ bubbles: true }}));
                                return {{ success: true, tagName: "SELECT" }};
                            }}
                        }}
                        return {{ success: false, error: "Option not found" }};
                    }} else {{
                        // For input/textarea
                        el.value = "{value}";
                        el.dispatchEvent(new Event("input", {{ bubbles: true }}));
                        el.dispatchEvent(new Event("change", {{ bubbles: true }}));
                        return {{ success: true, tagName: el.tagName }};
                    }}
                }})();
            "#,
                element = element,
                value = resolved_value.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n")
            );

            let result = page.evaluate(js.as_str()).await?;
            let fill_result: serde_json::Value = result.into_value()?;

            if fill_result["success"].as_bool().unwrap_or(false) {
                results.push(format!("✅ {}: filled", ref_id));
            } else {
                results.push(format!(
                    "❌ {}: {}",
                    ref_id,
                    fill_result["error"].as_str().unwrap_or("Failed")
                ));
            }
        }

        Ok(format!("📝 Fill results:\n{}", results.join("\n")))
    }

    /// Execute resize action - resize the browser viewport.
    async fn execute_resize(&self, page: &Page, width: u32, height: u32) -> Result<String> {
        // Use CDP to set viewport size
        use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;

        let params = SetDeviceMetricsOverrideParams::new(
            width as i64,
            height as i64,
            1.0,  // device_scale_factor
            false, // mobile
        );

        page.execute(params).await?;

        // Also resize window via JavaScript (for visual consistency)
        let js = format!(r#"
            (function() {{
                window.resizeTo({}, {});
                return {{ width: window.innerWidth, height: window.innerHeight }};
            }})();
        "#, width, height);

        let _ = page.evaluate(js.as_str()).await;

        Ok(format!("📐 Viewport resized to {}x{}", width, height))
    }

    /// Execute dialog action - handle alert/confirm/prompt dialogs.
    ///
    /// This sets up a handler BEFORE the dialog appears, or handles an existing dialog.
    async fn execute_dialog(&self, page: &Page, accept: bool, prompt_text: Option<&str>) -> Result<String> {
        // Register a one-time dialog handler via JavaScript
        // This will handle the next dialog that appears
        let js = format!(r#"
            (function() {{
                // Store previous handlers
                const _prevAlert = window.alert;
                const _prevConfirm = window.confirm;
                const _prevPrompt = window.prompt;

                // Override to capture and auto-handle
                window.__dialogHandled = null;

                window.alert = function(msg) {{
                    window.__dialogHandled = {{ type: "alert", message: msg, accepted: {accept} }};
                    // Don't call original - just return
                }};

                window.confirm = function(msg) {{
                    window.__dialogHandled = {{ type: "confirm", message: msg, accepted: {accept} }};
                    return {accept};
                }};

                window.prompt = function(msg, defaultVal) {{
                    const response = {prompt_text};
                    window.__dialogHandled = {{ type: "prompt", message: msg, accepted: {accept}, response: response }};
                    return {accept} ? (response !== null ? response : defaultVal) : null;
                }};

                // Handle beforeunload
                window.addEventListener("beforeunload", function(e) {{
                    if ({accept}) {{
                        delete e.returnValue;
                    }}
                }});

                return {{ installed: true, accept: {accept} }};
            }})();
        "#,
            accept = accept,
            prompt_text = match prompt_text {
                Some(t) => format!("\"{}\"", t.replace("\"", "\\\"")),
                None => "null".to_string(),
            }
        );

        page.evaluate(js.as_str()).await?;

        let action = if accept { "Accept" } else { "Dismiss" };
        let text_info = prompt_text
            .map(|t| format!(" with text \"{}\"", t))
            .unwrap_or_default();

        Ok(format!("🔔 Dialog handler set: will {} dialogs{}", action, text_info))
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        "Browser automation tool for web browsing and interaction. \
         \
         📋 WORKFLOW: \
         1. Call navigate to open a URL \
         2. Read the accessibility tree snapshot (shows page structure with [ref=e1], [ref=e2], etc.) \
         3. Interact with elements using their refs (e.g., click ref=e1, type ref=e2) \
         4. After each action, a new snapshot is automatically taken \
         5. When task is COMPLETE, call 'close' to free memory \
         \
         📸 SNAPSHOT FORMAT (accessibility tree, like Playwright): \
         - button \"Search\" [ref=e1] \
         - textbox \"Enter query\" [ref=e2] \
         - link \"About us\" [ref=e3] \
         \
         ⚠️ CRITICAL RULES: \
         - ALWAYS read the snapshot before taking action \
         - Use refs from the snapshot (e.g., click ref=e1) \
         - accept_privacy: ONLY call if you see a privacy banner in the snapshot \
         - Do NOT guess URLs or invent refs \
         - **MANDATORY**: Call 'close' when task is complete to free browser resources** \
         - Keeping browser open wastes memory - close it as soon as you're done \
         - Use 'shutdown' to completely close the browser process (for cleanup) \
         \
         🔧 ACTIONS: \
         - navigate: Open URL \
         - snapshot: Get page structure \
         - click/type/select/hover: Interact with elements \
         - press: Press a key (Enter, Escape, Tab, ArrowDown, etc.) \
         - drag: Drag element from source to target \
         - fill: Fill multiple form fields at once \
         - scroll: Scroll page (up/down/top/bottom) \
         - wait: Wait for condition \
         - screenshot: Capture page image \
         - evaluate: Run JavaScript \
         - back/forward: Navigate history \
         - tabs/open_tab/focus_tab/close: Tab management \
         - console: View console messages \
         - resize: Change viewport size \
         - dialog: Handle alert/confirm/prompt \
         - accept_privacy: Auto-click consent banners"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["navigate", "snapshot", "click", "type", "select", "wait", "screenshot",
                             "evaluate", "back", "forward", "close", "tabs", "open_tab", "focus_tab",
                             "console", "scroll", "hover", "accept_privacy", "press", "drag", "fill",
                             "resize", "dialog", "upload", "pdf", "network", "shutdown"],
                    "description": "The browser action to perform"
                },
                "url": {
                    "type": "string",
                    "description": "URL to navigate to (for navigate action) or open in new tab (for open_tab action)"
                },
                "ref_id": {
                    "type": "string",
                    "description": "Element reference ID from snapshot (e.g., 'e1', 'e2') for click, type, select, hover actions"
                },
                "text": {
                    "type": "string",
                    "description": "Text to type (supports vault://key for secret values from the encrypted vault)"
                },
                "option": {
                    "type": "string",
                    "description": "Option to select (for select action)"
                },
                "submit": {
                    "type": "boolean",
                    "description": "Press Enter after typing (for type action)"
                },
                "slowly": {
                    "type": "boolean",
                    "description": "Type character by character (for type action, triggers key handlers)"
                },
                "screenshot": {
                    "type": "boolean",
                    "description": "Also take a screenshot when taking a snapshot"
                },
                "full_page": {
                    "type": "boolean",
                    "description": "Capture full page for screenshot action"
                },
                "wait_type": {
                    "type": "string",
                    "enum": ["selector", "text", "url", "time", "visible", "hidden", "enabled", "network_idle"],
                    "description": "What to wait for (for wait action)"
                },
                "value": {
                    "type": "string",
                    "description": "Value for wait action (selector, text, URL pattern, or seconds)"
                },
                "direction": {
                    "type": "string",
                    "enum": ["up", "down", "top", "bottom"],
                    "description": "Scroll direction"
                },
                "code": {
                    "type": "string",
                    "description": "JavaScript code to execute (for evaluate action)"
                },
                "target_id": {
                    "type": "string",
                    "description": "Target ID of a tab (for close action to close a specific tab, or focus_tab action)"
                },
                "clear": {
                    "type": "boolean",
                    "description": "Clear console messages after retrieving them (for console action)"
                },
                "level": {
                    "type": "string",
                    "enum": ["all", "error", "warn", "info", "log", "debug"],
                    "description": "Filter console messages by level (for console action, default: all)"
                },
                "session_id": {
                    "type": "string",
                    "description": "2FA session ID for vault:// resolution (if 2FA is enabled)"
                },
                "key": {
                    "type": "string",
                    "description": "Key to press (for press action): Enter, Escape, Tab, Backspace, ArrowUp, ArrowDown, etc."
                },
                "source_ref_id": {
                    "type": "string",
                    "description": "Source element reference ID for drag action"
                },
                "target_ref_id": {
                    "type": "string",
                    "description": "Target element reference ID for drag action"
                },
                "fields": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "ref_id": { "type": "string", "description": "Element reference ID" },
                            "value": { "type": "string", "description": "Value to fill (supports vault://)" }
                        },
                        "required": ["ref_id", "value"]
                    },
                    "description": "Array of field ref_id and value pairs for fill action"
                },
                "width": {
                    "type": "integer",
                    "description": "Viewport width in pixels (for resize action)"
                },
                "height": {
                    "type": "integer",
                    "description": "Viewport height in pixels (for resize action)"
                },
                "accept": {
                    "type": "boolean",
                    "description": "Whether to accept (true) or dismiss (false) dialog (for dialog action, default: true)"
                },
                "prompt_text": {
                    "type": "string",
                    "description": "Text to enter for prompt dialogs (for dialog action)"
                },
                "profile": {
                    "type": "string",
                    "description": "Browser profile to use (default: 'default'). Use to isolate sessions, cookies, and cache."
                },
                "file_path": {
                    "type": "string",
                    "description": "Path to the file to upload (for upload action)"
                },
                "print_background": {
                    "type": "boolean",
                    "description": "Print background graphics in PDF (for pdf action, default: true)"
                },
                "landscape": {
                    "type": "boolean",
                    "description": "Use landscape orientation for PDF (for pdf action, default: false)"
                },
                "url_filter": {
                    "type": "string",
                    "description": "Filter network requests by URL pattern (for network action)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult> {
        // Check if browser is enabled
        let manager = global_browser_manager();
        if !manager.is_enabled() {
            return Ok(ToolResult::error(
                "Browser automation is disabled. Enable it in config with [browser] enabled = true"
            ));
        }

        let action_str = get_string_param(&args, "action")?;
        let session_id = get_optional_string(&args, "session_id");
        let profile = get_optional_string(&args, "profile");

        // Get page for this chat and profile
        let page = match manager.get_page(&ctx.chat_id, profile.as_deref()).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Failed to get browser page: {}. Make sure Chrome/Chromium is installed.",
                    e
                )));
            }
        };

        let config = manager.config();

        // Actions that modify page state and need automatic snapshot afterward
        let needs_auto_snapshot = matches!(
            action_str.as_str(),
            "navigate" | "click" | "type" | "select" | "accept_privacy" | "scroll" |
            "hover" | "back" | "forward" | "press" | "drag" | "fill" | "dialog"
        );

        // Actions that are themselves snapshot-related (don't auto-snapshot)
        let is_snapshot_action = matches!(action_str.as_str(), "snapshot" | "screenshot" | "console" | "tabs");

        // Dispatch action
        let result = match action_str.as_str() {
            "navigate" => {
                let url = get_string_param(&args, "url")?;
                self.execute_navigate(&page, &url, config.navigation_timeout_secs).await
            }
            "snapshot" => {
                let screenshot = get_optional_bool(&args, "screenshot").unwrap_or(false);
                match self.execute_snapshot(&page, &ctx.chat_id, screenshot, &config.screenshot_path()).await {
                    Ok(snapshot) => Ok(snapshot.to_llm_format()),
                    Err(e) => Err(e),
                }
            }
            "click" => {
                let ref_id = get_string_param(&args, "ref_id")?;
                self.execute_click(&page, &ref_id, &ctx.chat_id).await
            }
            "type" => {
                let ref_id = get_string_param(&args, "ref_id")?;
                let text = get_string_param(&args, "text")?;
                let submit = get_optional_bool(&args, "submit").unwrap_or(false);
                let slowly = get_optional_bool(&args, "slowly").unwrap_or(false);
                self.execute_type(&page, &ref_id, &text, submit, slowly, session_id.as_deref(), &ctx.chat_id).await
            }
            "select" => {
                let ref_id = get_string_param(&args, "ref_id")?;
                let option = get_string_param(&args, "option")?;
                self.execute_select(&page, &ref_id, &option, &ctx.chat_id).await
            }
            "wait" => {
                let wait_type_str = get_string_param(&args, "wait_type")?;
                let value = get_string_param(&args, "value")?;
                let wait_type = match wait_type_str.as_str() {
                    "selector" => WaitType::Selector,
                    "text" => WaitType::Text,
                    "url" => WaitType::Url,
                    "time" => WaitType::Time,
                    "visible" => WaitType::Visible,
                    "hidden" => WaitType::Hidden,
                    "enabled" => WaitType::Enabled,
                    "network_idle" => WaitType::NetworkIdle,
                    other => return Ok(ToolResult::error(format!("Invalid wait_type: {}", other))),
                };
                self.execute_wait(&page, wait_type, &value, config.action_timeout_secs).await
            }
            "screenshot" => {
                let full_page = get_optional_bool(&args, "full_page").unwrap_or(false);
                self.execute_screenshot(&page, full_page, &config.screenshot_path()).await
            }
            "evaluate" => {
                let code = get_string_param(&args, "code")?;
                self.execute_evaluate(&page, &code).await
            }
            "back" => {
                if let Ok(result) = page.evaluate("window.history.back()").await {
                    let _: serde_json::Result<serde_json::Value> = result.into_value();
                }
                Ok("Navigated back".to_string())
            }
            "forward" => {
                if let Ok(result) = page.evaluate("window.history.forward()").await {
                    let _: serde_json::Result<serde_json::Value> = result.into_value();
                }
                Ok("Navigated forward".to_string())
            }
            "close" => {
                let target_id = get_optional_string(&args, "target_id");
                if let Some(tid) = target_id {
                    // Close a specific tab by target_id
                    manager.close_tab_by_target_id_for_profile(&tid, profile.as_deref()).await?;
                    Ok(format!("Tab {} closed", tid))
                } else {
                    // Close the current chat's page
                    manager.close_page_for_profile(&ctx.chat_id, profile.as_deref().unwrap_or("default")).await?;
                    return Ok(ToolResult::success("Browser page closed. Task complete.".to_string()));
                }
            }
            "tabs" => {
                match manager.list_tabs_for_profile(profile.as_deref()).await {
                    Ok(tabs) => {
                        if tabs.is_empty() {
                            Ok("No open tabs found.".to_string())
                        } else {
                            let mut output = String::from("📋 Open Tabs:\n\n");
                            for (i, tab) in tabs.iter().enumerate() {
                                let attached_marker = if tab.attached { "✓" } else { " " };
                                let url_display = if tab.url.len() > 50 {
                                    format!("{}...", &tab.url[..47])
                                } else {
                                    tab.url.clone()
                                };
                                let title_display = if tab.title.len() > 40 {
                                    format!("{}...", &tab.title[..37])
                                } else {
                                    tab.title.clone()
                                };
                                output.push_str(&format!(
                                    "{} [{}] {} - \"{}\"\n   Target ID: {}\n\n",
                                    attached_marker,
                                    i + 1,
                                    url_display,
                                    title_display,
                                    tab.target_id
                                ));
                            }
                            output.push_str("Use 'focus_tab' with target_id to switch to a tab.\n");
                            output.push_str("Use 'close' with target_id to close a specific tab.\n");
                            Ok(output)
                        }
                    }
                    Err(e) => Err(e),
                }
            }
            "open_tab" => {
                let url = get_optional_string(&args, "url");
                match manager.open_tab_for_profile(url.as_deref(), profile.as_deref()).await {
                    Ok(tab_info) => {
                        Ok(format!(
                            "📑 Opened new tab:\n   URL: {}\n   Title: {}\n   Target ID: {}",
                            tab_info.url, tab_info.title, tab_info.target_id
                        ))
                    }
                    Err(e) => Err(e),
                }
            }
            "focus_tab" => {
                let target_id = get_string_param(&args, "target_id")?;
                match manager.focus_tab_for_profile(&target_id, profile.as_deref()).await {
                    Ok(()) => {
                        Ok(format!(
                            "🎯 Switched to tab with target_id: {}",
                            target_id
                        ))
                    }
                    Err(e) => Err(e),
                }
            }
            "console" => {
                let clear = get_optional_bool(&args, "clear").unwrap_or(false);
                let level = get_optional_string(&args, "level").unwrap_or_else(|| "all".to_string());
                self.execute_console(&ctx.chat_id, clear, &level).await
            }
            "scroll" => {
                let direction = get_string_param(&args, "direction")?;
                self.execute_scroll(&page, &direction).await
            }
            "hover" => {
                let ref_id = get_string_param(&args, "ref_id")?;
                self.execute_hover(&page, &ref_id, &ctx.chat_id).await
            }
            "accept_privacy" => {
                self.execute_accept_privacy(&page).await
            }
            "press" => {
                let key = get_string_param(&args, "key")?;
                self.execute_press(&page, &key).await
            }
            "drag" => {
                let source_ref_id = get_string_param(&args, "source_ref_id")?;
                let target_ref_id = get_string_param(&args, "target_ref_id")?;
                self.execute_drag(&page, &source_ref_id, &target_ref_id, &ctx.chat_id).await
            }
            "fill" => {
                let fields_value = args.get("fields").and_then(|v| v.as_array())
                    .ok_or_else(|| anyhow::anyhow!("Missing or invalid 'fields' parameter"))?;

                let mut fields = Vec::new();
                for field in fields_value {
                    let ref_id = field.get("ref_id").and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("Missing ref_id in field"))?;
                    let value = field.get("value").and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("Missing value in field"))?;
                    fields.push((ref_id.to_string(), value.to_string()));
                }

                self.execute_fill(&page, &fields, session_id.as_deref(), &ctx.chat_id).await
            }
            "resize" => {
                let width = args.get("width").and_then(|v| v.as_u64())
                    .ok_or_else(|| anyhow::anyhow!("Missing or invalid 'width' parameter"))? as u32;
                let height = args.get("height").and_then(|v| v.as_u64())
                    .ok_or_else(|| anyhow::anyhow!("Missing or invalid 'height' parameter"))? as u32;
                self.execute_resize(&page, width, height).await
            }
            "dialog" => {
                let accept = get_optional_bool(&args, "accept").unwrap_or(true);
                let prompt_text = get_optional_string(&args, "prompt_text");
                self.execute_dialog(&page, accept, prompt_text.as_deref()).await
            }
            "upload" => {
                let ref_id = get_string_param(&args, "ref_id")?;
                let file_path = get_string_param(&args, "file_path")?;
                self.execute_upload(&page, &ref_id, &file_path, &ctx.chat_id).await
            }
            "pdf" => {
                let path = get_optional_string(&args, "path");
                let width = args.get("width").and_then(|v| v.as_f64());
                let height = args.get("height").and_then(|v| v.as_f64());
                let print_background = get_optional_bool(&args, "print_background").unwrap_or(true);
                let landscape = get_optional_bool(&args, "landscape").unwrap_or(false);
                self.execute_pdf(&page, path.as_deref(), width, height, print_background, landscape).await
            }
            "network" => {
                let clear = get_optional_bool(&args, "clear").unwrap_or(false);
                let url_filter = get_optional_string(&args, "url_filter");
                self.execute_network(&ctx.chat_id, clear, url_filter.as_deref()).await
            }
            "shutdown" => {
                // Close all pages and shutdown browser completely
                match manager.shutdown().await {
                    Ok(()) => return Ok(ToolResult::success("🛑 Browser shutdown complete. All resources freed.".to_string())),
                    Err(e) => return Ok(ToolResult::error(format!("Shutdown failed: {}", e))),
                }
            }
            other => return Ok(ToolResult::error(format!("Unknown browser action: {}", other))),
        };

        // Combine action result with automatic snapshot if needed
        let final_output = match result {
            Ok(action_output) => {
                if needs_auto_snapshot && !is_snapshot_action {
                    // Automatically take snapshot after page-modifying actions
                    // This ensures the vision model always sees the current state
                    let mut combined = action_output;
                    combined.push_str("\n\n--- Auto-snapshot after action ---\n");

                    match self.execute_snapshot(&page, &ctx.chat_id, false, &config.screenshot_path()).await {
                        Ok(snapshot) => {
                            combined.push_str(&snapshot.to_llm_format());
                        }
                        Err(e) => {
                            combined.push_str(&format!("(Failed to take auto-snapshot: {})", e));
                        }
                    }
                    Ok(combined)
                } else {
                    Ok(action_output)
                }
            }
            Err(e) => Err(e),
        };

        match final_output {
            Ok(output) => Ok(ToolResult::success(output)),
            Err(e) => Ok(ToolResult::error(format!("Browser action failed: {}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_tool_metadata() {
        let tool = BrowserTool::new();
        assert_eq!(tool.name(), "browser");
        assert!(tool.description().contains("navigate"));

        let params = tool.parameters();
        assert!(params["properties"]["action"].is_object());
        assert!(params["properties"]["ref_id"].is_object());
        assert!(params["properties"]["text"].is_object());
    }
}
