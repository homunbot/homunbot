//! Unified browser automation tool.
//!
//! Wraps ~40 individual Playwright MCP tools behind a single `browser` tool
//! with an `action` enum. This drastically reduces cognitive load on weaker
//! LLM models which struggle to orchestrate many tools — they just send
//! `{action: "click", ref: "e42"}` and Rust handles the rest.
//!
//! Orchestration intelligence lives here:
//! - Auto-snapshot after `type` to detect autocomplete suggestions
//! - Ref normalization (strips common model mistakes)
//! - Snapshot compaction (filter to interactive elements only)
//! - Consecutive snapshot guard

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::RwLock;

use super::mcp::McpPeer;
use super::registry::{Tool, ToolContext, ToolResult};

/// Default idle timeout before auto-closing the browser (seconds).
pub const BROWSER_IDLE_TIMEOUT_SECS: u64 = 300; // 5 minutes

/// Shared browser session state, readable by the agent loop.
///
/// Tracks whether the browser is active, what page it's on, and when
/// the last action was. The agent loop uses this to:
/// 1. Inject a continuation hint ("browser is still on X, continue from there")
/// 2. Auto-close the browser after idle timeout
pub struct BrowserSession {
    last_url: RwLock<Option<String>>,
    last_action_at: RwLock<Option<Instant>>,
    peer: Arc<McpPeer>,
}

impl BrowserSession {
    fn new(peer: Arc<McpPeer>) -> Self {
        Self {
            last_url: RwLock::new(None),
            last_action_at: RwLock::new(None),
            peer,
        }
    }

    /// Record that a browser action just happened, optionally with a URL.
    async fn note_action(&self, url: Option<&str>) {
        *self.last_action_at.write().await = Some(Instant::now());
        if let Some(u) = url {
            *self.last_url.write().await = Some(u.to_string());
        }
    }

    /// Returns a continuation hint if the browser is still active on a page.
    /// The agent loop injects this as a system/user message so the model
    /// knows it can continue from the current page instead of restarting.
    pub async fn continuation_hint(&self) -> Option<String> {
        let url = self.last_url.read().await;
        let last_at = self.last_action_at.read().await;
        match (&*url, &*last_at) {
            (Some(u), Some(_)) => Some(format!(
                "Browser is still open on: {u}\n\
                 You can continue from the current page — call snapshot() to see it.\n\
                 Do NOT navigate again to the same site unless you need a different page."
            )),
            _ => None,
        }
    }

    /// Close the browser if it has been idle longer than the timeout.
    /// Returns `true` if the browser was closed.
    pub async fn close_if_idle(&self, timeout_secs: u64) -> bool {
        let last_at = self.last_action_at.read().await;
        let idle = match *last_at {
            Some(t) => t.elapsed().as_secs() >= timeout_secs,
            None => false, // never used → nothing to close
        };
        drop(last_at);

        if idle {
            tracing::info!(timeout_secs, "Browser idle timeout reached, auto-closing");
            let _ = self.peer.call_tool("browser_close", json!({})).await;
            *self.last_url.write().await = None;
            *self.last_action_at.write().await = None;
            true
        } else {
            false
        }
    }

    /// Clear session state (called after explicit close).
    async fn clear(&self) {
        *self.last_url.write().await = None;
        *self.last_action_at.write().await = None;
    }
}

/// Single unified browser tool that wraps Playwright MCP actions.
pub struct BrowserTool {
    peer: Arc<McpPeer>,
    /// Tracks whether the last action was a snapshot (consecutive guard).
    last_was_snapshot: AtomicBool,
    /// Whether anti-detection scripts have been injected for this session.
    stealth_injected: AtomicBool,
    /// Shared session state, also held by the agent loop.
    session: Arc<BrowserSession>,
}

impl BrowserTool {
    pub fn new(peer: Arc<McpPeer>) -> Self {
        let session = Arc::new(BrowserSession::new(Arc::clone(&peer)));
        Self {
            peer,
            last_was_snapshot: AtomicBool::new(false),
            stealth_injected: AtomicBool::new(false),
            session,
        }
    }

    /// Get a clone of the shared session state for the agent loop.
    pub fn session(&self) -> Arc<BrowserSession> {
        Arc::clone(&self.session)
    }

    /// Call an individual Playwright MCP tool through the persistent peer.
    async fn call_mcp(&self, tool_name: &str, args: Value) -> Result<String> {
        self.peer.call_tool(tool_name, args).await
    }

    /// Normalize a ref value from model output.
    ///
    /// Models often send malformed refs:
    /// - `"ref=e42"` → `"e42"`
    /// - `"42"` → `"e42"`
    /// - `"e42"` → `"e42"` (already correct)
    fn normalize_ref(args: &Value) -> Result<String> {
        let raw = args
            .get("ref")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'ref' parameter is required for this action"))?;

        let clean = raw
            .trim()
            .trim_start_matches("ref=")
            .trim_start_matches("ref:");

        if clean.starts_with('e') {
            Ok(clean.to_string())
        } else if clean.chars().all(|c| c.is_ascii_digit()) {
            Ok(format!("e{clean}"))
        } else {
            Ok(clean.to_string())
        }
    }

