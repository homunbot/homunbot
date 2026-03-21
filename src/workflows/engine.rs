//! Workflow orchestrator — manages multi-step execution lifecycle.
//!
//! The engine:
//! 1. Creates workflows and persists them to SQLite
//! 2. Executes steps sequentially via `AgentLoop::process_message()`
//! 3. Passes inter-step context (previous results) to each step
//! 4. Pauses at approval gates and resumes on user confirmation
//! 5. Retries failed steps up to `max_retries`
//! 6. Resumes interrupted workflows on gateway restart

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{bail, Result};
use crate::utils::text::truncate_str;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use crate::agent::AgentRegistry;
use crate::storage::Database;

use super::{
    StepStatus, Workflow, WorkflowCreateRequest, WorkflowEvent, WorkflowStatus, WorkflowStep,
};

/// Workflow engine — orchestrates persistent multi-step autonomous tasks.
pub struct WorkflowEngine {
    db: Database,
    /// Agent registry for per-step agent lookup (MAG-4).
    registry: Arc<AgentRegistry>,
    /// Tracks running workflow tasks: workflow_id → JoinHandle
    running: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
    /// Channel for sending events to the gateway (notifications, approvals)
    event_tx: mpsc::Sender<WorkflowEvent>,
}

impl WorkflowEngine {
    pub fn new(
        db: Database,
        registry: Arc<AgentRegistry>,
        event_tx: mpsc::Sender<WorkflowEvent>,
    ) -> Self {
        Self {
            db,
            registry,
            running: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    /// Create a new workflow and immediately start executing it.
    pub async fn create_and_start(
        &self,
        req: WorkflowCreateRequest,
        channel: &str,
        chat_id: &str,
    ) -> Result<String> {
        if req.steps.is_empty() {
            bail!("Workflow must have at least one step");
        }
        if req.steps.len() > 20 {
            bail!("Workflow cannot have more than 20 steps");
        }

        let workflow_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let created_by = format!("{channel}:{chat_id}");
        self.db
            .insert_workflow(&workflow_id, &req, Some(&created_by))
            .await?;

        tracing::info!(
            workflow_id = %workflow_id,
            name = %req.name,
            steps = req.steps.len(),
            "Workflow created"
        );

        self.start_workflow(&workflow_id).await?;
        Ok(workflow_id)
    }

    /// Start or resume a workflow from its current step.
    async fn start_workflow(&self, workflow_id: &str) -> Result<()> {
        let workflow = self
            .db
            .load_workflow(workflow_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Workflow {workflow_id} not found"))?;

        if workflow.status.is_terminal() {
            bail!(
                "Workflow {} is already {}",
                workflow_id,
                workflow.status.as_str()
            );
        }

        // Mark as running
        self.db
            .update_workflow_status(workflow_id, WorkflowStatus::Running, None)
            .await?;

        let db = self.db.clone();
        let registry = self.registry.clone();
        let event_tx = self.event_tx.clone();
        let wf_id = workflow_id.to_string();
        let running = self.running.clone();

        let handle = tokio::spawn(async move {
            if let Err(e) = run_workflow_loop(db.clone(), registry, event_tx.clone(), &wf_id).await
            {
                tracing::error!(workflow_id = %wf_id, error = %e, "Workflow execution failed");
                let _ = db
                    .update_workflow_status(&wf_id, WorkflowStatus::Failed, Some(&e.to_string()))
                    .await;
                let _ = db.cancel_pending_steps(&wf_id).await;

                // Load workflow for notification + automation run completion
                if let Ok(Some(wf)) = db.load_workflow(&wf_id).await {
                    // Complete parent automation run with error
                    if let (Some(auto_id), Some(run_id)) =
                        (&wf.automation_id, &wf.automation_run_id)
                    {
                        let _ = crate::scheduler::automations::evaluate_and_complete_automation_run(
                            &db, auto_id, run_id, &e.to_string(), true,
                        )
                        .await;
                    }

                    let total = wf.steps.len();
                    let step = wf.current_step_idx;
                    let _ = event_tx
                        .send(WorkflowEvent::WorkflowFailed {
                            workflow_id: wf_id.clone(),
                            workflow_name: wf.name,
                            step_idx: step,
                            total_steps: total,
                            error: e.to_string(),
                            deliver_to: wf.deliver_to,
                        })
                        .await;
                }
            }

            // Remove from running map
            running.lock().await.remove(&wf_id);
        });

        self.running
            .lock()
            .await
            .insert(workflow_id.to_string(), handle);

        Ok(())
    }

    /// Resume a paused workflow (after approval).
    pub async fn approve_and_resume(&self, workflow_id: &str) -> Result<String> {
        let workflow = self
            .db
            .load_workflow(workflow_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Workflow {workflow_id} not found"))?;

        if workflow.status != WorkflowStatus::Paused {
            bail!(
                "Workflow {} is not paused (current status: {})",
                workflow_id,
                workflow.status.as_str()
            );
        }

        let step_name = workflow
            .steps
            .get(workflow.current_step_idx)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "unknown".to_string());

        self.start_workflow(workflow_id).await?;
        Ok(format!(
            "Workflow \"{}\" resumed from step {}: {}",
            workflow.name, workflow.current_step_idx, step_name
        ))
    }

    /// Cancel a workflow.
    pub async fn cancel(&self, workflow_id: &str) -> Result<String> {
        let workflow = self
            .db
            .load_workflow(workflow_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Workflow {workflow_id} not found"))?;

        if workflow.status.is_terminal() {
            bail!(
                "Workflow {} is already {}",
                workflow_id,
                workflow.status.as_str()
            );
        }

        // Abort running task if any
        if let Some(handle) = self.running.lock().await.remove(workflow_id) {
            handle.abort();
        }

        self.db
            .update_workflow_status(workflow_id, WorkflowStatus::Cancelled, None)
            .await?;
        self.db.cancel_pending_steps(workflow_id).await?;

        Ok(format!("Workflow \"{}\" cancelled", workflow.name))
    }

    /// Delete a terminal workflow.
    pub async fn delete(&self, workflow_id: &str) -> Result<String> {
        let workflow = self
            .db
            .load_workflow(workflow_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Workflow {workflow_id} not found"))?;

        if !workflow.status.is_terminal() {
            bail!(
                "Cannot delete a {} workflow — cancel it first",
                workflow.status.as_str()
            );
        }

        self.db.delete_workflow(workflow_id).await?;
        Ok(format!("Workflow \"{}\" deleted", workflow.name))
    }

    /// Restart a terminal workflow (creates a fresh copy and starts it).
    pub async fn restart(&self, workflow_id: &str) -> Result<String> {
        let workflow = self
            .db
            .load_workflow(workflow_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Workflow {workflow_id} not found"))?;

        if !workflow.status.is_terminal() {
            bail!(
                "Cannot restart a {} workflow — it's still active",
                workflow.status.as_str()
            );
        }

        let req = WorkflowCreateRequest {
            name: workflow.name.clone(),
            objective: workflow.objective.clone(),
            steps: workflow
                .steps
                .iter()
                .map(|s| super::StepDefinition {
                    name: s.name.clone(),
                    instruction: s.instruction.clone(),
                    approval_required: s.approval_required,
                    max_retries: s.max_retries,
                    agent_id: Some(s.agent_id.clone()),
                })
                .collect(),
            deliver_to: workflow.deliver_to.clone(),
            automation_id: workflow.automation_id.clone(),
            automation_run_id: None, // restarted workflows don't link to a specific run
        };

        let channel_chat = workflow.created_by.as_deref().unwrap_or("web:web");
        let (channel, chat_id) = channel_chat.rsplit_once(':').unwrap_or(("web", "web"));

        let new_id = self.create_and_start(req, channel, chat_id).await?;
        Ok(format!(
            "Workflow \"{}\" restarted as {new_id}",
            workflow.name
        ))
    }

    /// List workflows (optionally filtered by status).
    pub async fn list(&self, status_filter: Option<&str>) -> Result<Vec<Workflow>> {
        self.db.list_workflows(status_filter).await
    }

    /// Get a single workflow's status.
    pub async fn status(&self, workflow_id: &str) -> Result<Option<Workflow>> {
        self.db.load_workflow(workflow_id).await
    }

    /// Resume any workflows that were running when the process stopped.
    pub async fn resume_on_startup(&self) -> Result<usize> {
        let resumable = self.db.load_resumable_workflows().await?;
        let count = resumable.len();

        for wf in resumable {
            tracing::info!(
                workflow_id = %wf.id,
                name = %wf.name,
                step = wf.current_step_idx,
                "Resuming workflow from previous session"
            );
            if let Err(e) = self.start_workflow(&wf.id).await {
                tracing::error!(
                    workflow_id = %wf.id,
                    error = %e,
                    "Failed to resume workflow"
                );
            }
        }

        if count > 0 {
            tracing::info!(count, "Resumed workflows from previous session");
        }
        Ok(count)
    }
}

/// Main execution loop for a single workflow.
/// Runs steps sequentially, handles retries and approval gates.
async fn run_workflow_loop(
    db: Database,
    registry: Arc<AgentRegistry>,
    event_tx: mpsc::Sender<WorkflowEvent>,
    workflow_id: &str,
) -> Result<()> {
    loop {
        // Reload workflow state (might have been updated by approval)
        let workflow = db
            .load_workflow(workflow_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Workflow disappeared"))?;

        if workflow.status.is_terminal() || workflow.status == WorkflowStatus::Cancelled {
            return Ok(());
        }

        let step_idx = workflow.current_step_idx;
        let Some(step) = workflow.steps.get(step_idx) else {
            // All steps exhausted — workflow complete
            return complete_workflow(&db, &event_tx, &workflow).await;
        };

        // Skip already completed steps (from resume)
        if step.status == StepStatus::Completed {
            db.update_workflow_step_idx(workflow_id, step_idx + 1)
                .await?;
            continue;
        }

        // Check approval gate
        if step.approval_required && step.status == StepStatus::Pending {
            tracing::info!(
                workflow_id = %workflow_id,
                step = step_idx,
                "Workflow paused for approval"
            );

            db.update_workflow_status(workflow_id, WorkflowStatus::Paused, None)
                .await?;

            let _ = event_tx
                .send(WorkflowEvent::ApprovalNeeded {
                    workflow_id: workflow_id.to_string(),
                    workflow_name: workflow.name.clone(),
                    step_idx,
                    total_steps: workflow.steps.len(),
                    step_name: step.name.clone(),
                    step_instruction: step.instruction.clone(),
                    deliver_to: workflow.deliver_to.clone(),
                })
                .await;

            // Exit loop — will be resumed by approve_and_resume()
            return Ok(());
        }

        // Execute the step
        let result = execute_step(&db, &registry, &event_tx, &workflow, step).await;

        match result {
            Ok(output) => {
                // Step succeeded — store result, update context, advance
                db.update_step_status(&step.id, StepStatus::Completed, Some(&output), None)
                    .await?;

                // Update shared context with step result
                let mut context = workflow.context.clone();
                if let serde_json::Value::Object(ref mut map) = context {
                    map.insert(
                        format!("step_{}", step_idx),
                        serde_json::json!({
                            "name": step.name,
                            "result": truncate_str(&output, 2000, "..."),
                        }),
                    );
                }
                db.update_workflow_context(workflow_id, &context).await?;

                // Notify step completion
                let _ = event_tx
                    .send(WorkflowEvent::StepCompleted {
                        workflow_id: workflow_id.to_string(),
                        workflow_name: workflow.name.clone(),
                        step_idx,
                        total_steps: workflow.steps.len(),
                        step_name: step.name.clone(),
                        result_summary: truncate_str(&output, 200, "..."),
                        deliver_to: workflow.deliver_to.clone(),
                    })
                    .await;

                // Advance to next step
                db.update_workflow_step_idx(workflow_id, step_idx + 1)
                    .await?;

                tracing::info!(
                    workflow_id = %workflow_id,
                    step = step_idx,
                    step_name = %step.name,
                    "Step completed"
                );
            }
            Err(e) => {
                let error_msg = e.to_string();
                tracing::warn!(
                    workflow_id = %workflow_id,
                    step = step_idx,
                    error = %error_msg,
                    retry = step.retry_count,
                    max = step.max_retries,
                    "Step failed"
                );

                if step.retry_count < step.max_retries {
                    // Retry
                    db.increment_step_retry(&step.id).await?;
                    db.update_step_status(&step.id, StepStatus::Pending, None, Some(&error_msg))
                        .await?;
                    // Loop will re-execute this step
                } else {
                    // Max retries exhausted — fail workflow
                    db.update_step_status(&step.id, StepStatus::Failed, None, Some(&error_msg))
                        .await?;
                    bail!(
                        "Step {} \"{}\" failed after {} retries: {}",
                        step_idx,
                        step.name,
                        step.max_retries,
                        error_msg
                    );
                }
            }
        }
    }
}

/// Execute a single workflow step via the agent loop.
async fn execute_step(
    db: &Database,
    registry: &AgentRegistry,
    event_tx: &mpsc::Sender<WorkflowEvent>,
    workflow: &Workflow,
    step: &WorkflowStep,
) -> Result<String> {
    // Mark step as running
    db.update_step_status(&step.id, StepStatus::Running, None, None)
        .await?;

    // Notify UI that step is starting
    let _ = event_tx
        .send(WorkflowEvent::StepStarted {
            workflow_id: workflow.id.clone(),
            workflow_name: workflow.name.clone(),
            step_idx: step.idx,
            total_steps: workflow.steps.len(),
            step_name: step.name.clone(),
            deliver_to: workflow.deliver_to.clone(),
        })
        .await;

    // Resolve agent for this step (MAG-4: per-step agent routing)
    let agent = registry
        .get(&step.agent_id)
        .unwrap_or_else(|| registry.default_agent());

    // Build prompt with tool guidance
    let tool_names = agent.registered_tool_names().await;
    let prompt = build_step_prompt(workflow, step, &tool_names);
    let session_key = format!("workflow:{}:step:{}", workflow.id, step.idx);

    tracing::debug!(
        workflow_id = %workflow.id,
        step_idx = step.idx,
        agent_id = %step.agent_id,
        "Executing workflow step with agent"
    );

    agent
        .process_message(&prompt, &session_key, "workflow", &workflow.id)
        .await
}

/// Build the prompt for a workflow step, including context from previous steps.
fn build_step_prompt(workflow: &Workflow, step: &WorkflowStep, tool_names: &[String]) -> String {
    let total = workflow.steps.len();
    let mut lines = Vec::new();

    lines.push(format!(
        "WORKFLOW EXECUTION — Step {}/{}: {}",
        step.idx, total, step.name
    ));
    lines.push(format!("Workflow: {}", workflow.name));
    lines.push(format!("Objective: {}", workflow.objective));
    lines.push(String::new());

    // Tool guidance: tell the model which tools to use proactively
    if !tool_names.is_empty() {
        lines.push("IMPORTANT: You MUST use your tools to complete this step. Do NOT just describe what you would do — actually DO it by calling tools.".to_string());

        // Highlight the most relevant tools for research tasks
        let has_web_search = tool_names.iter().any(|n| n == "web_search");
        let has_browser = tool_names.iter().any(|n| n.starts_with("browser"));
        let has_web_fetch = tool_names.iter().any(|n| n == "web_fetch");

        if has_web_search || has_browser || has_web_fetch {
            lines.push("Available research tools:".to_string());
            if has_web_search {
                lines.push("- web_search: search the web for information".to_string());
            }
            if has_web_fetch {
                lines.push("- web_fetch: read content from a specific URL".to_string());
            }
            if has_browser {
                lines.push(
                    "- browser: interact with websites (navigate, click, type, read)".to_string(),
                );
            }
        }
        lines.push(String::new());
    }

    // Include previous step results
    let completed: Vec<&WorkflowStep> = workflow
        .steps
        .iter()
        .filter(|s| s.status == StepStatus::Completed && s.idx < step.idx)
        .collect();

    if !completed.is_empty() {
        lines.push("Previous step results:".to_string());
        for s in completed {
            let result = s
                .result
                .as_deref()
                .map(|r| truncate_str(r, 500, "..."))
                .unwrap_or_else(|| "(no output)".to_string());
            lines.push(format!("- Step {} ({}): {}", s.idx, s.name, result));
        }
        lines.push(String::new());
    }

    lines.push("YOUR TASK FOR THIS STEP:".to_string());
    lines.push(step.instruction.clone());
    lines.push(String::new());
    lines.push(
        "After completing the task, provide your result clearly. \
         The result will be passed to subsequent steps."
            .to_string(),
    );

    lines.join("\n")
}

/// Mark workflow as completed and send final notification.
/// If this workflow was spawned by an automation, also completes the automation run.
async fn complete_workflow(
    db: &Database,
    event_tx: &mpsc::Sender<WorkflowEvent>,
    workflow: &Workflow,
) -> Result<()> {
    db.update_workflow_status(&workflow.id, WorkflowStatus::Completed, None)
        .await?;

    // Use the last completed step's result as the summary — that's the
    // final deliverable. Fall back to a short completion notice.
    let summary = workflow
        .steps
        .iter()
        .rev()
        .find(|s| s.status == StepStatus::Completed && s.result.is_some())
        .and_then(|s| s.result.clone())
        .unwrap_or_else(|| "Workflow completed successfully.".to_string());

    // Complete the parent automation run (if this workflow was spawned by one).
    // Uses the shared trigger evaluation so on_change/contains work for workflow automations.
    if let (Some(auto_id), Some(run_id)) =
        (&workflow.automation_id, &workflow.automation_run_id)
    {
        let _ = crate::scheduler::automations::evaluate_and_complete_automation_run(
            db, auto_id, run_id, &summary, false,
        )
        .await;
    }

    let _ = event_tx
        .send(WorkflowEvent::WorkflowCompleted {
            workflow_id: workflow.id.clone(),
            workflow_name: workflow.name.clone(),
            total_steps: workflow.steps.len(),
            summary,
            deliver_to: workflow.deliver_to.clone(),
        })
        .await;

    tracing::info!(
        workflow_id = %workflow.id,
        name = %workflow.name,
        "Workflow completed"
    );

    Ok(())
}

