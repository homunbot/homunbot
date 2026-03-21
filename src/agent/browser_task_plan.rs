//! Browser task plan — state tracking for browser automation sessions.
//!
//! Initialized from `CognitionResult` via `from_cognition()`.
//! Tracks visited sources, extraction progress, autocomplete state,
//! and enforces browser-specific safety vetoes at runtime.

use crate::agent::cognition::CognitionResult;
use crate::provider::ChatMessage;

/// High-level classification of what the browser task involves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserTaskClass {
    StaticLookup,
    InteractiveWeb,
    FormBooking,
    MultiSourceCompare,
}

/// Browser routing metadata derived from cognition understanding.
#[derive(Debug, Clone)]
pub struct BrowserRoutingDecision {
    task_class: BrowserTaskClass,
    browser_required: bool,
    required_sources: Vec<String>,
    reason: String,
}

/// Runtime state for an active browser automation session.
#[derive(Debug, Clone, Default)]
pub struct BrowserTaskPlanState {
    objective: String,
    routing: BrowserRoutingDecision,
    compare_mode: bool,
    required_sources: Vec<String>,
    visited_sources: Vec<String>,
    extracted_sources: Vec<String>,
    current_source: Option<String>,
    result_pages_seen: u8,
    pending_selection: bool,
    requires_fresh_snapshot: bool,
    /// Whether the agent has entered a booking site via Google search.
    used_google_entry: bool,
}

impl BrowserTaskPlanState {
    /// Initialize from a CognitionResult — the sole construction path.
    ///
    /// The cognition phase determines whether the browser is needed via
    /// semantic understanding rather than keyword matching.
    pub fn from_cognition(result: &CognitionResult, user_prompt: &str) -> Self {
        let browser_needed = result.tools.iter().any(|t| t.name == "browser");
        let compare_mode = result.constraints.iter().any(|c| {
            let lower = c.to_lowercase();
            lower.contains("compar") || lower.contains("confront")
        });

        let objective = crate::utils::text::truncate_str(user_prompt.trim(), 220, "");

        // Build a BrowserRoutingDecision from cognition understanding
        let task_class = if compare_mode && browser_needed {
            BrowserTaskClass::MultiSourceCompare
        } else if browser_needed {
            let understanding_lower = result.understanding.to_lowercase();
            if understanding_lower.contains("book")
                || understanding_lower.contains("prenot")
                || understanding_lower.contains("ticket")
                || understanding_lower.contains("bigliett")
            {
                BrowserTaskClass::FormBooking
            } else {
                BrowserTaskClass::InteractiveWeb
            }
        } else {
            BrowserTaskClass::StaticLookup
        };

        let routing = BrowserRoutingDecision {
            task_class,
            browser_required: browser_needed,
            required_sources: Vec::new(),
            reason: result.understanding.clone(),
        };

        Self {
            objective,
            compare_mode,
            required_sources: Vec::new(),
            routing,
            visited_sources: Vec::new(),
            extracted_sources: Vec::new(),
            current_source: None,
            result_pages_seen: 0,
            pending_selection: false,
            requires_fresh_snapshot: true,
            used_google_entry: false,
        }
    }

    /// Whether a results page has been seen during this browser task.
    pub fn has_seen_results(&self) -> bool {
        self.result_pages_seen > 0
    }

    /// Update internal state after a browser tool result.
    pub fn note_browser_result(&mut self, action: Option<&str>, output: &str) {
        let lower = output.to_ascii_lowercase();
        let mut source = None;
        for line in output.lines() {
            if let Some(url) = line.trim().strip_prefix("Page URL: ") {
                source = extract_source_name(url);
                break;
            }
        }

        if output.contains("Visible result hints:") || output.contains("RESULTS PAGE") {
            self.result_pages_seen = self.result_pages_seen.saturating_add(1);
        }
        self.pending_selection = lower.contains("visible suggestions:")
            || lower.contains("autocomplete still open")
            || lower.contains("combobox-style field")
            || lower.contains("autocomplete dropdown appeared")
            || lower.contains("select a suggestion before moving");

        if let Some(source_name) = source.clone().filter(|source| source != "about") {
            self.current_source = Some(source_name.clone());
            push_unique(&mut self.visited_sources, source_name.clone());
            if output.contains("Extracted results:") {
                push_unique(&mut self.extracted_sources, source_name);
                self.pending_selection = false;
            }
        }

        match action.unwrap_or_default() {
            "navigate" | "navigate_back" => {
                self.requires_fresh_snapshot = true;
                if !self.used_google_entry && lower.contains("google.com") {
                    self.used_google_entry = true;
                }
            }
            "snapshot" | "take_screenshot" => {
                self.requires_fresh_snapshot = false;
            }
            "click" | "type" | "select" | "choose_suggestion" | "press" | "submit_form"
            | "select_option" | "fill_form" | "press_key" | "drag" => {
                if lower.contains("latest browser step failed") || lower.contains("tool error") {
                    self.requires_fresh_snapshot = true;
                }
                if !self.used_google_entry
                    && action.unwrap_or_default() == "click"
                    && self.current_source.as_deref() == Some("google")
                {
                    self.used_google_entry = true;
                }
            }
            _ => {}
        }
    }