    /// Inject anti-detection (stealth) scripts into the browser context.
    ///
    /// Uses Playwright's `addInitScript` to patch browser properties that
    /// anti-bot systems check (navigator.webdriver, plugins, chrome runtime).
    /// These patches are equivalent to `playwright-extra-plugin-stealth` but
    /// injected through the MCP `browser_run_code` tool.
    ///
    /// Must be called BEFORE the first navigation — `addInitScript` only
    /// applies to subsequent page loads.
    async fn inject_stealth(&self) {
        if self.stealth_injected.load(Ordering::Relaxed) {
            return;
        }

        // Stealth patches — mirrors playwright-extra-plugin-stealth essentials:
        // 1. navigator.webdriver = false (primary bot detection flag)
        // 2. window.chrome runtime (Chrome identity check)
        // 3. navigator.plugins (0 plugins = automation)
        // 4. navigator.permissions.query (notification permission leak)
        // 5. WebGL vendor/renderer (headless detection)
        let stealth_code = r#"async (page) => {
            await page.addInitScript(() => {
                Object.defineProperty(navigator, 'webdriver', {
                    get: () => false
                });

                if (!window.chrome) {
                    window.chrome = { runtime: {}, loadTimes: function(){}, csi: function(){} };
                }

                Object.defineProperty(navigator, 'plugins', {
                    get: () => {
                        const plugins = [
                            { name: 'Chrome PDF Plugin', filename: 'internal-pdf-viewer' },
                            { name: 'Chrome PDF Viewer', filename: 'mhjfbmdgcfjbbpaeojofohoefgiehjai' },
                            { name: 'Native Client', filename: 'internal-nacl-plugin' }
                        ];
                        plugins.length = 3;
                        return plugins;
                    }
                });

                const origQuery = navigator.permissions.query.bind(navigator.permissions);
                navigator.permissions.query = (params) => {
                    if (params.name === 'notifications') {
                        return Promise.resolve({ state: Notification.permission });
                    }
                    return origQuery(params);
                };
            });
        }"#;

