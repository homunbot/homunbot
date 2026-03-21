use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::provider::ChatMessage;

const MAX_PLAN_ITEMS: usize = 6;

// ── Explicit plan types ────────────────────────────────────────────

/// Status of an explicit plan step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
}

/// A single step in an explicit plan created by the LLM via `plan_task`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub description: String,
    pub status: StepStatus,
}

/// Serializable step for the web UI snapshot.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlanStepSnapshot {
    pub description: String,
    pub status: String, // "pending" | "in_progress" | "completed"
}

// ── Snapshot (streamed to web UI) ──────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionPlanSnapshot {
    pub objective: String,
    pub constraints: Vec<String>,
    pub completed_steps: Vec<String>,
    pub active_blockers: Vec<String>,
    pub required_sources: Vec<String>,
    pub completed_sources: Vec<String>,
    pub current_source: Option<String>,
    /// Explicit plan steps created by the LLM via `plan_task`.
    /// Empty when using the default keyword-inferred mode.
    #[serde(default)]
    pub explicit_steps: Vec<PlanStepSnapshot>,
    /// Optional verification criterion supplied with the plan.
    #[serde(default)]
    pub verification: Option<String>,
    /// Orchestrator phase: "planning", "executing", "synthesizing".
    /// Empty when not using the task orchestrator.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phase: String,
}

// ── Core state ─────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct ExecutionPlanState {
    objective: String,
    constraints: Vec<String>,
    completed_steps: Vec<String>,
    active_blockers: Vec<String>,
    seen_step_signatures: HashSet<String>,
    /// Explicit plan steps set by the LLM via the virtual `plan_task` tool.
    /// When non-empty, `runtime_message()` renders these instead of
    /// keyword-inferred constraints.
    explicit_steps: Vec<PlanStep>,
    /// Optional verification note for the final goal check.
    verification: Option<String>,
}

impl ExecutionPlanState {
    pub fn new(user_prompt: &str) -> Self {
        Self {
            objective: compact(user_prompt, 220),
            constraints: infer_constraints(user_prompt),
            completed_steps: Vec::new(),
            active_blockers: Vec::new(),
            seen_step_signatures: HashSet::new(),
            explicit_steps: Vec::new(),
            verification: None,
        }
    }

    pub fn note_tool_result(
        &mut self,
        tool_name: &str,
        arguments: &Value,
        output: &str,
        is_error: bool,
    ) {
        let signature = format!(
            "{}:{}",
            tool_name,
            serde_json::to_string(arguments).unwrap_or_else(|_| "{}".to_string())
        );

        if !is_error && self.seen_step_signatures.insert(signature) {
            let summary = summarize_step(tool_name, arguments, output);
            push_unique_limited(&mut self.completed_steps, summary, MAX_PLAN_ITEMS);
        }

        let blockers = infer_blockers(tool_name, output, is_error);
        if blockers.is_empty() {
            if !is_error {
                self.active_blockers.clear();
            }
        } else {
            self.active_blockers = blockers;
        }
    }

    // ── Explicit plan methods ──────────────────────────────────────

    /// Create an explicit plan. Called when the LLM invokes `plan_task`.
    /// Replaces any prior explicit plan. Marks step[0] as InProgress.
    pub fn set_explicit_plan(&mut self, steps: Vec<String>, verification: Option<String>) {
        self.explicit_steps = steps
            .into_iter()
            .enumerate()
            .map(|(i, desc)| PlanStep {
                description: desc,
                status: if i == 0 {
                    StepStatus::InProgress
                } else {
                    StepStatus::Pending
                },
            })
            .collect();
        self.verification = verification;
    }

    /// Mark a plan step as completed and auto-advance the next pending
    /// step to InProgress. Called when the LLM invokes `complete_step`.
    pub fn complete_step(&mut self, step_index: usize) -> Result<String, String> {
        if self.explicit_steps.is_empty() {
            return Err("No plan exists. Call plan_task first.".to_string());
        }
        if step_index >= self.explicit_steps.len() {
            return Err(format!(
                "Step index {} is out of range (plan has {} steps).",
                step_index,
                self.explicit_steps.len()
            ));
        }

        self.explicit_steps[step_index].status = StepStatus::Completed;

        // Auto-advance: find the next pending step and mark it InProgress.
        for step in &mut self.explicit_steps {
            if step.status == StepStatus::Pending {
                step.status = StepStatus::InProgress;
                break;
            }
        }

        let desc = &self.explicit_steps[step_index].description;
        Ok(format!("Step {} completed: {}", step_index, desc))
    }

