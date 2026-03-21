//! Task planner — LLM-based task decomposition.
//!
//! Given an intent analysis and user prompt, generates a `TaskPlan`
//! with atomic subtasks and their dependency graph.

use anyhow::Result;

use crate::config::Config;
use crate::provider::one_shot::{llm_one_shot, OneShotRequest};

use super::types::{IntentAnalysis, Subtask, SubtaskStatus, TaskPlan, MAX_SUBTASKS};

/// Maximum tokens for the planning response.
const PLAN_MAX_TOKENS: u32 = 1536;

/// Timeout for planning call (seconds).
///
/// Needs to be generous — cloud models with reasoning (e.g. minimax-m2.7)
/// can take 15-25s for complex plan generation.
const PLAN_TIMEOUT_SECS: u64 = 30;

const SYSTEM_PROMPT: &str = r#"You are a task planner for an AI assistant that can browse the web, search, and interact with websites.

Given a user request and its intent analysis, decompose it into atomic subtasks that can be executed independently.

Reply with JSON only. No markdown, no explanation.

Schema:
{
  "objective": "brief description of the overall goal",
  "subtasks": [
    {"id": "t0", "description": "what this does", "prompt": "detailed instructions for the agent", "depends_on": [], "agent_id": ""},
    ...
  ],
  "verification": "how to verify the final result is complete"
}

Rules:
- Maximum 6 subtasks.
- Use "depends_on" to express which subtasks must complete first (by ID).
- Subtasks with no dependencies can run IN PARALLEL — design for this.
- The first subtask is usually a Google search to find relevant sources.
- For multi-source research: create one subtask per site/source so they run in parallel.
- Include a final synthesis subtask that depends on all data-gathering subtasks.
- Each subtask prompt must be self-contained — the agent executing it has NO context beyond what you write.
- CRITICAL: Never put constructed/guessed URLs in subtask prompts. Always instruct to "search Google for X" or "use the site's search form".
- For browser tasks, instruct: "Navigate to the site from Google results. Use the site's search form to find [query]. Extract: prices, links, images, contact info, descriptions. Check for pagination."
- agent_id is usually empty (uses default agent). Only set it if a specialized agent is needed."#;

/// Generate a task plan from the intent analysis and original prompt.
///
/// Uses `llm_one_shot` with the primary model. Falls back to a single-subtask
/// plan on parse errors (effectively a passthrough with enriched instructions).
pub async fn plan(
    config: &Config,
    user_prompt: &str,
    intent: &IntentAnalysis,
) -> Result<TaskPlan> {
    let user_message = format!(
        "Intent analysis:\n{}\n\nUser request:\n{}",
        serde_json::to_string_pretty(intent).unwrap_or_default(),
        user_prompt
    );

    let req = OneShotRequest {
        system_prompt: SYSTEM_PROMPT.to_string(),
        user_message,
        max_tokens: PLAN_MAX_TOKENS,
        temperature: 0.3,
        timeout_secs: PLAN_TIMEOUT_SECS,
        ..Default::default()
    };

    let response = llm_one_shot(config, req).await?;
    let plan = parse_plan(&response.content, user_prompt)?;

    tracing::info!(
        subtasks = plan.subtasks.len(),
        latency_ms = response.latency.as_millis() as u64,
        model = %response.model,
        "Task plan created"
    );

    Ok(plan)
}

/// Parse the LLM response into a TaskPlan, with fallback to single-subtask.
fn parse_plan(raw: &str, user_prompt: &str) -> Result<TaskPlan> {
    let trimmed = raw.trim();

    // Strip markdown code fences if present.
    let json_str = if trimmed.starts_with("```") {
        trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```"))
            .and_then(|s| s.strip_suffix("```"))
            .unwrap_or(trimmed)
            .trim()
    } else {
        trimmed
    };

    match serde_json::from_str::<TaskPlan>(json_str) {
        Ok(mut plan) => {
            // Enforce max subtasks.
            plan.subtasks.truncate(MAX_SUBTASKS);
            // Ensure all subtasks have default status.
            for task in &mut plan.subtasks {
                task.status = SubtaskStatus::Pending;
                task.result = None;
            }
            // Validate dependency references.
            let ids: Vec<String> = plan.subtasks.iter().map(|t| t.id.clone()).collect();
            for task in &mut plan.subtasks {
                task.depends_on.retain(|dep| ids.contains(dep));
            }
            Ok(plan)
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to parse task plan JSON, creating single-task fallback");
            Ok(single_task_fallback(user_prompt))
        }
    }
}

/// Create a single-subtask plan as fallback when planning fails.
fn single_task_fallback(user_prompt: &str) -> TaskPlan {
    TaskPlan {
        objective: crate::utils::text::truncate_str(user_prompt.trim(), 220, "").to_string(),
        subtasks: vec![Subtask {
            id: "t0".to_string(),
            description: "Execute user request directly".to_string(),
            prompt: user_prompt.to_string(),
            depends_on: vec![],
            agent_id: String::new(),
            status: SubtaskStatus::Pending,
            result: None,
        }],
        verification: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_plan() {
        let json = r#"{
            "objective": "Find moto guzzi v7 special listings",
            "subtasks": [
                {"id": "t0", "description": "Google search", "prompt": "Search Google for...", "depends_on": []},
                {"id": "t1", "description": "Visit subito.it", "prompt": "Go to subito.it...", "depends_on": ["t0"]},
                {"id": "t2", "description": "Visit autoscout24", "prompt": "Go to autoscout24...", "depends_on": ["t0"]},
                {"id": "t3", "description": "Synthesize", "prompt": "Combine results...", "depends_on": ["t1", "t2"]}
            ],
            "verification": "At least 3 listings with prices"
        }"#;
        let plan = parse_plan(json, "test").unwrap();
        assert_eq!(plan.subtasks.len(), 4);
        assert_eq!(plan.subtasks[3].depends_on, vec!["t1", "t2"]);
    }

    #[test]
    fn truncates_to_max_subtasks() {
        let mut subtasks = Vec::new();
        for i in 0..10 {
            subtasks.push(serde_json::json!({
                "id": format!("t{i}"),
                "description": format!("Task {i}"),
                "prompt": format!("Do {i}"),
                "depends_on": []
            }));
        }
        let json = serde_json::json!({
            "objective": "test",
            "subtasks": subtasks,
        });
        let plan = parse_plan(&json.to_string(), "test").unwrap();
        assert_eq!(plan.subtasks.len(), MAX_SUBTASKS);
    }

    #[test]
    fn strips_invalid_dependencies() {
        let json = r#"{
            "objective": "test",
            "subtasks": [
                {"id": "t0", "description": "A", "prompt": "Do A", "depends_on": ["nonexistent"]}
            ]
        }"#;
        let plan = parse_plan(json, "test").unwrap();
        assert!(plan.subtasks[0].depends_on.is_empty());
    }

    #[test]
    fn falls_back_to_single_task_on_garbage() {
        let plan = parse_plan("not json at all", "do something cool").unwrap();
        assert_eq!(plan.subtasks.len(), 1);
        assert_eq!(plan.subtasks[0].id, "t0");
        assert_eq!(plan.subtasks[0].prompt, "do something cool");
    }
}