        match self
            .call_mcp("browser_run_code", json!({"code": stealth_code}))
            .await
        {
            Ok(_) => {
                tracing::info!("Browser stealth scripts injected (anti-bot detection)");
                self.stealth_injected.store(true, Ordering::Relaxed);
            }
            Err(e) => {
                tracing::warn!("Failed to inject stealth scripts: {e} — bot detection may trigger");
            }
        }
    }

    /// Execute the `navigate` action.
    ///
    /// After navigation, automatically waits for the page to stabilize and
    /// returns a compacted snapshot. This prevents the model from seeing an
    /// empty/loading page and reflexively reloading.
    async fn action_navigate(&self, args: &Value) -> Result<ToolResult> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'url' parameter is required for navigate"))?;

        // Inject stealth scripts before the first navigation so addInitScript
        // runs BEFORE any page JavaScript (anti-bot detection countermeasure).
        self.inject_stealth().await;

        if let Err(e) = self.call_mcp("browser_navigate", json!({"url": url})).await {
            return Ok(ToolResult::error(format!("Navigate failed: {e}")));
        }

        // Track session state
        self.session.note_action(Some(url)).await;

        // Wait for the page to stabilize, then auto-snapshot.
        let snapshot = self.wait_for_stable_snapshot().await;
        self.last_was_snapshot.store(true, Ordering::Relaxed);

        let mut result = format!("Navigated to {url}\n\n");
        result.push_str(&snapshot);
        Ok(ToolResult::success(result))
    }

    /// Wait for the page to have meaningful interactive content.
    ///
    /// Heavy SPAs (Trenitalia, Italo) load in phases: skeleton → hydration →
    /// API data. We retry with increasing delays and also check for stability
    /// (element count stopped growing = page finished loading).
    async fn wait_for_stable_snapshot(&self) -> String {
        const MIN_INTERACTIVE: usize = 5;
        const DELAYS_MS: [u64; 5] = [1500, 2000, 2500, 3000, 3000];

        // Initial delay for the page to start rendering + JS hydration
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        let mut prev_count: usize = 0;

        for (attempt, delay) in DELAYS_MS.iter().enumerate() {
            match self.call_mcp("browser_snapshot", json!({})).await {
                Ok(output) => {
                    let compacted = compact_browser_snapshot(&output);
                    let interactive_count = compacted.matches("[ref=").count();

                    // Page is ready when:
                    // 1. Enough interactive elements AND count stabilized (not still growing), OR
                    // 2. Last attempt — return whatever we have
                    let is_stable =
                        interactive_count >= MIN_INTERACTIVE && interactive_count == prev_count;
                    let is_last = attempt == DELAYS_MS.len() - 1;

                    if is_stable || is_last {
                        tracing::debug!(
                            "Page stable after attempt {} ({} interactive elements, stable={})",
                            attempt + 1,
                            interactive_count,
                            is_stable
                        );
                        return compacted;
                    }

                    tracing::debug!(
                        "Page loading (attempt {}, {} → {} elements), waiting {}ms",
                        attempt + 1,
                        prev_count,
                        interactive_count,
                        delay
                    );
                    prev_count = interactive_count;
                    tokio::time::sleep(std::time::Duration::from_millis(*delay)).await;
                }
                Err(e) => {
                    tracing::warn!("Snapshot attempt {} failed: {e}", attempt + 1);
                    tokio::time::sleep(std::time::Duration::from_millis(*delay)).await;
                }
            }
        }

        // All retries exhausted — return error message
        "Page may still be loading. Call snapshot() to check again.".to_string()
    }

    /// Execute the `snapshot` action with compaction.
    async fn action_snapshot(&self) -> Result<ToolResult> {
        // Consecutive snapshot guard
        if self.last_was_snapshot.load(Ordering::Relaxed) {
            return Ok(ToolResult::error(
                "Page has not changed since last snapshot. \
                 Use the refs from the previous snapshot result. \
                 Do NOT call snapshot again — perform an action first (click, type, navigate)."
                    .to_string(),
            ));
        }

        match self.call_mcp("browser_snapshot", json!({})).await {
            Ok(output) => {
                self.last_was_snapshot.store(true, Ordering::Relaxed);
                Ok(ToolResult::success(compact_browser_snapshot(&output)))
            }
            Err(e) => Ok(ToolResult::error(format!("Snapshot failed: {e}"))),
        }
    }

    /// Execute the `click` action.
    ///
    /// After clicking, auto-snapshots to give the model fresh refs.
    /// This prevents the stale-ref problem where DOM changes after click
    /// (e.g. autocomplete dropdown closing) invalidate previously seen refs.
    async fn action_click(&self, args: &Value) -> Result<ToolResult> {
        let ref_val = Self::normalize_ref(args)?;
        let base_output = match self
            .call_mcp("browser_click", json!({"ref": ref_val}))
            .await
        {
            Ok(output) => compact_action_short(&output, "Clicked."),
            Err(e) => return Ok(ToolResult::error(format!("Click failed: {e}"))),
        };

        // Brief wait for DOM to settle, then auto-snapshot for fresh refs
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        match self.call_mcp("browser_snapshot", json!({})).await {
            Ok(snap_output) => {
                let compact = compact_browser_snapshot(&snap_output);
                self.last_was_snapshot.store(true, Ordering::Relaxed);
                Ok(ToolResult::success(format!("{base_output}\n\n{compact}")))
            }
            Err(_) => {
                // Snapshot failed (maybe navigation in progress) — return click result only
                Ok(ToolResult::success(base_output))
            }
        }
    }

    /// Execute the `type` action with auto-snapshot for autocomplete detection.
    async fn action_type(&self, args: &Value) -> Result<ToolResult> {
        let ref_val = Self::normalize_ref(args)?;
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'text' parameter is required for type"))?;

        let type_result = self
            .call_mcp(
                "browser_type",
                json!({"ref": ref_val, "text": text, "slowly": true}),
            )
            .await;

        let base_output = match type_result {
            Ok(output) => compact_action_short(&output, &format!("Typed \"{text}\".")),
            Err(e) => return Ok(ToolResult::error(format!("Type failed: {e}"))),
        };

        // Auto-snapshot to detect autocomplete suggestions
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if let Ok(snap_output) = self.call_mcp("browser_snapshot", json!({})).await {
            if let Some(suggestions) = extract_autocomplete_suggestions(&snap_output) {
                tracing::info!("Auto-snapshot after type: autocomplete suggestions found");
                // Mark as snapshot since we just did one
                self.last_was_snapshot.store(true, Ordering::Relaxed);
                return Ok(ToolResult::success(format!("{base_output}{suggestions}")));
            }
        }

        Ok(ToolResult::success(base_output))
    }

    /// Execute the `fill` action (clear + type, no autocomplete).
    async fn action_fill(&self, args: &Value) -> Result<ToolResult> {
        let ref_val = Self::normalize_ref(args)?;
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'text' parameter is required for fill"))?;

        // Select all existing text first, then type over it
        let _ = self
            .call_mcp("browser_click", json!({"ref": ref_val}))
            .await;
        let _ = self
            .call_mcp("browser_press_key", json!({"key": "Control+a"}))
            .await;

        match self
            .call_mcp("browser_type", json!({"ref": ref_val, "text": text}))
            .await
        {
            Ok(output) => Ok(ToolResult::success(compact_action_short(
                &output,
                &format!("Filled with \"{text}\"."),
            ))),
            Err(e) => Ok(ToolResult::error(format!("Fill failed: {e}"))),
        }
    }

    /// Execute the `select_option` action.
    async fn action_select_option(&self, args: &Value) -> Result<ToolResult> {
        let ref_val = Self::normalize_ref(args)?;
        let value = args
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'value' parameter is required for select_option"))?;

        match self
            .call_mcp(
                "browser_select_option",
                json!({"ref": ref_val, "values": [value]}),
            )
            .await
        {
            Ok(output) => Ok(ToolResult::success(compact_action_short(
                &output,
                &format!("Selected \"{value}\"."),
            ))),
            Err(e) => Ok(ToolResult::error(format!("Select failed: {e}"))),
        }
    }

    /// Execute the `press_key` action.
    async fn action_press_key(&self, args: &Value) -> Result<ToolResult> {
        let key = args.get("text").and_then(|v| v.as_str()).ok_or_else(|| {
            anyhow::anyhow!("'text' parameter is required for press_key (e.g. \"Enter\", \"Tab\")")
        })?;

        match self
            .call_mcp("browser_press_key", json!({"key": key}))
            .await
        {
            Ok(output) => Ok(ToolResult::success(compact_action_short(
                &output,
                &format!("Pressed {key}."),
            ))),
            Err(e) => Ok(ToolResult::error(format!("Press key failed: {e}"))),
        }
    }

    /// Execute the `hover` action.
    async fn action_hover(&self, args: &Value) -> Result<ToolResult> {
        let ref_val = Self::normalize_ref(args)?;
        match self
            .call_mcp("browser_hover", json!({"ref": ref_val}))
            .await
        {
            Ok(output) => Ok(ToolResult::success(compact_action_short(
                &output, "Hovered.",
            ))),
            Err(e) => Ok(ToolResult::error(format!("Hover failed: {e}"))),
        }
    }

    /// Execute the `scroll` action.
    async fn action_scroll(&self, args: &Value) -> Result<ToolResult> {
        let direction = args
            .get("direction")
            .and_then(|v| v.as_str())
            .unwrap_or("down");

        // Default scroll at viewport center; if ref provided, use element
        let mut params = json!({
            "coordinate": [640, 400],
            "scroll_direction": direction
        });
        // If ref is provided, scroll inside that element
        if let Ok(ref_val) = Self::normalize_ref(args) {
            // Playwright MCP scroll doesn't take ref directly — it uses coordinates.
            // For now, scroll at viewport center. A future improvement could
            // get element coordinates from the snapshot.
            let _ = ref_val; // acknowledge but don't use yet
            params = json!({
                "coordinate": [640, 400],
                "scroll_direction": direction
            });
        }

        match self.call_mcp("browser_scroll", params).await {
            Ok(output) => Ok(ToolResult::success(compact_action_short(
                &output,
                &format!("Scrolled {direction}."),
            ))),
            Err(e) => Ok(ToolResult::error(format!("Scroll failed: {e}"))),
        }
    }

    /// Execute the `drag` action.
    async fn action_drag(&self, args: &Value) -> Result<ToolResult> {
        let start_ref = Self::normalize_ref(args)?;
        let end_ref = args
            .get("end_ref")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'end_ref' parameter is required for drag"))?;

        match self
            .call_mcp(
                "browser_drag",
                json!({
                    "startRef": start_ref,
                    "endRef": end_ref,
                    "startElement": "drag source",
                    "endElement": "drag target"
                }),
            )
            .await
        {
            Ok(output) => Ok(ToolResult::success(compact_action_short(
                &output, "Dragged.",
            ))),
            Err(e) => Ok(ToolResult::error(format!("Drag failed: {e}"))),
        }
    }

    /// Execute tab management actions.
    async fn action_tabs(&self, action: &str, args: &Value) -> Result<ToolResult> {
        let mcp_action = match action {
            "tab_list" => "list",
            "tab_new" => "new",
            "tab_select" => "select",
            "tab_close" => "close",
            _ => return Ok(ToolResult::error(format!("Unknown tab action: {action}"))),
        };

        let mut params = json!({"action": mcp_action});
        if let Some(idx) = args.get("index").and_then(|v| v.as_i64()) {
            params["index"] = json!(idx);
        }

        match self.call_mcp("browser_tabs", params).await {
            Ok(output) => Ok(ToolResult::success(output)),
            Err(e) => Ok(ToolResult::error(format!("Tab {action} failed: {e}"))),
        }
    }

    /// Execute the `evaluate` action (run JavaScript).
    ///
    /// Blocks DOM-manipulating patterns (click, focus, scrollTo, remove,
    /// innerHTML, etc.) — these break SPA frameworks. The model should use
    /// click/type/scroll actions instead.
    async fn action_evaluate(&self, args: &Value) -> Result<ToolResult> {
        let expression = args
            .get("expression")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'expression' parameter is required for evaluate"))?;

        // Block DOM manipulation — models use it as a crutch and it breaks SPAs
        let expr_lower = expression.to_lowercase();
        let blocked_patterns = [
            ".click()",
            ".focus()",
            ".blur()",
            ".remove()",
            ".innerhtml",
            ".textcontent =",
            ".innertext =",
            "scrollto(",
            "scrollby(",
            "setattribute(",
            "removeattribute(",
            "classlist.",
            "style.",
            "dispatchevent(",
            "appendchild(",
            "removechild(",
            "replacechild(",
        ];
        if blocked_patterns.iter().any(|p| expr_lower.contains(p)) {
            return Ok(ToolResult::error(
                "evaluate() is for READING page state only. \
                 Do NOT use it to click, focus, scroll, or modify the DOM — \
                 use the dedicated actions instead: click(ref), type(ref, text), scroll(direction)."
                    .to_string(),
            ));
        }

        match self
            .call_mcp("browser_evaluate", json!({"function": expression}))
            .await
        {
            Ok(output) => {
                let truncated = if output.len() > 2_000 {
                    let mut s = output;
                    truncate_utf8(&mut s, 2_000);
                    s.push_str("...[truncated]");
                    s
                } else {
                    output
                };
                Ok(ToolResult::success(truncated))
            }
            Err(e) => Ok(ToolResult::error(format!("Evaluate failed: {e}"))),
        }
    }

    /// Execute the `wait` action.
    async fn action_wait(&self, args: &Value) -> Result<ToolResult> {
        let seconds = args
            .get("seconds")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0)
            .min(30.0); // cap at 30 seconds

        tokio::time::sleep(std::time::Duration::from_secs_f64(seconds)).await;
        Ok(ToolResult::success(format!("Waited {seconds}s.")))
    }

    /// Execute the `close` action.
    async fn action_close(&self) -> Result<ToolResult> {
        self.session.clear().await;
        match self.call_mcp("browser_close", json!({})).await {
            Ok(_) => Ok(ToolResult::success("Browser closed.".to_string())),
            Err(e) => Ok(ToolResult::error(format!("Close failed: {e}"))),
        }
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        "Browser automation. Actions:\n\
         - navigate(url): Go to URL (auto-returns page snapshot)\n\
         - snapshot(): Get page accessibility tree with interactive elements [ref=eN]\n\
         - click(ref): Click element (auto-returns updated snapshot)\n\
         - type(ref, text): Type text into field (triggers autocomplete check)\n\
         - fill(ref, text): Clear field + type (for overwriting)\n\
         - select_option(ref, value): Select dropdown option\n\
         - press_key(text): Press key (e.g. \"Enter\", \"Tab\")\n\
         - hover(ref): Hover over element\n\
         - scroll(direction, ref?): Scroll page or element up/down\n\
         - drag(ref, end_ref): Drag from ref to end_ref\n\
         - tab_list/tab_new/tab_select(index)/tab_close(index): Tab management\n\
         - evaluate(expression): Read page state via JS (READ-ONLY, no DOM changes)\n\
         - wait(seconds): Wait N seconds\n\
         - close(): Close browser\n\n\
         RULES:\n\
         1. navigate() already returns the page — do NOT call snapshot() right after\n\
         2. Use refs from the LATEST snapshot only (e.g. ref=\"e42\")\n\
         3. click() already returns a snapshot — no need to call snapshot() after\n\
         4. For autocomplete fields: type partial text → look at suggestions → click match\n\
         5. If page seems empty/broken, call snapshot() BEFORE reloading — it may still be loading"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "navigate", "snapshot", "click", "type", "fill",
                        "select_option", "press_key", "hover", "scroll",
                        "drag", "tab_list", "tab_new", "tab_select",
                        "tab_close", "evaluate", "close", "wait"
                    ],
                    "description": "Browser action to perform"
                },
                "url": {
                    "type": "string",
                    "description": "URL for navigate"
                },
                "ref": {
                    "type": "string",
                    "description": "Element ref from snapshot (e.g. \"e42\")"
                },
                "text": {
                    "type": "string",
                    "description": "Text for type/fill, or key for press_key (e.g. \"Enter\")"
                },
                "value": {
                    "type": "string",
                    "description": "Value for select_option"
                },
                "direction": {
                    "type": "string",
                    "enum": ["up", "down"],
                    "description": "Scroll direction"
                },
                "index": {
                    "type": "integer",
                    "description": "Tab index for tab_select/tab_close"
                },
                "expression": {
                    "type": "string",
                    "description": "JavaScript for evaluate"
                },
                "end_ref": {
                    "type": "string",
                    "description": "Target ref for drag"
                },
                "seconds": {
                    "type": "number",
                    "description": "Seconds for wait (max 30)"
                }
            }
        })
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");

        // Reset consecutive snapshot flag for non-snapshot actions
        if action != "snapshot" {
            self.last_was_snapshot.store(false, Ordering::Relaxed);
        }

        tracing::debug!(action = %action, "Browser tool action");

        let result = match action {
            "navigate" => self.action_navigate(&args).await?,
            "snapshot" => self.action_snapshot().await?,
            "click" => self.action_click(&args).await?,
            "type" => self.action_type(&args).await?,
            "fill" => self.action_fill(&args).await?,
            "select_option" => self.action_select_option(&args).await?,
            "press_key" => self.action_press_key(&args).await?,
            "hover" => self.action_hover(&args).await?,
            "scroll" => self.action_scroll(&args).await?,
            "drag" => self.action_drag(&args).await?,
            "tab_list" | "tab_new" | "tab_select" | "tab_close" => {
                self.action_tabs(action, &args).await?
            }
            "evaluate" => self.action_evaluate(&args).await?,
            "wait" => self.action_wait(&args).await?,
            "close" => self.action_close().await?,
            "" => ToolResult::error(
                "Missing 'action' parameter. Available actions: \
                 navigate, snapshot, click, type, fill, select_option, \
                 press_key, hover, scroll, drag, tab_list, tab_new, \
                 tab_select, tab_close, evaluate, wait, close"
                    .to_string(),
            ),
            unknown => ToolResult::error(format!(
                "Unknown action \"{unknown}\". Available actions: \
                 navigate, snapshot, click, type, fill, select_option, \
                 press_key, hover, scroll, drag, tab_list, tab_new, \
                 tab_select, tab_close, evaluate, wait, close"
            )),
        };

        // Track session timestamp for all non-close actions (navigate tracks URL separately)
        if action != "close" && !action.is_empty() {
            self.session.note_action(None).await;
        }

        Ok(result)
    }
}