    /// Whether all explicit steps are done.
    pub fn all_steps_completed(&self) -> bool {
        !self.explicit_steps.is_empty()
            && self
                .explicit_steps
                .iter()
                .all(|s| s.status == StepStatus::Completed)
    }

    /// Whether an explicit plan is active.
    pub fn has_explicit_plan(&self) -> bool {
        !self.explicit_steps.is_empty()
    }

    // ── Runtime message (injected each iteration) ───────────────

    pub fn runtime_message(&self) -> Option<ChatMessage> {
        if self.objective.is_empty()
            && self.constraints.is_empty()
            && self.completed_steps.is_empty()
            && self.active_blockers.is_empty()
            && self.explicit_steps.is_empty()
        {
            return None;
        }

        let mut lines = Vec::new();
        if !self.objective.is_empty() {
            lines.push(format!("Execution objective: {}", self.objective));
        }

        if !self.explicit_steps.is_empty() {
            // ── Explicit plan mode ──────────────────────────────
            lines.push("Execution plan:".to_string());
            for (i, step) in self.explicit_steps.iter().enumerate() {
                let tag = match step.status {
                    StepStatus::Completed => "[DONE]",
                    StepStatus::InProgress => "[CURRENT]",
                    StepStatus::Pending => "[TODO]",
                };
                lines.push(format!("  {} Step {}: {}", tag, i, step.description));
            }
            if let Some(ref v) = self.verification {
                lines.push(format!("Verification: {}", v));
            }
            if self.all_steps_completed() {
                let note = self
                    .verification
                    .as_deref()
                    .unwrap_or("Verify the results match the original request");
                lines.push(format!(
                    "All planned steps completed. {}. Review the results before giving your final response.",
                    note
                ));
            }
        } else {
            // ── Inferred mode (existing behavior) ──────────────
            if !self.constraints.is_empty() {
                lines.push("Constraints to satisfy before finishing:".to_string());
                for item in &self.constraints {
                    lines.push(format!("- {}", item));
                }
            }
        }

        // Completed tool results and blockers always apply.
        if !self.completed_steps.is_empty() {
            lines.push("Completed so far:".to_string());
            for item in &self.completed_steps {
                lines.push(format!("- {}", item));
            }
        }
        if !self.active_blockers.is_empty() {
            lines.push("Active blockers to resolve next:".to_string());
            for item in &self.active_blockers {
                lines.push(format!("- {}", item));
            }
        }
        lines.push(
            "Plan rule: continue from the remaining blockers/constraints instead of restarting; only finalize once the requested outcome is actually achieved or clearly impossible."
                .to_string(),
        );

        Some(ChatMessage::user(&lines.join("\n")))
    }

    // ── Snapshot (for web UI streaming) ─────────────────────────

    pub fn snapshot(&self) -> ExecutionPlanSnapshot {
        ExecutionPlanSnapshot {
            objective: self.objective.clone(),
            constraints: self.constraints.clone(),
            completed_steps: self.completed_steps.clone(),
            active_blockers: self.active_blockers.clone(),
            required_sources: Vec::new(),
            completed_sources: Vec::new(),
            current_source: None,
            explicit_steps: self
                .explicit_steps
                .iter()
                .map(|s| PlanStepSnapshot {
                    description: s.description.clone(),
                    status: match s.status {
                        StepStatus::Pending => "pending".to_string(),
                        StepStatus::InProgress => "in_progress".to_string(),
                        StepStatus::Completed => "completed".to_string(),
                    },
                })
                .collect(),
            verification: self.verification.clone(),
            phase: String::new(),
        }
    }
}

