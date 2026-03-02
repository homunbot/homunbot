//! Browser manager — singleton that manages browser lifecycle and page isolation.
//!
//! Uses CDP events (not JS injection) for console/network/error capture,
//! matching the approach used by OpenClaw (Playwright events → CDP protocol).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use chromiumoxide::browser::{Browser, BrowserConfig as ChromeConfig};
use chromiumoxide::cdp::browser_protocol::network::{
    EventLoadingFailed, EventRequestWillBeSent, EventResponseReceived,
};
use chromiumoxide::cdp::browser_protocol::target::{GetTargetsParams, TargetInfo};
use chromiumoxide::cdp::js_protocol::runtime::{
    ConsoleApiCalledType, EventConsoleApiCalled, EventExceptionThrown,
};
use chromiumoxide::page::Page;
use futures::StreamExt;
use serde::Serialize;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

use crate::config::BrowserConfig;

/// Global browser manager instance.
static BROWSER_MANAGER: std::sync::OnceLock<Arc<BrowserManager>> = std::sync::OnceLock::new();

/// Maximum number of console messages to keep per page
const MAX_CONSOLE_MESSAGES: usize = 100;
/// Maximum number of page errors to keep per page
const MAX_PAGE_ERRORS: usize = 50;
/// Maximum number of network requests to keep per page
const MAX_NETWORK_REQUESTS: usize = 200;

/// Console message type (matches CDP ConsoleAPICalled)
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConsoleLevel {
    Log,
    Warn,
    Error,
    Info,
    Debug,
}

/// A console message from the browser.
#[derive(Debug, Clone, Serialize)]
pub struct ConsoleMessage {
    /// Message level (log, warn, error, etc.)
    pub level: ConsoleLevel,
    /// The message text
    pub message: String,
    /// Timestamp (ISO 8601)
    pub timestamp: String,
    /// Source URL (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Line number (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<i32>,
}

/// A page error (JavaScript exception or unhandled rejection).
#[derive(Debug, Clone, Serialize)]
pub struct PageError {
    /// Error message
    pub message: String,
    /// Stack trace (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
    /// Timestamp (ISO 8601)
    pub timestamp: String,
    /// Source URL (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Line number (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<i32>,
}

/// HTTP method type
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
    Other,
}

/// A captured network request.
#[derive(Debug, Clone, Serialize)]
pub struct NetworkRequest {
    /// Request ID (CDP internal)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// HTTP method
    pub method: HttpMethod,
    /// Request URL
    pub url: String,
    /// Response status code (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    /// Response status text (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_text: Option<String>,
    /// Content type (from response headers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Request timestamp
    pub timestamp: String,
    /// Response timestamp (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_timestamp: Option<String>,
    /// Time in milliseconds (if response received)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
    /// Resource type (Document, Script, Stylesheet, Image, XHR, Fetch, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    /// Error message (if request failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// State tracked per page (console messages, errors, network requests)
#[derive(Debug, Clone, Default)]
pub struct PageState {
    /// Console messages (limited to MAX_CONSOLE_MESSAGES)
    pub console: Vec<ConsoleMessage>,
    /// Page errors (limited to MAX_PAGE_ERRORS)
    pub errors: Vec<PageError>,
    /// Network requests (limited to MAX_NETWORK_REQUESTS)
    pub network: Vec<NetworkRequest>,
}

impl PageState {
    /// Add a console message, respecting the limit.
    pub fn add_console(&mut self, msg: ConsoleMessage) {
        if self.console.len() >= MAX_CONSOLE_MESSAGES {
            self.console.remove(0);
        }
        self.console.push(msg);
    }

    /// Add a page error, respecting the limit.
    pub fn add_error(&mut self, err: PageError) {
        if self.errors.len() >= MAX_PAGE_ERRORS {
            self.errors.remove(0);
        }
        self.errors.push(err);
    }

    /// Add a network request, respecting the limit.
    pub fn add_network(&mut self, req: NetworkRequest) {
        if self.network.len() >= MAX_NETWORK_REQUESTS {
            self.network.remove(0);
        }
        self.network.push(req);
    }