// ============================================================================
// Snapshot compaction helpers (moved from agent_loop.rs)
// ============================================================================

/// Compact a `browser_snapshot` output for the model context window.
///
/// Uses agent-browser's approach: keep lines with `[ref=]`, content roles
/// (heading, cell, listitem), or value text (`": "`), plus all ancestor lines
/// for tree hierarchy. This preserves context (a button inside a dialog,
/// results inside a list) while filtering out noise.
pub fn compact_browser_snapshot(output: &str) -> String {
    let max_chars: usize = std::env::var("HOMUN_BROWSER_MAX_OUTPUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50_000);

    let (header_lines, tree_lines) = split_browser_output(output);

    let mut compact = String::new();

    // Header (URL, title)
    for line in &header_lines {
        compact.push_str(line);
        compact.push('\n');
    }

    if tree_lines.is_empty() {
        return compact;
    }

    // Compact tree: keep refs + content roles + value text + ancestors
    let kept_tree = compact_tree(&tree_lines);

    // Summary
    let ref_count = kept_tree.matches("[ref=").count();
    compact.push_str(&format!(
        "({ref_count} interactive elements) Use ref=\"eN\" exactly as shown.\n\n",
    ));

    compact.push_str(&kept_tree);

    // Form planning instruction when form fields are detected
    if has_form_fields(&kept_tree) {
        compact.push_str(
            "\n\n** FORM PLAN — do this before filling **\n\
             For each field, write: field → value from user's request.\n\
             IGNORE pre-filled / default values.\n\
             Convert: \"mattina\"→06:00-12:00, \"pomeriggio\"→12:00-18:00, \
             \"sera\"→18:00-23:00, \"domani\"→tomorrow's date.\n\
             Autocomplete fields (combobox): type partial text → snapshot → click match.\n\
             If a required value is missing, ask the user.\n",
        );
    }

    // Hard truncation (UTF-8 safe)
    if compact.len() > max_chars {
        truncate_utf8(&mut compact, max_chars);
        compact.push_str("\n...[snapshot truncated]");
    }

    compact
}

/// Compact a simple browser action output — keep just the confirmation.
fn compact_action_short(output: &str, prefix: &str) -> String {
    let (header_lines, _) = split_browser_output(output);
    if header_lines.is_empty() {
        return prefix.to_string();
    }
    let mut s = String::from(prefix);
    s.push(' ');
    for line in &header_lines {
        s.push_str(line);
        s.push(' ');
    }
    s.trim().to_string()
}

/// Extract autocomplete/dropdown suggestions from a snapshot.
///
/// Looks for `option "..." [ref=eN]` lines in the accessibility tree.
pub fn extract_autocomplete_suggestions(snapshot_output: &str) -> Option<String> {
    let mut suggestions = Vec::new();
    for line in snapshot_output.lines() {
        let trimmed = line.trim().trim_start_matches("- ");
        if trimmed.starts_with("option ") && trimmed.contains("[ref=") {
            suggestions.push(trimmed.to_string());
        }
    }
    if suggestions.is_empty() {
        return None;
    }
    let mut result = format!(
        "\n\nAutocomplete dropdown appeared with {} suggestion(s):\n",
        suggestions.len()
    );
    for s in suggestions.iter().take(10) {
        result.push_str("  - ");
        result.push_str(s);
        result.push('\n');
    }
    result.push_str(
        "→ Click the matching option to select it: browser({action: \"click\", ref: \"eN\"})",
    );
    Some(result)
}

/// Split browser tool output into header lines and accessibility tree lines.
fn split_browser_output(output: &str) -> (Vec<&str>, Vec<&str>) {
    let mut header_lines: Vec<&str> = Vec::new();
    let mut tree_lines: Vec<&str> = Vec::new();
    let mut in_tree = false;

    for raw_line in output.lines() {
        let line = raw_line.trim_end();
        if line.starts_with("[image:") {
            continue;
        }
        if !in_tree && line.trim_start().starts_with("- ") {
            in_tree = true;
        }
        if in_tree {
            tree_lines.push(line);
        } else {
            header_lines.push(line);
        }
    }

    (header_lines, tree_lines)
}

/// Compact tree by keeping meaningful lines and their ancestors.
///
/// Inspired by agent-browser.dev's `compact_tree`:
/// - Lines with `[ref=]` → interactive elements (clickable, fillable)
/// - Lines matching content roles (heading, cell, listitem) → result data
/// - Lines with value text (`": "` after attributes) → field values, displayed data
/// - For every kept line, all ancestor lines (by indentation) are preserved
///
/// This preserves the tree hierarchy so the model sees context: a button inside
/// a dialog, results inside a list, a form inside a section.
fn compact_tree(lines: &[&str]) -> String {
    if lines.is_empty() {
        return String::new();
    }

    let mut keep = vec![false; lines.len()];

    for (i, line) in lines.iter().enumerate() {
        if should_keep_line(line) {
            keep[i] = true;
            // Mark ancestor lines (walk backwards, find smaller indents)
            let my_indent = count_indent(line);
            for j in (0..i).rev() {
                let ancestor_indent = count_indent(lines[j]);
                if ancestor_indent < my_indent {
                    keep[j] = true;
                    if ancestor_indent == 0 {
                        break;
                    }
                }
            }
        }
    }

    lines
        .iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, line)| *line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Decide whether a snapshot line carries meaningful information.
///
/// Three categories of meaningful lines:
/// 1. Interactive elements with refs (can be clicked/filled)
/// 2. Content roles that carry data (headings, table cells, list items)
/// 3. Value text (field values, displayed data like prices/times)
fn should_keep_line(line: &str) -> bool {
    // 1. Interactive elements with refs — always keep
    if line.contains("[ref=") {
        return true;
    }

    let trimmed = line.trim_start().trim_start_matches("- ");

    // 2. Content roles with quoted text (data display: titles, prices, names)
    //    Matches agent-browser's CONTENT_ROLES: heading, cell, gridcell,
    //    columnheader, rowheader, listitem, article
    if (trimmed.starts_with("heading ")
        || trimmed.starts_with("cell ")
        || trimmed.starts_with("gridcell ")
        || trimmed.starts_with("columnheader ")
        || trimmed.starts_with("rowheader ")
        || trimmed.starts_with("listitem "))
        && trimmed.contains('"')
    {
        return true;
    }

    // 3. Value text: ": " after closing bracket indicates a field/element value
    //    e.g., `textbox "Email" [ref=e5]: john@example.com`
    if let Some(bracket_pos) = line.rfind(']') {
        if line[bracket_pos..].contains(": ") {
            return true;
        }
    }

    false
}

/// Count indentation level in 2-space units.
fn count_indent(line: &str) -> usize {
    let trimmed = line.trim_start();
    (line.len() - trimmed.len()) / 2
}

/// Check if the compact tree contains form fields (combobox, textbox, etc.).
fn has_form_fields(tree: &str) -> bool {
    tree.lines().any(|line| {
        let t = line.trim_start().trim_start_matches("- ");
        t.starts_with("combobox ")
            || t.starts_with("textbox ")
            || t.starts_with("checkbox ")
            || t.starts_with("radio ")
            || t.starts_with("searchbox ")
            || t.starts_with("slider ")
            || t.starts_with("spinbutton ")
    })
}

/// Truncate a string to at most `max_bytes`, snapping to a char boundary.
fn truncate_utf8(s: &mut String, max_bytes: usize) {
    if s.len() <= max_bytes {
        return;
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) && end > 0 {
        end -= 1;
    }
    s.truncate(end);
}

/// Extract the `action` field from browser tool arguments.
///
/// Used by `agent_loop.rs` to determine what browser action was performed
/// without knowing the internal structure of BrowserTool.
pub fn browser_action_from_args(args: &Value) -> Option<&str> {
    args.get("action").and_then(|v| v.as_str())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_ref_correct() {
        let args = json!({"ref": "e42"});
        assert_eq!(BrowserTool::normalize_ref(&args).unwrap(), "e42");
    }

    #[test]
    fn test_normalize_ref_numeric() {
        let args = json!({"ref": "42"});
        assert_eq!(BrowserTool::normalize_ref(&args).unwrap(), "e42");
    }

    #[test]
    fn test_normalize_ref_with_prefix() {
        let args = json!({"ref": "ref=e42"});
        assert_eq!(BrowserTool::normalize_ref(&args).unwrap(), "e42");
    }

    #[test]
    fn test_normalize_ref_missing() {
        let args = json!({"action": "click"});
        assert!(BrowserTool::normalize_ref(&args).is_err());
    }

    #[test]
    fn test_compact_browser_snapshot() {
        // Use single-line string with explicit \n to preserve indentation
        let output = "Page URL: https://example.com\nPage Title: Example\n- navigation\n  - heading \"Welcome\"\n  - textbox \"Email\" [ref=e5]\n  - textbox \"Password\" [ref=e6]\n  - button \"Login\" [ref=e7]\n- paragraph \"Footer text\"\n";
        let compact = compact_browser_snapshot(output);
        // Interactive elements preserved with refs
        assert!(compact.contains("Email"));
        assert!(compact.contains("[ref=e5]"));
        assert!(compact.contains("3 interactive"));
        // Heading preserved as content role
        assert!(compact.contains("heading \"Welcome\""));
        // Ancestor preserved (navigation contains the form)
        assert!(compact.contains("navigation"));
        // Non-interactive paragraph stripped
        assert!(!compact.contains("Footer text"));
        // Form planning instruction present
        assert!(compact.contains("FORM PLAN"));
    }

    #[test]
    fn test_compact_preserves_tree_hierarchy() {
        let output = "- main\n  - section\n    - heading \"Results\"\n    - list\n      - listitem \"Train ICE 1234\"\n      - button \"Buy\" [ref=e10]\n  - footer\n    - paragraph \"Copyright\"\n";
        let compact = compact_browser_snapshot(output);
        // Button and its ancestors preserved
        assert!(compact.contains("button \"Buy\" [ref=e10]"));
        assert!(compact.contains("list"));
        assert!(compact.contains("section"));
        assert!(compact.contains("main"));
        // Content role preserved
        assert!(compact.contains("heading \"Results\""));
        assert!(compact.contains("listitem \"Train ICE 1234\""));
        // Footer without interactive content stripped
        assert!(!compact.contains("Copyright"));
    }

    #[test]
    fn test_compact_preserves_value_text() {
        let output = "- form\n  - textbox \"City\" [ref=e1]: Napoli\n  - button \"Go\" [ref=e2]\n";
        let compact = compact_browser_snapshot(output);
        // Value text after ] is preserved
        assert!(compact.contains(": Napoli"));
        assert!(compact.contains("[ref=e1]"));
    }

    #[test]
    fn test_compact_preserves_table_cells() {
        let output = "- table\n  - row\n    - columnheader \"Time\"\n    - columnheader \"Price\"\n  - row\n    - cell \"06:15\"\n    - cell \"€49.90\"\n    - button \"Buy\" [ref=e20]\n";
        let compact = compact_browser_snapshot(output);
        assert!(compact.contains("cell \"06:15\""));
        assert!(compact.contains("cell \"€49.90\""));
        assert!(compact.contains("columnheader \"Time\""));
        assert!(compact.contains("columnheader \"Price\""));
        assert!(compact.contains("button \"Buy\" [ref=e20]"));
    }

    #[test]
    fn test_extract_autocomplete_suggestions() {
        let output = "- combobox \"From\" [ref=e10]\n  - listbox\n    - option \"Roma Termini\" [ref=e11]\n    - option \"Roma Tiburtina\" [ref=e12]\n";
        let suggestions = extract_autocomplete_suggestions(output).unwrap();
        assert!(suggestions.contains("Roma Termini"));
        assert!(suggestions.contains("2 suggestion(s)"));
        assert!(suggestions.contains("click"));
    }

    #[test]
    fn test_no_autocomplete_suggestions() {
        let output = "- textbox \"Search\" [ref=e1]\n- button \"Go\" [ref=e2]\n";
        assert!(extract_autocomplete_suggestions(output).is_none());
    }

    #[test]
    fn test_should_keep_line() {
        // Interactive elements with refs
        assert!(should_keep_line("  - textbox \"Email\" [ref=e5]"));
        assert!(should_keep_line("  - button \"Submit\" [ref=e7]"));
        // Content roles with text
        assert!(should_keep_line("  - heading \"Results\""));
        assert!(should_keep_line("  - cell \"€49.90\""));
        assert!(should_keep_line("  - listitem \"Train ICE 1234\""));
        // Value text after ]
        assert!(should_keep_line("  - textbox \"City\" [ref=e1]: Roma"));
        // Non-meaningful lines
        assert!(!should_keep_line("  - paragraph \"text\""));
        assert!(!should_keep_line("  - generic"));
        assert!(!should_keep_line("  - navigation"));
    }

    #[test]
    fn test_count_indent() {
        assert_eq!(count_indent("- heading"), 0);
        assert_eq!(count_indent("  - link"), 1);
        assert_eq!(count_indent("    - text"), 2);
    }

    #[test]
    fn test_browser_action_from_args() {
        assert_eq!(
            browser_action_from_args(&json!({"action": "click", "ref": "e42"})),
            Some("click")
        );
        assert_eq!(browser_action_from_args(&json!({})), None);
    }
}