fn infer_constraints(user_prompt: &str) -> Vec<String> {
    let text = user_prompt.to_ascii_lowercase();
    let mut constraints = Vec::new();
    let requested_sources = infer_named_sources(&text);

    if contains_any(
        &text,
        &[
            "compare",
            "confronta",
            "versus",
            " vs ",
            "both ",
            "entrambi",
            "sia ",
            "che ",
        ],
    ) {
        constraints.push(
            "Cover every requested option/source and compare them before finalizing.".to_string(),
        );
    }

    if contains_any(
        &text,
        &[
            "today",
            "oggi",
            "tomorrow",
            "domani",
            "latest",
            "current",
            "adesso",
            "stasera",
            "tonight",
            "this week",
            "questa settimana",
        ],
    ) {
        constraints.push(
            "Treat date/time-sensitive details as current and verify them from fresh evidence."
                .to_string(),
        );
    }

    if contains_any(
        &text,
        &[
            "after ",
            "before ",
            "dopo ",
            "prima delle",
            "entro ",
            "under ",
            "below ",
            "meno di",
            "fino a",
            "at least",
            "almeno",
            "between ",
            "tra ",
        ],
    ) || text.contains(':')
        || text.chars().any(|ch| ch.is_ascii_digit())
    {
        constraints.push(
            "Respect explicit numeric, date, price, time, and threshold constraints from the request."
                .to_string(),
        );
    }

    if contains_any(
        &text,
        &[
            "book",
            "booking",
            "reserve",
            "reservation",
            "ticket",
            "biglietto",
            "prenota",
            "checkout",
            "order",
            "buy",
            "purchase",
            "search form",
            "form",
        ],
    ) {
        constraints.push(
            "For multi-step forms, confirm each required field/widget before submitting."
                .to_string(),
        );
    }

    if contains_any(
        &text,
        &[
            "and ", " e ", " then ", " poi ", " also ", " anche ", "oltre ", "plus ",
        ],
    ) {
        constraints
            .push("Complete all distinct sub-requests in the prompt before stopping.".to_string());
    }

    if !requested_sources.is_empty() {
        constraints.push(format!(
            "Required sources to cover: {}.",
            requested_sources.join(", ")
        ));
    }

    constraints.truncate(MAX_PLAN_ITEMS);
    constraints
}

fn infer_blockers(tool_name: &str, output: &str, is_error: bool) -> Vec<String> {
    let lower = output.to_ascii_lowercase();
    let mut blockers = Vec::new();

    if lower.contains("blocked click on element [") && lower.contains("form still looks incomplete")
    {
        blockers.push(
            "The form still appears incomplete; resolve missing/unfinished widgets before trying to submit again."
                .to_string(),
        );
    }
    if lower.contains("visible suggestions:") || lower.contains("autocomplete") {
        blockers.push(
            "A typed field still needs an explicit autocomplete/combobox selection.".to_string(),
        );
    }
    if lower.contains("date picker appears to be open") {
        blockers.push("A date picker is open and still needs an explicit selection.".to_string());
    }
    if lower.contains("time options appear to be open") {
        blockers.push("Time options are open and still need an explicit selection.".to_string());
    }
    if lower.contains("tool vetoed:") {
        blockers.push(compact(output, 220));
    }
    if lower.contains("this appears to be an error page") {
        blockers.push(
            "Navigation hit an error page. Try the site's homepage or an alternative URL."
                .to_string(),
        );
    }
    if is_error && blockers.is_empty() {
        blockers.push(format!(
            "Latest {} step failed; inspect the last tool result and adjust the next action instead of repeating blindly.",
            tool_name
        ));
    }

    blockers.truncate(MAX_PLAN_ITEMS);
    blockers
}

fn summarize_step(tool_name: &str, arguments: &Value, output: &str) -> String {
    match tool_name {
        "web_search" => format!(
            "Searched the web for {}.",
            arguments
                .get("query")
                .and_then(|v| v.as_str())
                .map(|v| compact(v, 80))
                .unwrap_or_else(|| "the request".to_string())
        ),
        "web_fetch" => format!(
            "Read {}.",
            arguments
                .get("url")
                .and_then(|v| v.as_str())
                .map(|v| compact(v, 96))
                .unwrap_or_else(|| "a source".to_string())
        ),
        "shell" => "Ran a shell step.".to_string(),
        _ if crate::browser::is_browser_tool(tool_name) => {
            let action = arguments
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("action");
            if let Some(source) = infer_source_from_browser_output(output) {
                if output.contains("Extracted results:") {
                    format!("Extracted browser results from {}.", source)
                } else {
                    format!(
                        "Browser step completed on {}: {}.",
                        source,
                        compact(action, 40)
                    )
                }
            } else {
                format!("Browser step completed: {}.", compact(action, 40))
            }
        }
        _ => {
            let summary = compact(output, 96);
            if summary.is_empty() {
                format!("Completed {}.", tool_name)
            } else {
                format!("{}: {}", tool_name, summary)
            }
        }
    }
}