    /// Clear all messages, errors, and network requests.
    pub fn clear(&mut self) {
        self.console.clear();
        self.errors.clear();
        self.network.clear();
    }

    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Get network request by URL pattern
    pub fn filter_network(&self, url_pattern: Option<&str>) -> Vec<&NetworkRequest> {
        match url_pattern {
            Some(pattern) => self
                .network
                .iter()
                .filter(|r| r.url.contains(pattern))
                .collect(),
            None => self.network.iter().collect(),
        }
    }
}

/// Information about a browser tab.
#[derive(Debug, Clone, Serialize)]
pub struct TabInfo {
    /// CDP target ID (unique identifier for the tab)
    pub target_id: String,
    /// Tab URL
    pub url: String,
    /// Tab title
    pub title: String,
    /// Whether this tab is attached/controlled by us
    pub attached: bool,
}

/// Manages browser instances with multi-profile support and per-session page isolation.
///
/// Architecture:
/// - Multiple browser processes, one per profile (lazy-initialized)
/// - Each (profile, chat_id) gets its own page (tab isolation)
/// - Pages are cached and reused within a session
/// - Handler tasks tracked per browser for proper shutdown
/// - State (console, errors) tracked per page
pub struct BrowserManager {
    /// Browser instances per profile (profile_name -> Browser)
    browsers: RwLock<HashMap<String, Arc<Browser>>>,
    /// Per-profile, per-chat pages ((profile, chat_id) -> Page)
    pages: Mutex<HashMap<(String, String), Arc<Page>>>,
    /// Per-chat state (console messages, errors) - key is chat_id
    page_states: Mutex<HashMap<String, PageState>>,
    /// Background handler tasks per profile (profile_name -> JoinHandle)
    handler_tasks: Mutex<HashMap<String, JoinHandle<()>>>,
    /// Configuration
    config: BrowserConfig,
}

impl BrowserManager {
    /// Create a new browser manager with the given configuration.
    fn new(config: BrowserConfig) -> Self {
        Self {
            browsers: RwLock::new(HashMap::new()),
            pages: Mutex::new(HashMap::new()),
            page_states: Mutex::new(HashMap::new()),
            handler_tasks: Mutex::new(HashMap::new()),
            config,
        }
    }

    /// Get or create the global browser manager.
    pub fn global() -> Arc<BrowserManager> {
        BROWSER_MANAGER
            .get_or_init(|| {
                let config = crate::config::Config::load()
                    .map(|c| c.browser)
                    .unwrap_or_default();
                Arc::new(Self::new(config))
            })
            .clone()
    }

    /// Initialize with explicit config (for testing or custom setup).
    pub fn init_with_config(config: BrowserConfig) -> Arc<BrowserManager> {
        BROWSER_MANAGER
            .get_or_init(|| Arc::new(Self::new(config)))
            .clone()
    }

    /// Get the browser configuration.
    pub fn config(&self) -> &BrowserConfig {
        &self.config
    }

    /// Check if browser is enabled in config.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get page state for a chat_id (console messages, errors).
    pub async fn get_page_state(&self, chat_id: &str) -> PageState {
        let states = self.page_states.lock().await;
        states.get(chat_id).cloned().unwrap_or_default()
    }

    /// Clear page state for a chat_id.
    pub async fn clear_page_state(&self, chat_id: &str) {
        let mut states = self.page_states.lock().await;
        if let Some(state) = states.get_mut(chat_id) {
            state.clear();
        }
    }

    /// Add a console message for a chat_id.
    pub async fn add_console_message(&self, chat_id: &str, msg: ConsoleMessage) {
        let mut states = self.page_states.lock().await;
        let state = states.entry(chat_id.to_string()).or_default();
        state.add_console(msg);
    }

