use crate::provider::ChatMessage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserTaskClass {
    StaticLookup,
    InteractiveWeb,
    FormBooking,
    MultiSourceCompare,
}

#[derive(Debug, Clone)]
pub struct BrowserRoutingDecision {
    task_class: BrowserTaskClass,
    browser_required: bool,
    required_sources: Vec<String>,
    reason: String,
}

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
}

impl BrowserTaskPlanState {
    pub fn new(user_prompt: &str) -> Self {
        let routing = BrowserRoutingDecision::from_prompt(user_prompt);
        let objective = user_prompt.trim().chars().take(220).collect::<String>();
        Self {
            objective,
            compare_mode: matches!(routing.task_class, BrowserTaskClass::MultiSourceCompare),
            required_sources: routing.required_sources.clone(),
            routing,
            visited_sources: Vec::new(),
            extracted_sources: Vec::new(),
            current_source: None,
            result_pages_seen: 0,
            pending_selection: false,
            requires_fresh_snapshot: true,
        }
    }

    pub fn routing_decision(&self) -> &BrowserRoutingDecision {
        &self.routing
    }

    pub fn note_browser_result(&mut self, action: Option<&str>, output: &str) {
        let lower = output.to_ascii_lowercase();
        let mut source = None;
        for line in output.lines() {
            if let Some(url) = line.trim().strip_prefix("Page URL: ") {
                source = extract_source_name(url);
                break;
            }
        }

        if output.contains("Visible result hints:") {
            self.result_pages_seen = self.result_pages_seen.saturating_add(1);
        }
        self.pending_selection = lower.contains("visible suggestions:")
            || lower.contains("autocomplete still open")
            || lower.contains("combobox-style field");

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
            }
            "snapshot" | "take_screenshot" => {
                self.requires_fresh_snapshot = false;
            }
            "click" | "type" | "select" | "choose_suggestion" | "press" | "submit_form"
            | "select_option" | "fill_form" | "press_key" | "drag" => {
                if lower.contains("latest browser step failed") || lower.contains("tool error") {
                    self.requires_fresh_snapshot = true;
                }
            }
            _ => {}
        }
    }

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
    /// `action` is e.g. `Some("click")`, `Some("navigate")` — extracted by `browser_action_from_tool()`.
    pub fn veto_browser_action_mcp(
        &self,
        action: Option<&str>,
        arguments: &serde_json::Value,
    ) -> Option<String> {
        self.veto_browser_action_inner(action.unwrap_or_default(), arguments)
    }

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

    pub fn runtime_message(&self, browser_available: bool) -> Option<ChatMessage> {
        if self.routing.browser_required && !browser_available {
            return Some(ChatMessage::user(&format!(
                "This request requires interactive browser automation ({}) but the browser runtime is unavailable. Do not use web_fetch, web_search, or shell as a surrogate for dynamic booking/comparison sites. Respond immediately with a clear browser-unavailable error.",
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
                "Browser routing: {}. Start with the browser tool, not web_fetch or shell.",
                self.routing.reason
            ));
        }
        if !self.required_sources.is_empty() {
            lines.push(format!(
                "Required browser sources: {}",
                self.required_sources.join(", ")
            ));
        }
        if self.compare_mode {
            lines.push(
                "This is a compare-style browser task. Do not stop after the first source if another source still needs to be checked."
                    .to_string(),
            );
        }
        if let Some(current_source) = &self.current_source {
            lines.push(format!("Current browser source: {}", current_source));
        }
        if !self.visited_sources.is_empty() {
            lines.push(format!(
                "Visited sources so far: {}",
                self.visited_sources.join(", ")
            ));
        }
        if !self.extracted_sources.is_empty() {
            lines.push(format!(
                "Sources with extracted results: {}",
                self.extracted_sources.join(", ")
            ));
        }
        if self.result_pages_seen > 0 {
            lines.push(
                "If the current page already shows candidate results, prefer the browser extract action before further navigation."
                    .to_string(),
            );
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
    pub fn from_prompt(user_prompt: &str) -> Self {
        let lower = user_prompt.to_ascii_lowercase();
        let required_sources = extract_required_sources(&lower);
        let compare_mode = contains_any(
            &lower,
            &[
                "compare",
                "confronta",
                "both",
                "sia",
                "che",
                "piu economico",
                "più economico",
                "cheaper",
                "lowest price",
                "compare prices",
            ],
        ) || required_sources.len() > 1;
        let booking_intent = contains_any(
            &lower,
            &[
                "train",
                "treno",
                "flight",
                "volo",
                "hotel",
                "biglietto",
                "ticket",
                "prenot",
                "book",
                "booking",
                "checkout",
                "fare search",
                "noleggio",
                "rental",
            ],
        );
        let interactive_web = booking_intent
            || !required_sources.is_empty()
            || contains_any(
                &lower,
                &[
                    "login",
                    "accedi",
                    "form",
                    "select",
                    "dropdown",
                    "picker",
                    "dynamic",
                    "js",
                    "javascript",
                    "browser",
                ],
            );

        let (task_class, browser_required, reason) = if compare_mode && interactive_web {
            (
                BrowserTaskClass::MultiSourceCompare,
                true,
                "multi-source comparison on interactive websites".to_string(),
            )
        } else if booking_intent {
            (
                BrowserTaskClass::FormBooking,
                true,
                "travel/booking workflow with dynamic form state".to_string(),
            )
        } else if interactive_web {
            (
                BrowserTaskClass::InteractiveWeb,
                true,
                "interactive or JS-rendered website workflow".to_string(),
            )
        } else {
            (
                BrowserTaskClass::StaticLookup,
                false,
                "static lookup".to_string(),
            )
        };

        Self {
            task_class,
            browser_required,
            required_sources,
            reason,
        }
    }

    pub fn task_class(&self) -> BrowserTaskClass {
        self.task_class
    }

    pub fn browser_required(&self) -> bool {
        self.browser_required
    }

    pub fn required_sources(&self) -> &[String] {
        &self.required_sources
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub fn named_sources_known(&self) -> bool {
        !self.required_sources.is_empty()
    }
}

fn push_unique(items: &mut Vec<String>, value: String) {
    if !items.iter().any(|existing| existing == &value) {
        items.push(value);
    }
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn extract_required_sources(lower_prompt: &str) -> Vec<String> {
    let mut sources = Vec::new();
    for (source, needles) in [
        ("trenitalia", &["trenitalia"][..]),
        ("italo", &["italo", "italotreno"][..]),
        ("trainline", &["trainline"][..]),
        ("omio", &["omio"][..]),
        ("booking", &["booking.com", "booking"][..]),
        ("amazon", &["amazon"][..]),
        ("ebay", &["ebay"][..]),
        ("skyscanner", &["skyscanner"][..]),
    ] {
        if needles.iter().any(|needle| lower_prompt.contains(needle)) {
            push_unique(&mut sources, source.to_string());
        }
    }
    sources
}

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
    use super::{BrowserRoutingDecision, BrowserTaskClass, BrowserTaskPlanState};

    #[test]
    fn classifies_booking_compare_as_browser_first() {
        let decision = BrowserRoutingDecision::from_prompt(
            "mi trovi un treno domani da napoli a milano confrontando trenitalia e italo",
        );
        assert_eq!(decision.task_class(), BrowserTaskClass::MultiSourceCompare);
        assert!(decision.browser_required());
        assert_eq!(decision.required_sources(), &["trenitalia", "italo"]);
    }

    #[test]
    fn browser_plan_tracks_sources_and_extractions() {
        let mut plan =
            BrowserTaskPlanState::new("compare trenitalia and italo for tomorrow after 16:00");
        plan.note_browser_result(
            Some("snapshot"),
            "Page URL: https://www.trenitalia.com/en.html\nVisible result hints:\n1. Napoli Centrale 16:20 Milano Centrale",
        );
        let message = plan
            .runtime_message(true)
            .expect("expected runtime message");
        let rendered = message.rendered_text().unwrap();
        assert!(rendered.contains("Required browser sources: trenitalia, italo"));
        assert!(rendered.contains("Visited sources so far: trenitalia"));
        assert!(rendered.contains("prefer the browser extract action"));

        plan.note_browser_result(
            Some("extract"),
            "Page URL: https://www.italotreno.it/\nExtracted results:\n1. Napoli Centrale - Milano Centrale | price=EUR 49 | time=16:30",
        );
        let rendered = plan.runtime_message(true).unwrap().rendered_text().unwrap();
        assert!(rendered.contains("Sources with extracted results: italo"));
        assert!(rendered.contains("Current browser source: italo"));
    }

    #[test]
    fn returns_unavailable_message_for_required_browser_tasks() {
        let plan = BrowserTaskPlanState::new("compare trenitalia and italo trains");
        let rendered = plan
            .runtime_message(false)
            .unwrap()
            .rendered_text()
            .unwrap();
        assert!(rendered.contains("browser runtime is unavailable"));
    }

    #[test]
    fn vetoes_non_selection_actions_when_autocomplete_is_open() {
        let mut plan = BrowserTaskPlanState::new("book a train on trenitalia");
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
    fn vetoes_switching_sources_before_extracting_current_results() {
        let mut plan =
            BrowserTaskPlanState::new("compare trenitalia and italo for tomorrow after 16:00");
        plan.note_browser_result(
            Some("snapshot"),
            "Page URL: https://www.trenitalia.com/en.html\nVisible result hints:\n1. Napoli Centrale 16:20 Milano Centrale",
        );
        let veto = plan.veto_browser_action(&serde_json::json!({
            "action": "navigate",
            "url": "https://www.italotreno.it/"
        }));
        assert!(veto.is_some());
    }

    #[test]
    fn requires_snapshot_before_ref_actions_after_navigation() {
        let mut plan = BrowserTaskPlanState::new("book a train on trenitalia");
        plan.note_browser_result(Some("navigate"), "Page URL: https://www.trenitalia.com");
        let veto = plan.veto_browser_action(&serde_json::json!({
            "action": "click",
            "ref_id": "e37"
        }));
        assert!(veto.is_some());
    }

    #[test]
    fn vetoes_close_when_current_source_is_not_extracted_yet() {
        let mut plan =
            BrowserTaskPlanState::new("compare trenitalia and italo for tomorrow after 16:00");
        plan.note_browser_result(
            Some("snapshot"),
            "Page URL: https://www.trenitalia.com/en.html\nVisible result hints:\n1. Napoli Centrale 16:20 Milano Centrale",
        );
        let veto = plan.veto_browser_action(&serde_json::json!({
            "action": "close"
        }));
        assert!(veto.is_some());
    }
}