fn push_unique_limited(items: &mut Vec<String>, item: String, limit: usize) {
    if item.trim().is_empty() || items.iter().any(|existing| existing == &item) {
        return;
    }
    items.push(item);
    if items.len() > limit {
        let overflow = items.len() - limit;
        items.drain(0..overflow);
    }
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn compact(value: &str, max_len: usize) -> String {
    let joined = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.len() <= max_len {
        return joined;
    }
    let mut chars = joined.chars();
    let truncated: String = chars.by_ref().take(max_len.saturating_sub(1)).collect();
    if truncated.is_empty() {
        String::new()
    } else {
        format!("{}…", truncated)
    }
}

fn infer_named_sources(text: &str) -> Vec<String> {
    let mut sources = Vec::new();

    // Tier 1: well-known sites (exact match)
    for (source, needles) in [
        ("trenitalia", &["trenitalia"][..]),
        ("italo", &["italo", "italotreno"][..]),
        ("amazon", &["amazon"][..]),
        ("ebay", &["ebay"][..]),
        ("booking", &["booking"][..]),
        ("trainline", &["trainline"][..]),
        ("omio", &["omio"][..]),
    ] {
        if needles.iter().any(|needle| text.contains(needle)) && !sources.contains(&source) {
            sources.push(source);
        }
    }

    let mut result: Vec<String> = sources.into_iter().map(|s| s.to_string()).collect();

    // Tier 2: generic brand detection from "X o Y" / "X or Y" patterns
    if result.is_empty() {
        for s in extract_brand_pair(text) {
            if !result.contains(&s) {
                result.push(s);
            }
        }
    }

    result
}

/// Extract brand pair from "X o Y" / "X or Y" / "X e Y" patterns.
///
/// Skips filler words (articles, prepositions) to handle "di Prada o di Gucci".
fn extract_brand_pair(lower: &str) -> Vec<String> {
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
            let before = (0..i).rev().map(|j| words[j]).find(|w| is_brand(w));
            let after = ((i + 1)..words.len())
                .map(|j| words[j])
                .take(3)
                .find(|w| is_brand(w));
            if let (Some(b), Some(a)) = (before, after) {
                if !brands.contains(&b.to_string()) {
                    brands.push(b.to_string());
                }
                if !brands.contains(&a.to_string()) {
                    brands.push(a.to_string());
                }
            }
        }
    }
    brands
}