    /// Merge browser state into an execution plan snapshot.
    pub fn merged_snapshot(
        &self,
        mut snapshot: crate::agent::execution_plan::ExecutionPlanSnapshot,
    ) -> crate::agent::execution_plan::ExecutionPlanSnapshot {
        snapshot.required_sources = self.required_sources.clone();
        snapshot.completed_sources = self.extracted_sources.clone();
        snapshot.current_source = self.current_source.clone();
        snapshot
    }

    /// Veto check for MCP-style browser tools where the action comes from the tool name suffix.
    pub fn veto_browser_action_mcp(
        &self,
        action: Option<&str>,
        arguments: &serde_json::Value,
    ) -> Option<String> {
        self.veto_browser_action_inner(action.unwrap_or_default(), arguments)
    }

    /// Veto check for the unified browser tool (action in arguments).
    pub fn veto_browser_action(&self, arguments: &serde_json::Value) -> Option<String> {
        let action = arguments
            .get("action")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        self.veto_browser_action_inner(action, arguments)
    }

    fn veto_browser_action_inner(
        &self,
        action: &str,
        arguments: &serde_json::Value,
    ) -> Option<String> {
        // Require fresh snapshot before interacting after navigation
        if self.requires_fresh_snapshot
            && matches!(
                action,
                "click"
                    | "type"
                    | "select"
                    | "choose_suggestion"
                    | "submit_form"
                    | "extract"
                    | "select_option"
                    | "fill_form"
                    | "drag"
            )
        {
            return Some(
                "Browser planner veto: take a fresh snapshot before interacting with page refs or extracting results after navigation/page change."
                    .to_string(),
            );
        }

        // Google-first veto for FormBooking tasks
        if !self.used_google_entry
            && matches!(self.routing.task_class, BrowserTaskClass::FormBooking)
            && action == "navigate"
        {
            let target_url = arguments
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !target_url.contains("google.com") {
                let site_name =
                    extract_source_name(&target_url).unwrap_or_else(|| target_url.clone());
                return Some(format!(
                    "Browser planner veto: for booking sites, navigate via Google search first \
                     to establish a natural browsing session. Navigate to \
                     https://www.google.com/search?q={site_name} and click the organic result."
                ));
            }
        }

        // Pending autocomplete selection — block unrelated actions
        if self.pending_selection {
            let allowed = match action {
                "choose_suggestion" | "snapshot" | "wait" | "click" | "wait_for" => true,
                "press" | "press_key" => arguments
                    .get("key")
                    .and_then(|value| value.as_str())
                    .map(|key| matches!(key, "Enter" | "Tab" | "ArrowDown" | "ArrowUp"))
                    .unwrap_or(false),
                _ => false,
            };
            if !allowed {
                return Some(
                    "Browser planner veto: the current field still has an open autocomplete/combobox state. Confirm the suggestion first with choose_suggestion, an option click, or Enter/Tab before touching other fields or changing page."
                        .to_string(),
                );
            }
        }

        // Veto re-navigating to a site we already have open
        if action == "navigate" {
            if let Some(current_source) = &self.current_source {
                let target_url = arguments
                    .get("url")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if let Some(target_source) = extract_source_name(&target_url) {
                    if target_source == *current_source {
                        return Some(format!(
                            "Browser planner veto: you already have {} open. \
                             Use snapshot() to see the current page, or click links/buttons on it. \
                             Do NOT navigate() to the same site again — continue from where you are.",
                            current_source
                        ));
                    }
                }
            }
        }

        // Compare-mode vetoes: don't leave or close before extracting
        if self.compare_mode && !self.required_sources.is_empty() {
            if let Some(current_source) = &self.current_source {
                let current_done = self.extracted_sources.iter().any(|s| s == current_source);
                if !current_done && action == "close" {
                    return Some(format!(
                        "Browser planner veto: do not close the browser while {} is still incomplete. Extract the current source results first.",
                        current_source
                    ));
                }
                if !current_done && action == "navigate" {
                    let target_url = arguments
                        .get("url")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    if let Some(target_source) = extract_source_name(&target_url) {
                        if target_source != *current_source {
                            return Some(format!(
                                "Browser planner veto: do not leave {} yet. Extract the results from the current source before switching to {}.",
                                current_source, target_source
                            ));
                        }
                    }
                }
            }
        }

        None
    }

