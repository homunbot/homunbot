use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::provider::ChatMessage;

const MAX_PLAN_ITEMS: usize = 6;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionPlanSnapshot {
    pub objective: String,
    pub constraints: Vec<String>,
    pub completed_steps: Vec<String>,
    pub active_blockers: Vec<String>,
    pub required_sources: Vec<String>,
    pub completed_sources: Vec<String>,
    pub current_source: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct ExecutionPlanState {
    objective: String,
    constraints: Vec<String>,
    completed_steps: Vec<String>,
    active_blockers: Vec<String>,
    seen_step_signatures: HashSet<String>,
}

impl ExecutionPlanState {
    pub fn new(user_prompt: &str) -> Self {
        Self {
            objective: compact(user_prompt, 220),
            constraints: infer_constraints(user_prompt),
            completed_steps: Vec::new(),
            active_blockers: Vec::new(),
            seen_step_signatures: HashSet::new(),
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

    pub fn runtime_message(&self) -> Option<ChatMessage> {
        if self.objective.is_empty()
            && self.constraints.is_empty()
            && self.completed_steps.is_empty()
            && self.active_blockers.is_empty()
        {
            return None;
        }

        let mut lines = Vec::new();
        if !self.objective.is_empty() {
            lines.push(format!("Execution objective: {}", self.objective));
        }
        if !self.constraints.is_empty() {
            lines.push("Constraints to satisfy before finishing:".to_string());
            for item in &self.constraints {
                lines.push(format!("- {}", item));
            }
        }
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

    pub fn snapshot(&self) -> ExecutionPlanSnapshot {
        ExecutionPlanSnapshot {
            objective: self.objective.clone(),
            constraints: self.constraints.clone(),
            completed_steps: self.completed_steps.clone(),
            active_blockers: self.active_blockers.clone(),
            required_sources: Vec::new(),
            completed_sources: Vec::new(),
            current_source: None,
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
    sources.into_iter().map(str::to_string).collect()
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
        .filter(|part| !matches!(*part, "com" | "it" | "org" | "net" | "co"))
        .next()?;
    if source.is_empty() {
        None
    } else {
        Some(source.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionPlanState;

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
}
