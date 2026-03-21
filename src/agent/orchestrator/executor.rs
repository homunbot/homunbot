//! Task executor — runs subtasks from a `TaskPlan`.
//!
//! Walks the dependency DAG, executing independent subtasks in parallel
//! via `SubagentManager` and sequential ones on the primary agent loop.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::agent::AgentLoop;
use crate::config::Config;
use crate::provider::StreamChunk;

use super::types::{SubtaskResult, SubtaskStatus, TaskPlan};

/// Maximum retries per subtask on failure.
const MAX_RETRIES: u8 = 1;

/// Execute all subtasks in a plan, respecting dependency order.
///
/// Independent subtasks run in parallel via `SubagentManager`.
/// Returns results for all subtasks (including failed ones).
#[allow(clippy::too_many_arguments)]
pub async fn execute(
    mut plan: TaskPlan,
    agent: &Arc<AgentLoop>,
    _config: &Config,
    original_prompt: &str,
    session_key: &str,
    channel: &str,
    chat_id: &str,
    stream_tx: Option<&mpsc::Sender<StreamChunk>>,
) -> Vec<SubtaskResult> {
    let mut results: HashMap<String, SubtaskResult> = HashMap::new();

    loop {
        // Find subtasks whose dependencies are all satisfied.
        let ready: Vec<usize> = plan
            .subtasks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                t.status == SubtaskStatus::Pending
                    && t.depends_on
                        .iter()
                        .all(|dep| results.contains_key(dep))
            })
            .map(|(i, _)| i)
            .collect();

        if ready.is_empty() {
            break;
        }

        // Build context from completed dependencies for each ready task.
        let mut handles: Vec<(usize, tokio::task::JoinHandle<SubtaskResult>)> = Vec::new();

        for &idx in &ready {
            plan.subtasks[idx].status = SubtaskStatus::Running;

            let prompt = build_subtask_prompt(
                &plan.subtasks[idx].prompt,
                &plan.subtasks[idx].depends_on,
                &results,
                original_prompt,
            );

            let subtask_id = plan.subtasks[idx].id.clone();
            let subtask_desc = plan.subtasks[idx].description.clone();

            if ready.len() == 1 {
                // Single ready task: run on primary agent loop (preserves session context).
                let sub_session = format!("{session_key}:orch:{subtask_id}");
                let result = execute_single(
                    agent,
                    &prompt,
                    &sub_session,
                    channel,
                    chat_id,
                    &subtask_desc,
                )
                .await;
                results.insert(subtask_id.clone(), result.clone());
                plan.subtasks[idx].result = Some(result.clone());
                plan.subtasks[idx].status = if result.success {
                    SubtaskStatus::Completed
                } else {
                    SubtaskStatus::Failed {
                        reason: result.output.clone(),
                    }
                };

                tracing::info!(
                    subtask = %subtask_id,
                    success = result.success,
                    output_len = result.output.len(),
                    "Subtask completed (sequential)"
                );

                emit_step_progress(stream_tx, &plan).await;
            } else {
                // Multiple ready tasks: run in parallel via tokio::spawn.
                let agent_clone = agent.clone();
                let sub_session = format!("{session_key}:orch:{subtask_id}");
                let ch = channel.to_string();
                let cid = chat_id.to_string();
                let desc = subtask_desc.clone();
                let sid = subtask_id.clone();

                let handle = tokio::spawn(async move {
                    execute_single(&agent_clone, &prompt, &sub_session, &ch, &cid, &desc).await
                });
                handles.push((idx, handle));

                tracing::info!(
                    subtask = %sid,
                    "Subtask spawned (parallel)"
                );
            }
        }

        // Collect parallel results.
        let had_parallel = !handles.is_empty();
        for (idx, handle) in handles {
            let subtask_id = plan.subtasks[idx].id.clone();
            match handle.await {
                Ok(result) => {
                    plan.subtasks[idx].result = Some(result.clone());
                    plan.subtasks[idx].status = if result.success {
                        SubtaskStatus::Completed
                    } else {
                        SubtaskStatus::Failed {
                            reason: result.output.clone(),
                        }
                    };
                    tracing::info!(
                        subtask = %subtask_id,
                        success = result.success,
                        output_len = result.output.len(),
                        "Subtask completed (parallel)"
                    );
                    results.insert(subtask_id, result);
                }
                Err(e) => {
                    let result = SubtaskResult {
                        output: format!("Subtask panicked: {e}"),
                        success: false,
                    };
                    plan.subtasks[idx].status = SubtaskStatus::Failed {
                        reason: result.output.clone(),
                    };
                    plan.subtasks[idx].result = Some(result.clone());
                    results.insert(subtask_id, result);
                }
            }
        }

        // Emit progress after parallel batch completes.
        if had_parallel {
            emit_step_progress(stream_tx, &plan).await;
        }

        // Check for stuck state: if no progress was made, break.
        let any_running = plan
            .subtasks
            .iter()
            .any(|t| t.status == SubtaskStatus::Running);
        if !any_running
            && plan
                .subtasks
                .iter()
                .all(|t| t.status != SubtaskStatus::Pending)
        {
            break;
        }
    }

    // Return results in subtask order.
    plan.subtasks
        .iter()
        .map(|t| {
            t.result.clone().unwrap_or(SubtaskResult {
                output: "Subtask was not executed (dependency failed)".to_string(),
                success: false,
            })
        })
        .collect()
}