    /// Build a runtime message with browser task context for the LLM.
    pub fn runtime_message(&self, browser_available: bool) -> Option<ChatMessage> {
        if self.routing.browser_required && !browser_available {
            return Some(ChatMessage::user(&format!(
                "Note: this task involves {} but the browser is currently unavailable.",
                self.routing.reason
            )));
        }

        if self.objective.is_empty()
            || (!self.routing.browser_required
                && !self.compare_mode
                && self.result_pages_seen == 0
                && self.visited_sources.is_empty())
        {
            return None;
        }

        let mut lines = vec![format!("Browser task objective: {}", self.objective)];
        if self.routing.browser_required {
            lines.push(format!(
                "Task type: {} (browser-oriented).",
                self.routing.reason
            ));
        }
        if !self.required_sources.is_empty() {
            lines.push(format!(
                "Sources mentioned by user: {}",
                self.required_sources.join(", ")
            ));
        }
        if self.compare_mode {
            lines.push("The user expects a comparison across multiple sources.".to_string());
        }
        if let Some(current_source) = &self.current_source {
            lines.push(format!("Currently on: {}", current_source));
        }
        if !self.visited_sources.is_empty() {
            lines.push(format!(
                "Visited so far: {}",
                self.visited_sources.join(", ")
            ));
        }
        if !self.extracted_sources.is_empty() {
            lines.push(format!(
                "Results extracted from: {}",
                self.extracted_sources.join(", ")
            ));
        }
        if self.result_pages_seen > 0 {
            lines.push("Current page shows candidate results.".to_string());
        }

        Some(ChatMessage::user(&lines.join("\n")))
    }
}

impl Default for BrowserRoutingDecision {
    fn default() -> Self {
        Self {
            task_class: BrowserTaskClass::StaticLookup,
            browser_required: false,
            required_sources: Vec::new(),
            reason: "Static lookup".to_string(),
        }
    }
}