    /// Add a page error for a chat_id.
    pub async fn add_page_error(&self, chat_id: &str, err: PageError) {
        let mut states = self.page_states.lock().await;
        let state = states.entry(chat_id.to_string()).or_default();
        state.add_error(err);
    }

    /// Add a network request for a chat_id.
    pub async fn add_network_request(&self, chat_id: &str, req: NetworkRequest) {
        let mut states = self.page_states.lock().await;
        let state = states.entry(chat_id.to_string()).or_default();
        state.add_network(req);
    }

    /// Get network requests for a chat_id.
    pub async fn get_network_requests(
        &self,
        chat_id: &str,
        url_filter: Option<&str>,
    ) -> Vec<NetworkRequest> {
        let states = self.page_states.lock().await;
        match states.get(chat_id) {
            Some(state) => state
                .filter_network(url_filter)
                .into_iter()
                .cloned()
                .collect(),
            None => Vec::new(),
        }
    }

    /// Clear network requests for a chat_id.
    pub async fn clear_network_requests(&self, chat_id: &str) {
        let mut states = self.page_states.lock().await;
        if let Some(state) = states.get_mut(chat_id) {
            state.network.clear();
        }
    }

    /// Check if a page has errors.
    pub async fn page_has_errors(&self, chat_id: &str) -> bool {
        let states = self.page_states.lock().await;
        states.get(chat_id).map(|s| s.has_errors()).unwrap_or(false)
    }

    /// Test if browser can be launched.
    ///
    /// Test if browser can be launched.
    ///
    /// This attempts to start the browser and returns success/failure.
    /// Used by the web UI to verify browser configuration.
    pub async fn test_connection(&self) -> Result<()> {
        self.ensure_browser(None).await?;
        Ok(())
    }

