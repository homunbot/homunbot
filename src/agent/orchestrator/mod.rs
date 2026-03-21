//! Task orchestrator — LLM-based intent analysis and task decomposition.
//!
//! Sits between gateway routing and the agent ReAct loop. Decides whether
//! a user request is simple (direct passthrough) or needs decomposition
//! into parallel subtasks.

pub mod types;

mod intent;
mod planner;
mod executor;
mod synthesizer;

pub use types::{IntentAnalysis, Subtask, SubtaskResult, SubtaskStatus, TaskComplexity, TaskPlan};

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::agent::execution_plan::{ExecutionPlanSnapshot, PlanStepSnapshot};
use crate::agent::AgentLoop;
use crate::config::Config;
use crate::provider::StreamChunk;

/// Top-level task orchestrator.
///
/// Entry point for all user messages after agent routing. Classifies intent
/// via LLM, then either passes through to the ReAct loop (simple) or
/// decomposes into subtasks (orchestrated).
pub struct TaskOrchestrator;

impl TaskOrchestrator {
    /// Handle an incoming user message with optional orchestration.
    ///
    /// For simple messages, this is a transparent passthrough to `process_message`.
    /// For complex multi-step tasks, it decomposes into subtasks, executes them
    /// (potentially in parallel), and synthesizes the final response.
    #[allow(clippy::too_many_arguments)]
    pub async fn handle(
        agent: &Arc<AgentLoop>,
        config: &Config,
        content: &str,
        session_key: &str,
        channel: &str,
        chat_id: &str,
        stream_tx: Option<mpsc::Sender<StreamChunk>>,
        blocked_tools: &[&str],
        thinking_override: Option<bool>,
    ) -> Result<String> {
        // Skip orchestration entirely if disabled.
        if intent::should_skip(config) {
            return passthrough(
                agent,
                content,
                session_key,
                channel,
                chat_id,
                stream_tx,
                blocked_tools,
                thinking_override,
            )
            .await;
        }

        // Step 1: Intent analysis.
        let analysis = intent::classify(config, content).await;

        // Step 2: Route based on complexity.
        match analysis.task_complexity() {
            TaskComplexity::Simple => {
                passthrough(
                    agent,
                    content,
                    session_key,
                    channel,
                    chat_id,
                    stream_tx,
                    blocked_tools,
                    thinking_override,
                )
                .await
            }
            TaskComplexity::Orchestrated => {
                orchestrate(
                    agent,
                    config,
                    content,
                    session_key,
                    channel,
                    chat_id,
                    stream_tx,
                    blocked_tools,
                    thinking_override,
                    &analysis,
                )
                .await
            }
        }
    }
}

/// Direct passthrough to the agent's ReAct loop — used for simple messages.
#[allow(clippy::too_many_arguments)]
async fn passthrough(
    agent: &Arc<AgentLoop>,
    content: &str,
    session_key: &str,
    channel: &str,
    chat_id: &str,
    stream_tx: Option<mpsc::Sender<StreamChunk>>,
    blocked_tools: &[&str],
    thinking_override: Option<bool>,
) -> Result<String> {
    if let Some(tx) = stream_tx {
        agent
            .process_message_streaming_with_options(
                content,
                session_key,
                channel,
                chat_id,
                tx,
                blocked_tools,
                thinking_override,
            )
            .await
    } else {
        agent
            .process_message_with_blocked_tools(
                content,
                session_key,
                channel,
                chat_id,
                blocked_tools,
            )
            .await
    }
}

/// Full orchestration path: plan → execute → synthesize.
#[allow(clippy::too_many_arguments)]
async fn orchestrate(
    agent: &Arc<AgentLoop>,
    config: &Config,
    content: &str,
    session_key: &str,
    channel: &str,
    chat_id: &str,
    stream_tx: Option<mpsc::Sender<StreamChunk>>,
    blocked_tools: &[&str],
    thinking_override: Option<bool>,
    analysis: &IntentAnalysis,
) -> Result<String> {
    // Emit "planning" phase to UI.
    emit_plan_snapshot(stream_tx.as_ref(), content, &[], "planning").await;

    // Step 1: Generate task plan.
    let plan = match planner::plan(config, content, analysis).await {
        Ok(plan) => plan,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "Task planner failed, falling back to direct execution"
            );
            return passthrough(
                agent,
                content,
                session_key,
                channel,
                chat_id,
                stream_tx,
                blocked_tools,
                thinking_override,
            )
            .await;
        }
    };

    tracing::info!(
        subtasks = plan.subtasks.len(),
        objective = %plan.objective,
        "Task plan generated"
    );

    // Emit plan with subtask list — first subtask marked as in_progress.
    let steps = plan_to_step_snapshots(&plan, None);
    emit_plan_snapshot(stream_tx.as_ref(), &plan.objective, &steps, "executing").await;

    // Step 2: Execute subtasks (executor emits per-step progress).
    let results = executor::execute(
        plan.clone(),
        agent,
        config,
        content,
        session_key,
        channel,
        chat_id,
        stream_tx.as_ref(),
    )
    .await;

    // Emit "synthesizing" phase — all steps should be completed/failed.
    let final_steps = plan_to_step_snapshots(&plan, Some(&results));
    emit_plan_snapshot(
        stream_tx.as_ref(),
        &plan.objective,
        &final_steps,
        "synthesizing",
    )
    .await;

    // Step 3: Synthesize results.
    match synthesizer::synthesize(config, content, &plan, &results).await {
        Ok(response) => Ok(response),
        Err(e) => {
            tracing::warn!(error = %e, "Synthesizer failed, returning raw results");
            // Fallback: concatenate raw subtask results.
            let fallback = results
                .iter()
                .filter(|r| r.success)
                .map(|r| r.output.as_str())
                .collect::<Vec<_>>()
                .join("\n\n---\n\n");
            if fallback.is_empty() {
                anyhow::bail!("All subtasks failed and synthesizer also failed: {e}");
            }
            Ok(fallback)
        }
    }
}

/// Build `PlanStepSnapshot`s from a `TaskPlan`, optionally incorporating results.
fn plan_to_step_snapshots(
    plan: &TaskPlan,
    results: Option<&[SubtaskResult]>,
) -> Vec<PlanStepSnapshot> {
    plan.subtasks
        .iter()
        .enumerate()
        .map(|(i, subtask)| {
            let status = match results {
                Some(res) if i < res.len() => {
                    if res[i].success {
                        "completed"
                    } else {
                        "completed" // failed subtasks still count as "done" in the UI
                    }
                }
                None if i == 0 => "in_progress",
                _ => "pending",
            };
            PlanStepSnapshot {
                description: subtask.description.clone(),
                status: status.to_string(),
            }
        })
        .collect()
}

/// Emit an orchestrator plan snapshot to the streaming UI.
async fn emit_plan_snapshot(
    stream_tx: Option<&mpsc::Sender<StreamChunk>>,
    objective: &str,
    steps: &[PlanStepSnapshot],
    phase: &str,
) {
    let Some(tx) = stream_tx else {
        return;
    };
    let snapshot = ExecutionPlanSnapshot {
        objective: objective.to_string(),
        explicit_steps: steps.to_vec(),
        phase: phase.to_string(),
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
