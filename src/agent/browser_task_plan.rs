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
    /// Whether the agent has entered a booking site via Google search.
    used_google_entry: bool,
}

impl BrowserTaskPlanState {
    pub fn new(user_prompt: &str) -> Self {
        let routing = BrowserRoutingDecision::from_prompt(user_prompt);
        let objective = crate::utils::text::truncate_str(user_prompt.trim(), 220, "");
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
            used_google_entry: false,
        }
    }

    pub fn routing_decision(&self) -> &BrowserRoutingDecision {
        &self.routing
    }

    /// Whether a results page has been seen during this browser task.
    pub fn has_seen_results(&self) -> bool {
        self.result_pages_seen > 0
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
                // Track Google entry for FormBooking tasks
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
                // Click from Google results page completes the Google entry
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

        // --- Google-first veto for FormBooking tasks ---
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

        // --- Veto re-navigating to a site we already have open (applies to ALL task types) ---
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

        // --- Compare-mode vetoes: don't leave or close before extracting ---
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
    pub fn from_prompt(user_prompt: &str) -> Self {
        let lower = user_prompt.to_ascii_lowercase();
        let required_sources = extract_required_sources(&lower);
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
        let shopping_intent = contains_any(
            &lower,
            &[
                "scarpe",
                "shoes",
                "buy ",
                "compra",
                "shop",
                "shopping",
                "product",
                "prodott",
                "price",
                "prezzo",
                "taglia",
                "size",
                "collezione",
                "collection",
                "store",
                "negozio",
                "abbigliamento",
                "clothing",
                "borsa",
                "bag",
                "orologio",
                "watch",
            ],
        );
        let has_price_constraint = contains_any(
            &lower,
            &[
                "costano meno",
                "non costano più",
                "meno di",
                "sotto ",
                "under ",
                "below ",
                "cheaper",
                "cheapest",
                "economico",
                "economica",
                "lowest price",
                "best price",
                "miglior prezzo",
                "budget",
            ],
        );
        let compare_mode = contains_any(
            &lower,
            &[
                "compare",
                "confronta",
                "piu economico",
                "più economico",
                "cheaper",
                "lowest price",
                "compare prices",
            ],
        ) || contains_word(&lower, "both")
            || contains_word(&lower, "sia")
            || contains_word(&lower, "che")
            || required_sources.len() > 1
            || (shopping_intent && has_price_constraint);
        let interactive_web = booking_intent
            || shopping_intent
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

/// Check if a short word appears as a standalone word (not inside another word).
/// Boundary = start/end of string, or a non-alphanumeric char.
fn contains_word(text: &str, word: &str) -> bool {
    for (idx, _) in text.match_indices(word) {
        let before_ok = idx == 0 || !text.as_bytes()[idx - 1].is_ascii_alphanumeric();
        let after = idx + word.len();
        let after_ok = after >= text.len() || !text.as_bytes()[after].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

fn extract_required_sources(lower_prompt: &str) -> Vec<String> {
    let mut sources = Vec::new();

    // Tier 1: well-known sites (exact match on lowercase)
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

    // Tier 2: generic brand detection from "X o Y" / "X or Y" patterns
    if sources.is_empty() {
        let brand_sources = extract_brand_sources(lower_prompt);
        for s in brand_sources {
            push_unique(&mut sources, s);
        }
    }

    sources
}

/// Extract brand/site names from patterns like "X o Y", "X or Y", "di X o di Y".
///
/// Works on the lowercased prompt. Looks for connectors (o, or, e, and) and
/// extracts the nearest non-filler word on each side as a brand name.
fn extract_brand_sources(lower: &str) -> Vec<String> {
    let connectors = [" o ", " or ", " e ", " and "];
    let filler: &[&str] = &[
        "di", "da", "le", "il", "la", "lo", "un", "una", "del", "al", "per", "con", "su", "the",
        "a", "an", "in", "on", "for", "to", "from", "my", "me", "this", "that", "più", "piu",
        "meno", "anche", "poi", "tipo", "come", "quale",
    ];
    let is_brand = |word: &str| -> bool {
        word.len() >= 3
            && word.chars().all(|c| c.is_alphanumeric() || c == '-')
            && !filler.contains(&word)
    };

    let words: Vec<&str> = lower.split_whitespace().collect();
    let mut brands = Vec::new();

    for conn in &connectors {
        let conn_word = conn.trim();
        for (i, &w) in words.iter().enumerate() {
            if w != conn_word {
                continue;
            }
            // Find nearest brand word BEFORE connector (skip filler like "di")
            let before = (0..i).rev().map(|j| words[j]).find(|word| is_brand(word));
            // Find nearest brand word AFTER connector (skip filler like "di")
            let after = ((i + 1)..words.len())
                .map(|j| words[j])
                .take(3) // look at most 3 words ahead
                .find(|word| is_brand(word));

            if let (Some(b), Some(a)) = (before, after) {
                push_unique(&mut brands, b.to_string());
                push_unique(&mut brands, a.to_string());
            }
        }
    }
    brands
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
        assert!(rendered.contains("Sources mentioned by user: trenitalia, italo"));
        assert!(rendered.contains("Visited so far: trenitalia"));
        assert!(rendered.contains("Current page shows candidate results"));

        plan.note_browser_result(
            Some("extract"),
            "Page URL: https://www.italotreno.it/\nExtracted results:\n1. Napoli Centrale - Milano Centrale | price=EUR 49 | time=16:30",
        );
        let rendered = plan.runtime_message(true).unwrap().rendered_text().unwrap();
        assert!(rendered.contains("Results extracted from: italo"));
        assert!(rendered.contains("Currently on: italo"));
    }

    #[test]
    fn returns_unavailable_message_for_required_browser_tasks() {
        let plan = BrowserTaskPlanState::new("compare trenitalia and italo trains");
        let rendered = plan
            .runtime_message(false)
            .unwrap()
            .rendered_text()
            .unwrap();
        assert!(rendered.contains("browser is currently unavailable"));
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

    #[test]
    fn classifies_shopping_compare_as_multi_source() {
        let decision = BrowserRoutingDecision::from_prompt(
            "mi trovi delle scarpe di pelle marrone da uomo taglia 44 di prada o di gucci",
        );
        assert_eq!(decision.task_class(), BrowserTaskClass::MultiSourceCompare);
        assert!(decision.browser_required());
        assert!(decision.required_sources().contains(&"prada".to_string()));
        assert!(decision.required_sources().contains(&"gucci".to_string()));
    }

    #[test]
    fn generic_brand_extraction_works() {
        use super::extract_brand_sources;
        let brands = extract_brand_sources("scarpe prada o gucci taglia 44");
        assert!(brands.contains(&"prada".to_string()));
        assert!(brands.contains(&"gucci".to_string()));

        // English
        let brands = extract_brand_sources("shoes from nike or adidas size 10");
        assert!(brands.contains(&"nike".to_string()));
        assert!(brands.contains(&"adidas".to_string()));

        // "e" connector
        let brands = extract_brand_sources("confronta zara e mango per vestiti");
        assert!(brands.contains(&"zara".to_string()));
        assert!(brands.contains(&"mango".to_string()));
    }

    #[test]
    fn generic_brand_skips_filler_words() {
        use super::extract_brand_sources;
        // "di" and "il" are filler, should not be extracted
        let brands = extract_brand_sources("il prezzo di un prodotto");
        assert!(brands.is_empty());
    }

    #[test]
    fn generic_shopping_with_price_constraint_is_multi_source() {
        let decision = BrowserRoutingDecision::from_prompt(
            "trovami delle scarpe classiche marroni di pelle taglia 44 che non costano più di 50 euro",
        );
        assert_eq!(decision.task_class(), BrowserTaskClass::MultiSourceCompare);
        assert!(decision.browser_required());
        // No named sources — aggregator hint should appear instead
        assert!(decision.required_sources().is_empty());
    }

    #[test]
    fn generic_shopping_runtime_message_shows_comparison_context() {
        let plan = BrowserTaskPlanState::new(
            "trovami scarpe classiche marroni taglia 44 che non costano più di 50 euro",
        );
        let msg = plan
            .runtime_message(true)
            .expect("expected runtime message");
        let rendered = msg.rendered_text().unwrap();
        assert!(rendered.contains("comparison across multiple sources"));
    }

    #[test]
    fn shopping_without_price_constraint_is_interactive_web() {
        // Just shopping intent, no price constraint, no brands → InteractiveWeb (not MultiSource)
        let decision = BrowserRoutingDecision::from_prompt(
            "trovami delle scarpe classiche marroni di pelle taglia 44",
        );
        assert_eq!(decision.task_class(), BrowserTaskClass::InteractiveWeb);
        assert!(decision.browser_required());
    }

    #[test]
    fn known_sources_still_work() {
        let decision =
            BrowserRoutingDecision::from_prompt("confronta trenitalia e italo per domani");
        assert_eq!(decision.task_class(), BrowserTaskClass::MultiSourceCompare);
        assert!(decision
            .required_sources()
            .contains(&"trenitalia".to_string()));
        assert!(decision.required_sources().contains(&"italo".to_string()));
    }

    #[test]
    fn vetoes_re_navigate_same_site_for_form_booking() {
        // FormBooking (NOT compare_mode) — the re-navigate veto must still fire.
        let mut plan =
            BrowserTaskPlanState::new("prenotami un treno su trenitalia da napoli a milano");
        assert!(!plan.compare_mode, "should NOT be compare mode");
        // Simulate: agent entered via Google (so Google-first veto is satisfied).
        plan.note_browser_result(
            Some("navigate"),
            "Page URL: https://www.google.com/search?q=trenitalia",
        );
        plan.note_browser_result(Some("click"), "Page URL: https://www.trenitalia.com/");
        assert!(plan.used_google_entry);
        // Agent got a results page.
        plan.note_browser_result(
            Some("snapshot"),
            "Page URL: https://www.trenitalia.com/it/treni_regionali.html\nVisible result hints:\n1. 12:30 Frecciarossa",
        );
        assert_eq!(plan.current_source.as_deref(), Some("trenitalia"));
        // Now the agent tries to navigate back to trenitalia homepage — must be vetoed.
        let veto = plan.veto_browser_action(&serde_json::json!({
            "action": "navigate",
            "url": "https://www.trenitalia.com/"
        }));
        assert!(
            veto.is_some(),
            "re-navigate to same site should be vetoed even for FormBooking"
        );
        assert!(veto.unwrap().contains("already have trenitalia open"));
    }

    #[test]
    fn vetoes_direct_navigate_for_form_booking() {
        let plan = BrowserTaskPlanState::new("book a train on trenitalia");
        let veto = plan.veto_browser_action(&serde_json::json!({
            "action": "navigate",
            "url": "https://www.trenitalia.com/"
        }));
        assert!(
            veto.is_some(),
            "direct navigate to booking site must be vetoed"
        );
        assert!(veto.unwrap().contains("Google search"));
    }

    #[test]
    fn allows_navigate_to_google_for_form_booking() {
        let plan = BrowserTaskPlanState::new("book a train on trenitalia");
        let veto = plan.veto_browser_action(&serde_json::json!({
            "action": "navigate",
            "url": "https://www.google.com/search?q=trenitalia"
        }));
        assert!(
            veto.is_none(),
            "navigate to Google itself should be allowed"
        );
    }

    #[test]
    fn allows_direct_navigate_after_google_entry() {
        let mut plan = BrowserTaskPlanState::new("book a train on trenitalia");
        // Navigate to Google
        plan.note_browser_result(
            Some("navigate"),
            "Page URL: https://www.google.com/search?q=trenitalia",
        );
        // Click from Google results
        plan.note_browser_result(Some("click"), "Page URL: https://www.trenitalia.com/");
        assert!(plan.used_google_entry, "Google entry should be tracked");
        // Snapshot to clear requires_fresh_snapshot
        plan.note_browser_result(
            Some("snapshot"),
            "Page URL: https://www.trenitalia.com/\n- textbox \"From\" [ref=e1]",
        );
        // Direct navigate to a subpage should now be allowed
        // (though re-navigate veto may still fire for same domain)
        let veto = plan.veto_browser_action(&serde_json::json!({
            "action": "navigate",
            "url": "https://www.italotreno.it/"
        }));
        // No Google-first veto (used_google_entry = true), and different domain so no re-nav veto
        assert!(
            veto.is_none(),
            "after Google entry, direct navigate should be allowed"
        );
    }

    #[test]
    fn no_google_veto_for_static_lookup() {
        let plan = BrowserTaskPlanState::new("check the weather in london");
        let veto = plan.veto_browser_action(&serde_json::json!({
            "action": "navigate",
            "url": "https://weather.com/"
        }));
        assert!(
            veto.is_none(),
            "StaticLookup should not trigger Google-first veto"
        );
    }

    #[test]
    fn no_google_veto_for_interactive_web() {
        let plan = BrowserTaskPlanState::new("go to amazon and find headphones");
        let veto = plan.veto_browser_action(&serde_json::json!({
            "action": "navigate",
            "url": "https://www.amazon.com/"
        }));
        // InteractiveWeb (shopping without booking keywords) — no Google-first veto
        // Note: "amazon" triggers InteractiveWeb via required_sources, not FormBooking
        assert!(
            veto.is_none(),
            "InteractiveWeb should not trigger Google-first veto"
        );
    }
}