    /// Start the browser for a specific profile if not already running.
    ///
    /// This launches Chrome/Chromium with CDP enabled for the given profile.
    async fn ensure_browser(&self, profile_name: Option<&str>) -> Result<Arc<Browser>> {
        let (profile_key, _profile) = self.config.get_profile(profile_name);
        let profile_name = profile_key.clone();

        // Fast path: already initialized
        {
            let guard = self.browsers.read().await;
            if let Some(browser) = guard.get(&profile_name) {
                return Ok(browser.clone());
            }
        }

        // Slow path: initialize browser
        let mut guard = self.browsers.write().await;

        // Double-check after acquiring write lock
        if let Some(browser) = guard.get(&profile_name) {
            return Ok(browser.clone());
        }

        tracing::info!(profile = %profile_name, "Starting browser for automation...");

        // Get executable path
        let executable = self.config.resolved_executable()
            .context("No Chrome/Chromium executable found. Please install Chrome or set browser.executable_path in config.")?;

        // Get profile-specific settings
        let headless = self.config.headless_for_profile(&profile_name);
        let user_data_dir = self.config.profile_user_data_path(&profile_name);

        tracing::info!(
            executable = %executable.display(),
            headless = headless,
            profile = %profile_name,
            user_data = %user_data_dir.display(),
            "Launching browser"
        );

        // Create profile directory
        std::fs::create_dir_all(&user_data_dir).with_context(|| {
            format!(
                "Failed to create browser profile dir: {}",
                user_data_dir.display()
            )
        })?;

        // Build browser config with STEALTH settings to avoid bot detection
        let mut chrome_config = ChromeConfig::builder()
            .chrome_executable(std::path::PathBuf::from(&executable))
            .user_data_dir(&user_data_dir)
            // STEALTH: Hide navigator.webdriver flag
            .hide()
            // More realistic window size (1920x1080 is most common)
            .window_size(1920, 1080)
            // Additional stealth arguments
            .arg("disable-blink-features=AutomationControlled")
            .arg("disable-infobars")
            .arg("disable-dev-shm-usage")
            .arg("disable-browser-side-navigation")
            .arg("disable-gpu");

        // Add profile-specific args
        if let Some(profile) = self.config.profiles.get(&profile_name) {
            for arg in &profile.args {
                chrome_config = chrome_config.arg(arg.clone());
            }
        }

        if headless {
            // Use new headless mode (less detectable)
            chrome_config = chrome_config.new_headless_mode();
        } else {
            // Non-headless mode: call with_head() to show the browser window
            chrome_config = chrome_config.with_head().no_sandbox();
        }

        let chrome_config = chrome_config
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build browser config: {}", e))?;

        // Launch browser - returns (Browser, Handler)
        let (browser, mut handler) = Browser::launch(chrome_config)
            .await
            .context("Failed to launch browser. Make sure Chrome/Chromium is installed.")?;

        // IMPORTANT: Spawn a background task to poll the handler
        let profile_name_clone = profile_name.clone();
        let handler_handle = tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                tracing::trace!(profile = %profile_name_clone, ?event, "Browser event");
            }
            tracing::info!(profile = %profile_name_clone, "Browser handler task finished");
        });

        // Store handler task for cleanup
        {
            let mut tasks = self.handler_tasks.lock().await;
            tasks.insert(profile_name.clone(), handler_handle);
        }

        let browser = Arc::new(browser);
        guard.insert(profile_name.clone(), browser.clone());

        tracing::info!(profile = %profile_name, "Browser started successfully");
        Ok(browser)
    }

    /// Get or create a page for a specific chat/session with optional profile.
    ///
    /// Each (profile, chat_id) gets its own page (tab) for isolation.
    pub async fn get_page(&self, chat_id: &str, profile_name: Option<&str>) -> Result<Arc<Page>> {
        let (profile_key, _) = self.config.get_profile(profile_name);
        let profile_name = profile_key.clone();
        let page_key = (profile_name.clone(), chat_id.to_string());

        // Check if we have a cached page
        {
            let pages = self.pages.lock().await;
            if let Some(page) = pages.get(&page_key) {
                return Ok(page.clone());
            }
        }

        // Create new page
        let browser = self.ensure_browser(Some(&profile_name)).await?;
        let page = browser
            .new_page("about:blank")
            .await
            .context("Failed to create new browser page")?;

        let page = Arc::new(page);

        // Set up CDP event listeners for console, errors, and network.
        // Unlike the old JS injection approach, CDP events survive page navigations.
        self.setup_cdp_listeners(&page, chat_id).await?;

        // Cache the page
        {
            let mut pages = self.pages.lock().await;
            pages.insert(page_key, page.clone());
        }

        tracing::debug!(chat_id = %chat_id, profile = %profile_name, "Created new browser page");
        Ok(page)
    }

    /// Get or create a page using the default profile (convenience method).
    pub async fn get_page_default(&self, chat_id: &str) -> Result<Arc<Page>> {
        self.get_page(chat_id, None).await
    }

    /// Set up CDP event listeners for console messages, exceptions, and network requests.
    ///
    /// This replaces the old JS injection approach (`setup_console_capture`).
    /// CDP events are emitted at the protocol level and survive page navigations,
    /// matching OpenClaw's approach of using `page.on("console")` etc.
    async fn setup_cdp_listeners(&self, page: &Page, chat_id: &str) -> Result<()> {
        // --- Console messages (Runtime.consoleAPICalled) ---
        let mut console_events = page
            .event_listener::<EventConsoleApiCalled>()
            .await
            .context("Failed to subscribe to console events")?;

        let manager_for_console = BROWSER_MANAGER.get().cloned();
        let chat_id_console = chat_id.to_string();
        tokio::spawn(async move {
            while let Some(event) = console_events.next().await {
                let level = match event.r#type {
                    ConsoleApiCalledType::Log => ConsoleLevel::Log,
                    ConsoleApiCalledType::Warning => ConsoleLevel::Warn,
                    ConsoleApiCalledType::Error => ConsoleLevel::Error,
                    ConsoleApiCalledType::Info => ConsoleLevel::Info,
                    ConsoleApiCalledType::Debug => ConsoleLevel::Debug,
                    _ => ConsoleLevel::Log,
                };

                // Serialize args to readable text (like OpenClaw's msg.text())
                let message = event
                    .args
                    .iter()
                    .filter_map(|arg| {
                        arg.value
                            .as_ref()
                            .map(|v| match v {
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            })
                            .or_else(|| arg.description.clone())
                    })
                    .collect::<Vec<_>>()
                    .join(" ");

                let msg = ConsoleMessage {
                    level,
                    message,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    url: None,
                    line: None,
                };

                if let Some(ref mgr) = manager_for_console {
                    mgr.add_console_message(&chat_id_console, msg).await;
                }
            }
        });

        // --- Exceptions (Runtime.exceptionThrown) ---
        let mut exception_events = page
            .event_listener::<EventExceptionThrown>()
            .await
            .context("Failed to subscribe to exception events")?;

        let manager_for_errors = BROWSER_MANAGER.get().cloned();
        let chat_id_errors = chat_id.to_string();
        tokio::spawn(async move {
            while let Some(event) = exception_events.next().await {
                let details = &event.exception_details;
                let message = details
                    .exception
                    .as_ref()
                    .and_then(|e| e.description.clone())
                    .unwrap_or_else(|| details.text.clone());

                let stack = details.stack_trace.as_ref().map(|st| {
                    st.call_frames
                        .iter()
                        .map(|f| {
                            format!(
                                "  at {} ({}:{}:{})",
                                f.function_name, f.url, f.line_number, f.column_number
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                });

                let err = PageError {
                    message,
                    stack,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    url: details.url.clone(),
                    line: Some(details.line_number as i32),
                };

                if let Some(ref mgr) = manager_for_errors {
                    mgr.add_page_error(&chat_id_errors, err).await;
                }
            }
        });

        // --- Network: request sent (Network.requestWillBeSent) ---
        let mut request_events = page
            .event_listener::<EventRequestWillBeSent>()
            .await
            .context("Failed to subscribe to network request events")?;

        let manager_for_requests = BROWSER_MANAGER.get().cloned();
        let chat_id_requests = chat_id.to_string();
        tokio::spawn(async move {
            while let Some(event) = request_events.next().await {
                let method = match event.request.method.to_uppercase().as_str() {
                    "GET" => HttpMethod::Get,
                    "POST" => HttpMethod::Post,
                    "PUT" => HttpMethod::Put,
                    "DELETE" => HttpMethod::Delete,
                    "PATCH" => HttpMethod::Patch,
                    "HEAD" => HttpMethod::Head,
                    "OPTIONS" => HttpMethod::Options,
                    _ => HttpMethod::Other,
                };

                let resource_type = event.r#type.as_ref().map(|rt| format!("{:?}", rt));

                let req = NetworkRequest {
                    request_id: Some(event.request_id.inner().to_string()),
                    method,
                    url: event.request.url.clone(),
                    status: None,
                    status_text: None,
                    content_type: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    response_timestamp: None,
                    duration_ms: None,
                    resource_type,
                    error: None,
                };

                if let Some(ref mgr) = manager_for_requests {
                    mgr.add_network_request(&chat_id_requests, req).await;
                }
            }
        });

        // --- Network: response received (Network.responseReceived) ---
        let mut response_events = page
            .event_listener::<EventResponseReceived>()
            .await
            .context("Failed to subscribe to network response events")?;

        let manager_for_responses = BROWSER_MANAGER.get().cloned();
        let chat_id_responses = chat_id.to_string();
        tokio::spawn(async move {
            while let Some(event) = response_events.next().await {
                let request_id = event.request_id.inner().to_string();
                if let Some(ref mgr) = manager_for_responses {
                    let mut states = mgr.page_states.lock().await;
                    if let Some(state) = states.get_mut(&chat_id_responses) {
                        // Find the matching request and enrich it with response data
                        if let Some(req) = state
                            .network
                            .iter_mut()
                            .rev()
                            .find(|r| r.request_id.as_deref() == Some(&request_id))
                        {
                            req.status = Some(event.response.status as i32);
                            req.status_text = Some(event.response.status_text.clone());
                            req.response_timestamp = Some(chrono::Utc::now().to_rfc3339());
                            // Use mime_type from Response (cleaner than parsing headers)
                            if !event.response.mime_type.is_empty() {
                                req.content_type = Some(event.response.mime_type.clone());
                            }
                        }
                    }
                }
            }
        });

        // --- Network: loading failed (Network.loadingFailed) ---
        let mut failed_events = page
            .event_listener::<EventLoadingFailed>()
            .await
            .context("Failed to subscribe to network failure events")?;

        let manager_for_failures = BROWSER_MANAGER.get().cloned();
        let chat_id_failures = chat_id.to_string();
        tokio::spawn(async move {
            while let Some(event) = failed_events.next().await {
                let request_id = event.request_id.inner().to_string();
                if let Some(ref mgr) = manager_for_failures {
                    let mut states = mgr.page_states.lock().await;
                    if let Some(state) = states.get_mut(&chat_id_failures) {
                        if let Some(req) = state
                            .network
                            .iter_mut()
                            .rev()
                            .find(|r| r.request_id.as_deref() == Some(&request_id))
                        {
                            req.error = Some(event.error_text.clone());
                        }
                    }
                }
            }
        });

        tracing::debug!(chat_id = %chat_id, "CDP event listeners set up (console, errors, network)");
        Ok(())
    }

    /// Close the page for a specific chat (closes from all profiles).
    /// Also shuts down browsers that have no more pages.
    pub async fn close_page(&self, chat_id: &str) -> Result<()> {
        let mut pages = self.pages.lock().await;

        // Find and remove all pages for this chat_id across all profiles
        let keys_to_remove: Vec<(String, String)> = pages
            .keys()
            .filter(|(_, cid)| cid == chat_id)
            .cloned()
            .collect();

        // Track which profiles had pages removed
        let mut affected_profiles: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for key in keys_to_remove {
            if let Some(page) = pages.remove(&key) {
                affected_profiles.insert(key.0.clone());
                match Arc::try_unwrap(page) {
                    Ok(page) => {
                        if let Err(e) = page.close().await {
                            tracing::warn!(error = %e, "Failed to close browser page via CDP");
                        } else {
                            tracing::info!(chat_id = %chat_id, profile = %key.0, "Browser page closed");
                        }
                    }
                    Err(arc) => {
                        tracing::debug!(chat_id = %chat_id, "Browser page has other references, dropping");
                        drop(arc);
                    }
                }
            }
        }

        // Check which profiles now have no pages
        let profiles_to_shutdown: Vec<String> = affected_profiles
            .into_iter()
            .filter(|profile| !pages.keys().any(|(p, _)| p == profile))
            .collect();

        drop(pages); // Release lock before shutting down

        // Shut down browsers with no remaining pages
        for profile in profiles_to_shutdown {
            tracing::info!(profile = %profile, "No more pages for profile, shutting down browser");
            self.shutdown_profile(&profile).await?;
        }

        Ok(())
    }

    /// Close the page for a specific chat and profile.
    /// If this was the last page for the profile, also closes the browser to free resources.
    pub async fn close_page_for_profile(&self, chat_id: &str, profile_name: &str) -> Result<()> {
        let page_key = (profile_name.to_string(), chat_id.to_string());
        let mut pages = self.pages.lock().await;

        if let Some(page) = pages.remove(&page_key) {
            match Arc::try_unwrap(page) {
                Ok(page) => {
                    if let Err(e) = page.close().await {
                        tracing::warn!(error = %e, "Failed to close browser page via CDP");
                    } else {
                        tracing::info!(chat_id = %chat_id, profile = %profile_name, "Browser page closed");
                    }
                }
                Err(arc) => {
                    tracing::debug!(chat_id = %chat_id, profile = %profile_name, "Browser page has other references, dropping");
                    drop(arc);
                }
            }
        }

        // Check if any pages remain for this profile
        let has_remaining_pages = pages.keys().any(|(profile, _)| profile == profile_name);
        drop(pages); // Release lock before shutting down

        // If no more pages for this profile, shut down the browser to free resources
        if !has_remaining_pages {
            tracing::info!(profile = %profile_name, "No more pages for profile, shutting down browser");
            self.shutdown_profile(profile_name).await?;
        }

        Ok(())
    }

    /// Shut down the browser for a specific profile only.
    pub async fn shutdown_profile(&self, profile_name: &str) -> Result<()> {
        let mut browsers = self.browsers.write().await;
        let mut handler_tasks = self.handler_tasks.lock().await;

        if let Some((profile, browser)) = browsers.remove_entry(profile_name) {
            tracing::info!(profile = %profile, "Shutting down browser for profile");

            // Stop the handler task first
            if let Some(task) = handler_tasks.remove(&profile) {
                task.abort();
                tracing::debug!(profile = %profile, "Aborted handler task");
            }

            match Arc::try_unwrap(browser) {
                Ok(mut browser) => {
                    if let Err(e) = browser.close().await {
                        tracing::warn!(profile = %profile, error = %e, "Failed to close browser gracefully");
                    } else {
                        tracing::info!(profile = %profile, "Browser closed successfully");
                    }
                }
                Err(arc) => {
                    tracing::debug!(profile = %profile, "Browser has other references, dropping Arc");
                    drop(arc);
                }
            }
        }

        Ok(())
    }

    /// Close a tab by its target ID (uses default profile).
    pub async fn close_tab_by_target_id(&self, target_id: &str) -> Result<()> {
        self.close_tab_by_target_id_for_profile(target_id, None)
            .await
    }

    /// Close a tab by its target ID for a specific profile.
    pub async fn close_tab_by_target_id_for_profile(
        &self,
        target_id: &str,
        profile_name: Option<&str>,
    ) -> Result<()> {
        let browser = self.ensure_browser(profile_name).await?;

        let close_cmd = chromiumoxide::cdp::browser_protocol::target::CloseTargetParams {
            target_id: target_id.to_string().into(),
        };

        browser
            .execute(close_cmd)
            .await
            .context("Failed to close tab via CDP")?;

        tracing::info!(target_id = %target_id, "Tab closed");
        Ok(())
    }

    /// List all open tabs in the browser (uses default profile).
    pub async fn list_tabs(&self) -> Result<Vec<TabInfo>> {
        self.list_tabs_for_profile(None).await
    }

    /// List all open tabs for a specific profile.
    pub async fn list_tabs_for_profile(&self, profile_name: Option<&str>) -> Result<Vec<TabInfo>> {
        let browser = self.ensure_browser(profile_name).await?;

        let targets_result = browser
            .execute(GetTargetsParams::default())
            .await
            .context("Failed to get browser targets")?;

        let tabs: Vec<TabInfo> = targets_result
            .result
            .target_infos
            .into_iter()
            .filter(|t| t.r#type == "page")
            .map(|t| TabInfo {
                target_id: t.target_id.into(),
                url: t.url,
                title: t.title,
                attached: t.attached,
            })
            .collect();

        Ok(tabs)
    }

    /// Open a new tab with an optional URL (uses default profile).
    pub async fn open_tab(&self, url: Option<&str>) -> Result<TabInfo> {
        self.open_tab_for_profile(url, None).await
    }

    /// Open a new tab for a specific profile.
    pub async fn open_tab_for_profile(
        &self,
        url: Option<&str>,
        profile_name: Option<&str>,
    ) -> Result<TabInfo> {
        let browser = self.ensure_browser(profile_name).await?;

        let target_url = url.unwrap_or("about:blank");
        let page = browser
            .new_page(target_url)
            .await
            .context("Failed to create new tab")?;

        let target_id: String = page.target_id().clone().into();

        let page_url = page
            .url()
            .await
            .ok()
            .flatten()
            .map(|u| u.to_string())
            .unwrap_or_else(|| target_url.to_string());

        let title = page
            .get_title()
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "New Tab".to_string());

        Ok(TabInfo {
            target_id,
            url: page_url,
            title,
            attached: true,
        })
    }

    /// Focus/switch to a specific tab by target ID (uses default profile).
    pub async fn focus_tab(&self, target_id: &str) -> Result<()> {
        self.focus_tab_for_profile(target_id, None).await
    }

    /// Focus/switch to a specific tab for a profile.
    pub async fn focus_tab_for_profile(
        &self,
        target_id: &str,
        profile_name: Option<&str>,
    ) -> Result<()> {
        let browser = self.ensure_browser(profile_name).await?;

        let tabs = self.list_tabs_for_profile(profile_name).await?;
        let target_exists = tabs.iter().any(|t| t.target_id == target_id);

        if !target_exists {
            return Err(anyhow::anyhow!(
                "Tab with target_id '{}' not found",
                target_id
            ));
        }

        let activate_cmd = chromiumoxide::cdp::browser_protocol::target::ActivateTargetParams {
            target_id: target_id.to_string().into(),
        };

        browser
            .execute(activate_cmd)
            .await
            .context("Failed to activate tab via CDP")?;

        tracing::info!(target_id = %target_id, "Tab focused");
        Ok(())
    }

    /// List all available browser profiles.
    pub fn list_profiles(&self) -> Vec<(&String, &crate::config::BrowserProfile)> {
        self.config.profiles.iter().collect()
    }

    /// Get the default profile name.
    pub fn default_profile(&self) -> &str {
        &self.config.default_profile
    }

    /// Close all pages and shut down all browser instances.
    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("Shutting down browser manager...");

        // Clear all pages
        {
            let mut pages = self.pages.lock().await;
            let count = pages.len();
            pages.clear();
            tracing::debug!("Cleared {} browser pages", count);
        }

        // Abort all handler tasks
        {
            let mut tasks = self.handler_tasks.lock().await;
            for (profile, handle) in tasks.drain() {
                handle.abort();
                tracing::debug!(profile = %profile, "Aborted browser handler task");
            }
        }

        // Close all browser instances
        {
            let mut browsers = self.browsers.write().await;
            for (profile, browser) in browsers.drain() {
                match Arc::try_unwrap(browser) {
                    Ok(mut browser) => {
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(5),
                            browser.close(),
                        )
                        .await
                        {
                            Ok(Ok(_)) => {
                                tracing::info!(profile = %profile, "Browser closed gracefully")
                            }
                            Ok(Err(e)) => {
                                tracing::warn!(profile = %profile, error = %e, "Browser close returned error")
                            }
                            Err(_) => {
                                tracing::warn!(profile = %profile, "Browser close timed out after 5s")
                            }
                        }
                    }
                    Err(arc) => {
                        tracing::debug!(profile = %profile, "Browser has other references, dropping Arc");
                        drop(arc);
                    }
                }
            }
        }

        tracing::info!("Browser shutdown complete");
        Ok(())
    }

    /// Check if any browser is currently running.
    pub async fn is_running(&self) -> bool {
        let guard = self.browsers.read().await;
        !guard.is_empty()
    }

    /// Check if a specific profile's browser is running.
    pub async fn is_profile_running(&self, profile_name: &str) -> bool {
        let guard = self.browsers.read().await;
        guard.contains_key(profile_name)
    }

    /// Get the number of active pages.
    pub async fn page_count(&self) -> usize {
        self.pages.lock().await.len()
    }

    /// Get the number of running browsers.
    pub async fn browser_count(&self) -> usize {
        self.browsers.read().await.len()
    }
}

/// Get the global browser manager instance.
pub fn global_browser_manager() -> Arc<BrowserManager> {
    BrowserManager::global()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_manager_creation() {
        let config = BrowserConfig::default();
        let manager = BrowserManager::new(config);
        assert!(!manager.is_enabled());
    }

    #[test]
    fn test_global_manager() {
        let manager = global_browser_manager();
        assert!(Arc::strong_count(&manager) >= 1);
    }
}