impl BrowserRoutingDecision {
    pub fn browser_required(&self) -> bool {
        self.browser_required
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

fn push_unique(items: &mut Vec<String>, value: String) {
    if !items.iter().any(|existing| existing == &value) {
        items.push(value);
    }
}

/// Extract the primary domain name from a URL (e.g. "trenitalia" from "https://www.trenitalia.com/foo").
fn extract_source_name(url: &str) -> Option<String> {
    let trimmed = url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.");
    let host = trimmed.split('/').next()?.trim().to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let host = host
        .split(':')
        .next()
        .unwrap_or(host.as_str())
        .trim()
        .to_string();
    if host.is_empty() {
        return None;
    }
    let source = host
        .split('.')
        .find(|part| !matches!(*part, "com" | "it" | "org" | "net" | "co"))
        .unwrap_or(host.as_str())
        .to_string();
    if source.is_empty() {
        None
    } else {
        Some(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::cognition::{CognitionResult, Complexity, DiscoveredTool};

    fn make_browser_cognition(understanding: &str) -> CognitionResult {
        CognitionResult {
            understanding: understanding.to_string(),
            complexity: Complexity::Complex,
            answer_directly: false,
            direct_answer: None,
            tools: vec![DiscoveredTool {
                name: "browser".to_string(),
                description: "Browser".to_string(),
                reason: "Needs browser".to_string(),
            }],
            skills: Vec::new(),
            mcp_tools: Vec::new(),
            memory_context: None,
            rag_context: None,
            plan: Vec::new(),
            constraints: Vec::new(),
            autonomy_override: None,
        }
    }

    #[test]
    fn from_cognition_classifies_booking() {
        let result = make_browser_cognition("Book a train ticket from Rome to Milan");
        let plan = BrowserTaskPlanState::from_cognition(&result, "book a train");
        assert_eq!(plan.routing.task_class, BrowserTaskClass::FormBooking);
        assert!(plan.routing.browser_required);
    }

    #[test]
    fn from_cognition_classifies_interactive() {
        let result = make_browser_cognition("Search for headphones on Amazon");
        let plan = BrowserTaskPlanState::from_cognition(&result, "find headphones");
        assert_eq!(plan.routing.task_class, BrowserTaskClass::InteractiveWeb);
    }

    #[test]
    fn from_cognition_classifies_compare() {
        let mut result = make_browser_cognition("Compare train prices");
        result.constraints = vec!["confronta prezzi".to_string()];
        let plan = BrowserTaskPlanState::from_cognition(&result, "confronta treni");
        assert_eq!(plan.routing.task_class, BrowserTaskClass::MultiSourceCompare);
        assert!(plan.compare_mode);
    }

    #[test]
    fn from_cognition_static_without_browser() {
        let result = CognitionResult {
            understanding: "Check the weather".to_string(),
            complexity: Complexity::Simple,
            answer_directly: false,
            direct_answer: None,
            tools: vec![DiscoveredTool {
                name: "weather".to_string(),
                description: "Weather".to_string(),
                reason: "Need weather".to_string(),
            }],
            skills: Vec::new(),
            mcp_tools: Vec::new(),
            memory_context: None,
            rag_context: None,
            plan: Vec::new(),
            constraints: Vec::new(),
            autonomy_override: None,
        };
        let plan = BrowserTaskPlanState::from_cognition(&result, "weather?");
        assert_eq!(plan.routing.task_class, BrowserTaskClass::StaticLookup);
        assert!(!plan.routing.browser_required);
    }

    #[test]
    fn requires_snapshot_before_ref_actions_after_navigation() {
        let result = make_browser_cognition("Book a train on trenitalia");
        let mut plan = BrowserTaskPlanState::from_cognition(&result, "book a train");
        plan.note_browser_result(Some("navigate"), "Page URL: https://www.trenitalia.com");
        let veto = plan.veto_browser_action(&serde_json::json!({
            "action": "click",
            "ref_id": "e37"
        }));
        assert!(veto.is_some());
    }

    #[test]
    fn vetoes_non_selection_actions_when_autocomplete_is_open() {
        let result = make_browser_cognition("Book a train on trenitalia");
        let mut plan = BrowserTaskPlanState::from_cognition(&result, "book a train");
        plan.note_browser_result(
            Some("snapshot"),
            "Page URL: https://www.trenitalia.com/\nVisible suggestions:\n- Napoli Centrale\n- Napoli Afragola",
        );
        let veto = plan.veto_browser_action(&serde_json::json!({
            "action": "type",
            "ref_id": "e2",
            "text": "Milano Centrale"
        }));
        assert!(veto.is_some());
    }

    #[test]
    fn vetoes_re_navigate_same_site() {
        let result = make_browser_cognition("Book a train on trenitalia");
        let mut plan = BrowserTaskPlanState::from_cognition(&result, "book a train");
        // Navigate via Google first
        plan.note_browser_result(Some("navigate"), "Page URL: https://www.google.com/search?q=trenitalia");
        plan.note_browser_result(Some("click"), "Page URL: https://www.trenitalia.com/");
        plan.note_browser_result(Some("snapshot"), "Page URL: https://www.trenitalia.com/\n- textbox");
        // Try to re-navigate to same site
        let veto = plan.veto_browser_action(&serde_json::json!({
            "action": "navigate",
            "url": "https://www.trenitalia.com/"
        }));
        assert!(veto.is_some());
        assert!(veto.unwrap().contains("already have trenitalia open"));
    }

    #[test]
    fn vetoes_direct_navigate_for_form_booking() {
        let result = make_browser_cognition("Book a train on trenitalia");
        let plan = BrowserTaskPlanState::from_cognition(&result, "book a train on trenitalia");
        let veto = plan.veto_browser_action(&serde_json::json!({
            "action": "navigate",
            "url": "https://www.trenitalia.com/"
        }));
        assert!(veto.is_some());
        assert!(veto.unwrap().contains("Google search"));
    }

    #[test]
    fn allows_navigate_to_google_for_form_booking() {
        let result = make_browser_cognition("Book a train on trenitalia");
        let plan = BrowserTaskPlanState::from_cognition(&result, "book a train");
        let veto = plan.veto_browser_action(&serde_json::json!({
            "action": "navigate",
            "url": "https://www.google.com/search?q=trenitalia"
        }));
        assert!(veto.is_none());
    }
}