/// Execute a single subtask on an agent loop.
async fn execute_single(
    agent: &Arc<AgentLoop>,
    prompt: &str,
    session_key: &str,
    channel: &str,
    chat_id: &str,
    description: &str,
) -> SubtaskResult {
    let mut retries = 0u8;
    let mut last_error = String::new();

    while retries <= MAX_RETRIES {
        let message = if retries == 0 {
            prompt.to_string()
        } else {
            format!(
                "{}\n\n[Previous attempt failed: {}. Try a different approach.]",
                prompt, last_error
            )
        };

        match agent
            .process_message(&message, session_key, channel, chat_id)
            .await
        {
            Ok(output) => {
                return SubtaskResult {
                    output,
                    success: true,
                };
            }
            Err(e) => {
                last_error = e.to_string();
                tracing::warn!(
                    description,
                    retry = retries,
                    error = %e,
                    "Subtask execution failed"
                );
                retries += 1;
            }
        }
    }

    SubtaskResult {
        output: format!("Failed after {} retries: {}", MAX_RETRIES + 1, last_error),
        success: false,
    }
}

/// Build the full prompt for a subtask, injecting dependency results.
fn build_subtask_prompt(
    subtask_prompt: &str,
    depends_on: &[String],
    completed: &HashMap<String, SubtaskResult>,
    original_prompt: &str,
) -> String {
    let mut parts = Vec::new();

    parts.push(format!(
        "You are executing a subtask of a larger request.\n\nOriginal user request: {original_prompt}"
    ));

    // Inject results from dependencies.
    if !depends_on.is_empty() {
        parts.push("Results from previous subtasks:".to_string());
        for dep_id in depends_on {
            if let Some(result) = completed.get(dep_id) {
                let status = if result.success { "SUCCESS" } else { "FAILED" };
                parts.push(format!(
                    "[{dep_id}] ({status}):\n{}",
                    crate::utils::text::truncate_str(&result.output, 4000, "\n[...truncated]")
                ));
            }
        }
    }

    parts.push(format!("Your specific task:\n{subtask_prompt}"));

    // Browser research rules — always include for subtask agents.
    parts.push(
        "IMPORTANT rules:\n\
         - NEVER navigate to URLs you constructed or guessed. Only use URLs from search results or links on the current page.\n\
         - On each website, use the site's SEARCH FORM to find what you need.\n\
         - Extract structured data: links, prices, images, contact info, descriptions.\n\
         - Check for pagination (next page) on result pages."
            .to_string(),
    );

    parts.join("\n\n")
}

/// Emit current step progress to the streaming UI.
async fn emit_step_progress(
    stream_tx: Option<&mpsc::Sender<StreamChunk>>,
    plan: &TaskPlan,
) {
    let Some(tx) = stream_tx else {
        return;
    };

    use crate::agent::execution_plan::{ExecutionPlanSnapshot, PlanStepSnapshot};

    let steps: Vec<PlanStepSnapshot> = plan
        .subtasks
        .iter()
        .map(|t| {
            let status = match &t.status {
                SubtaskStatus::Pending => "pending",
                SubtaskStatus::Running => "in_progress",
                SubtaskStatus::Completed => "completed",
                SubtaskStatus::Failed { .. } => "completed",
            };
            PlanStepSnapshot {
                description: t.description.clone(),
                status: status.to_string(),
            }
        })
        .collect();

    let snapshot = ExecutionPlanSnapshot {
        objective: plan.objective.clone(),
        explicit_steps: steps,
        phase: "executing".to_string(),
        ..Default::default()
    };
    let Ok(payload) = serde_json::to_string(&snapshot) else {
        return;
    };
    let _ = tx
        .send(StreamChunk {
            delta: payload,
            done: false,
            event_type: Some("plan".to_string()),
            tool_call_data: None,
        })
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_prompt_includes_dependency_results() {
        let mut completed = HashMap::new();
        completed.insert(
            "t0".to_string(),
            SubtaskResult {
                output: "Found 3 sites: A, B, C".to_string(),
                success: true,
            },
        );

        let prompt = build_subtask_prompt(
            "Visit site A and extract listings",
            &["t0".to_string()],
            &completed,
            "find moto guzzi v7",
        );

        assert!(prompt.contains("Original user request: find moto guzzi v7"));
        assert!(prompt.contains("[t0] (SUCCESS)"));
        assert!(prompt.contains("Found 3 sites: A, B, C"));
        assert!(prompt.contains("Visit site A and extract listings"));
        assert!(prompt.contains("NEVER navigate to URLs you constructed"));
    }

    #[test]
    fn build_prompt_without_dependencies() {
        let completed = HashMap::new();
        let prompt = build_subtask_prompt("Search Google", &[], &completed, "find stuff");

        assert!(prompt.contains("Original user request: find stuff"));
        assert!(!prompt.contains("Results from previous subtasks"));
        assert!(prompt.contains("Search Google"));
    }
}