fn infer_source_from_browser_output(output: &str) -> Option<String> {
    let url = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("Page URL: "))?;
    let host = url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .split('/')
        .next()?;
    let source = host
        .split('.')
        .find(|part| !matches!(*part, "com" | "it" | "org" | "net" | "co"))?;
    if source.is_empty() {
        None
    } else {
        Some(source.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionPlanState, StepStatus};

    // ── Existing tests ─────────────────────────────────────────

    #[test]
    fn infers_generic_constraints_from_prompt() {
        let state = ExecutionPlanState::new(
            "find me a train tomorrow after 16:00 and compare both Trenitalia and Italo",
        );
        let message = state.runtime_message().expect("plan message");
        let content = message.content.expect("content");
        assert!(content.contains("compare them before finalizing"));
        assert!(content.contains("date/time-sensitive"));
        assert!(content.contains("Respect explicit numeric"));
        assert!(content.contains("Required sources to cover: trenitalia, italo"));
    }

    #[test]
    fn records_blockers_from_browser_form_hints() {
        let mut state = ExecutionPlanState::new("book a train ticket");
        state.note_tool_result(
            "playwright__browser_type",
            &serde_json::json!({"ref":"e1","text":"napoli"}),
            "Visible suggestions: Napoli Centrale | Napoli Afragola. This field likely requires selecting an explicit suggestion before continuing.",
            false,
        );
        let content = state
            .runtime_message()
            .and_then(|msg| msg.content)
            .expect("content");
        assert!(content.contains("autocomplete/combobox selection"));
    }

    // ── Explicit plan tests ────────────────────────────────────

    #[test]
    fn explicit_plan_creation_marks_first_step_in_progress() {
        let mut state = ExecutionPlanState::new("build a website");
        state.set_explicit_plan(
            vec!["Design mockup".into(), "Write HTML".into(), "Deploy".into()],
            Some("Check site is live".into()),
        );

        assert!(state.has_explicit_plan());
        assert_eq!(state.explicit_steps[0].status, StepStatus::InProgress);
        assert_eq!(state.explicit_steps[1].status, StepStatus::Pending);
        assert_eq!(state.explicit_steps[2].status, StepStatus::Pending);
        assert!(!state.all_steps_completed());
    }

    #[test]
    fn complete_step_advances_to_next() {
        let mut state = ExecutionPlanState::new("multi-step task");
        state.set_explicit_plan(
            vec!["Step A".into(), "Step B".into(), "Step C".into()],
            None,
        );

        // Complete step 0 → step 1 becomes InProgress
        let msg = state.complete_step(0).unwrap();
        assert!(msg.contains("Step A"));
        assert_eq!(state.explicit_steps[0].status, StepStatus::Completed);
        assert_eq!(state.explicit_steps[1].status, StepStatus::InProgress);
        assert_eq!(state.explicit_steps[2].status, StepStatus::Pending);

        // Complete step 1 → step 2 becomes InProgress
        state.complete_step(1).unwrap();
        assert_eq!(state.explicit_steps[2].status, StepStatus::InProgress);

        // Complete step 2 → all done
        state.complete_step(2).unwrap();
        assert!(state.all_steps_completed());
    }

    #[test]
    fn complete_step_out_of_range_returns_error() {
        let mut state = ExecutionPlanState::new("test");
        state.set_explicit_plan(vec!["Only step".into()], None);
        assert!(state.complete_step(5).is_err());
    }

    #[test]
    fn complete_step_without_plan_returns_error() {
        let mut state = ExecutionPlanState::new("test");
        assert!(state.complete_step(0).is_err());
    }

    #[test]
    fn runtime_message_renders_explicit_plan_not_constraints() {
        let mut state = ExecutionPlanState::new("do X and Y");
        // Would normally infer "Complete all sub-requests" constraint
        state.set_explicit_plan(vec!["Do X".into(), "Do Y".into()], Some("Both done".into()));
        let content = state
            .runtime_message()
            .and_then(|msg| msg.content)
            .expect("content");

        // Should use explicit plan format
        assert!(content.contains("[CURRENT] Step 0: Do X"));
        assert!(content.contains("[TODO] Step 1: Do Y"));
        assert!(content.contains("Verification: Both done"));

        // Should NOT show inferred constraints
        assert!(!content.contains("Constraints to satisfy"));
    }

    #[test]
    fn runtime_message_nudges_verification_when_all_completed() {
        let mut state = ExecutionPlanState::new("quick task");
        state.set_explicit_plan(vec!["Do it".into()], Some("Confirm done".into()));
        state.complete_step(0).unwrap();

        let content = state
            .runtime_message()
            .and_then(|msg| msg.content)
            .expect("content");
        assert!(content.contains("All planned steps completed"));
        assert!(content.contains("Confirm done"));
    }

    #[test]
    fn snapshot_includes_explicit_steps() {
        let mut state = ExecutionPlanState::new("test");
        state.set_explicit_plan(vec!["A".into(), "B".into()], None);
        let snap = state.snapshot();
        assert_eq!(snap.explicit_steps.len(), 2);
        assert_eq!(snap.explicit_steps[0].status, "in_progress");
        assert_eq!(snap.explicit_steps[1].status, "pending");
    }

    #[test]
    fn backward_compat_no_explicit_plan_uses_constraints() {
        let state = ExecutionPlanState::new("compare Trenitalia and Italo");
        let content = state
            .runtime_message()
            .and_then(|msg| msg.content)
            .expect("content");
        assert!(content.contains("Constraints to satisfy"));

        let snap = state.snapshot();
        assert!(snap.explicit_steps.is_empty());
        assert!(snap.verification.is_none());
    }

    #[test]
    fn infer_brand_sources_from_prompt() {
        let state =
            ExecutionPlanState::new("mi trovi delle scarpe di pelle marrone di prada o di gucci");
        let content = state
            .runtime_message()
            .and_then(|msg| msg.content)
            .expect("content");
        assert!(content.contains("prada"));
        assert!(content.contains("gucci"));
    }

    #[test]
    fn error_page_creates_blocker() {
        use super::infer_blockers;
        let blockers = infer_blockers(
            "browser",
            "⚠ This appears to be an error page (404).\nTry the homepage.",
            false,
        );
        assert!(blockers.iter().any(|b| b.contains("error page")));
    }
}
