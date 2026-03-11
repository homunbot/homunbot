//! LLM tool for creating and managing multi-step workflows.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use super::registry::{Tool, ToolContext, ToolResult};
use crate::workflows::engine::WorkflowEngine;
use crate::workflows::WorkflowCreateRequest;

/// Workflow tool — lets the agent create and manage persistent multi-step tasks.
///
/// Uses `OnceCell` for late binding (same pattern as SpawnTool) because
/// WorkflowEngine depends on Arc<AgentLoop> which is created after the tool registry.
pub struct WorkflowTool {
    engine: Arc<tokio::sync::OnceCell<Arc<WorkflowEngine>>>,
}

impl WorkflowTool {
    pub fn new(engine: Arc<tokio::sync::OnceCell<Arc<WorkflowEngine>>>) -> Self {
        Self { engine }
    }

    fn get_engine(&self) -> Result<&WorkflowEngine> {
        self.engine
            .get()
            .map(|e| e.as_ref())
            .ok_or_else(|| anyhow::anyhow!("Workflow engine not initialized yet"))
    }
}

#[async_trait]
impl Tool for WorkflowTool {
    fn name(&self) -> &str {
        "workflow"
    }

    fn description(&self) -> &str {
        "Create and manage multi-step autonomous workflows. Use for complex tasks that require \
         a chain of actions (research → analyze → decide → execute). Each step runs independently \
         with its own agent session, and results are passed between steps automatically. \
         Steps can require human approval before proceeding."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "list", "status", "approve", "cancel"],
                    "description": "Action to perform"
                },
                "name": {
                    "type": "string",
                    "description": "Workflow name (for action=create)"
                },
                "objective": {
                    "type": "string",
                    "description": "High-level goal of the workflow (for action=create)"
                },
                "steps": {
                    "type": "array",
                    "description": "Array of step definitions (for action=create)",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {
                                "type": "string",
                                "description": "Short step name"
                            },
                            "instruction": {
                                "type": "string",
                                "description": "Detailed task instructions for this step"
                            },
                            "approval_required": {
                                "type": "boolean",
                                "description": "Whether to pause for human approval before this step"
                            },
                            "max_retries": {
                                "type": "integer",
                                "description": "Max retry attempts on failure (default: 1)"
                            }
                        },
                        "required": ["name", "instruction"]
                    }
                },
                "deliver_to": {
                    "type": "string",
                    "description": "Channel:chat_id for progress notifications (e.g. 'telegram:123456'). Defaults to current channel."
                },
                "workflow_id": {
                    "type": "string",
                    "description": "Workflow ID (for action=status/approve/cancel)"
                },
                "filter": {
                    "type": "string",
                    "enum": ["all", "active", "completed"],
                    "description": "Status filter (for action=list). Default: all"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");

        let engine = match self.get_engine() {
            Ok(e) => e,
            Err(e) => return Ok(ToolResult::error(format!("{e}"))),
        };

        match action {
            "create" => self.handle_create(engine, &args, ctx).await,
            "list" => self.handle_list(engine, &args).await,
            "status" => self.handle_status(engine, &args).await,
            "approve" => self.handle_approve(engine, &args).await,
            "cancel" => self.handle_cancel(engine, &args).await,
            other => Ok(ToolResult::error(format!(
                "Unknown action: {other}. Use create, list, status, approve, or cancel."
            ))),
        }
    }
}

impl WorkflowTool {
    async fn handle_create(
        &self,
        engine: &WorkflowEngine,
        args: &Value,
        ctx: &ToolContext,
    ) -> Result<ToolResult> {
        let name = match args.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => return Ok(ToolResult::error("Missing required field: name")),
        };
        let objective = match args.get("objective").and_then(|v| v.as_str()) {
            Some(o) => o.to_string(),
            None => return Ok(ToolResult::error("Missing required field: objective")),
        };
        let steps_val = match args.get("steps") {
            Some(s) => s,
            None => return Ok(ToolResult::error("Missing required field: steps")),
        };

        let steps: Vec<crate::workflows::StepDefinition> =
            match serde_json::from_value(steps_val.clone()) {
                Ok(s) => s,
                Err(e) => {
                    return Ok(ToolResult::error(format!(
                        "Invalid steps format: {e}. Each step needs 'name' and 'instruction'."
                    )))
                }
            };

        // Default deliver_to to current channel
        let deliver_to = args
            .get("deliver_to")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| Some(format!("{}:{}", ctx.channel, ctx.chat_id)));

        let req = WorkflowCreateRequest {
            name: name.clone(),
            objective,
            steps,
            deliver_to,
        };

        match engine
            .create_and_start(req, &ctx.channel, &ctx.chat_id)
            .await
        {
            Ok(id) => Ok(ToolResult::success(format!(
                "Workflow \"{name}\" created (id: {id}) and execution started. \
                 Progress notifications will be sent to the delivery channel."
            ))),
            Err(e) => Ok(ToolResult::error(format!("Failed to create workflow: {e}"))),
        }
    }

    async fn handle_list(&self, engine: &WorkflowEngine, args: &Value) -> Result<ToolResult> {
        let filter = args.get("filter").and_then(|v| v.as_str()).unwrap_or("all");

        let status_filter = match filter {
            "active" => Some("running"),
            "completed" => Some("completed"),
            _ => None,
        };

        let workflows = match engine.list(status_filter).await {
            Ok(w) => w,
            Err(e) => return Ok(ToolResult::error(format!("Failed to list workflows: {e}"))),
        };

        if workflows.is_empty() {
            return Ok(ToolResult::success("No workflows found."));
        }

        let mut lines = Vec::new();
        for wf in &workflows {
            let completed = wf
                .steps
                .iter()
                .filter(|s| s.status == crate::workflows::StepStatus::Completed)
                .count();
            let total = wf.steps.len();
            lines.push(format!(
                "- [{}] {} (id: {}) — {}/{} steps, status: {}",
                wf.id,
                wf.name,
                wf.id,
                completed,
                total,
                wf.status.as_str()
            ));
        }

        Ok(ToolResult::success(lines.join("\n")))
    }

    async fn handle_status(&self, engine: &WorkflowEngine, args: &Value) -> Result<ToolResult> {
        let id = match args.get("workflow_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required field: workflow_id")),
        };

        let workflow = match engine.status(id).await {
            Ok(Some(wf)) => wf,
            Ok(None) => return Ok(ToolResult::error(format!("Workflow {id} not found"))),
            Err(e) => return Ok(ToolResult::error(format!("Failed to load workflow: {e}"))),
        };

        let mut lines = Vec::new();
        lines.push(format!("Workflow: {} (id: {})", workflow.name, workflow.id));
        lines.push(format!("Status: {}", workflow.status.as_str()));
        lines.push(format!("Objective: {}", workflow.objective));
        lines.push(format!("Created: {}", workflow.created_at));
        if let Some(ref err) = workflow.error {
            lines.push(format!("Error: {err}"));
        }
        lines.push(String::new());
        lines.push("Steps:".to_string());

        for step in &workflow.steps {
            let status_icon = match step.status {
                crate::workflows::StepStatus::Completed => "[done]",
                crate::workflows::StepStatus::Running => "[running]",
                crate::workflows::StepStatus::Failed => "[failed]",
                crate::workflows::StepStatus::Skipped => "[skipped]",
                crate::workflows::StepStatus::Pending => "[pending]",
            };
            let approval = if step.approval_required {
                " (approval required)"
            } else {
                ""
            };
            lines.push(format!(
                "  {} Step {}: {}{}",
                status_icon, step.idx, step.name, approval
            ));
            if let Some(ref result) = step.result {
                let short = if result.len() > 100 {
                    format!("{}...", &result[..100])
                } else {
                    result.clone()
                };
                lines.push(format!("      Result: {short}"));
            }
            if let Some(ref err) = step.error {
                lines.push(format!("      Error: {err}"));
            }
        }

        Ok(ToolResult::success(lines.join("\n")))
    }

    async fn handle_approve(&self, engine: &WorkflowEngine, args: &Value) -> Result<ToolResult> {
        let id = match args.get("workflow_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required field: workflow_id")),
        };

        match engine.approve_and_resume(id).await {
            Ok(msg) => Ok(ToolResult::success(msg)),
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to approve workflow: {e}"
            ))),
        }
    }

    async fn handle_cancel(&self, engine: &WorkflowEngine, args: &Value) -> Result<ToolResult> {
        let id = match args.get("workflow_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required field: workflow_id")),
        };

        match engine.cancel(id).await {
            Ok(msg) => Ok(ToolResult::success(msg)),
            Err(e) => Ok(ToolResult::error(format!("Failed to cancel workflow: {e}"))),
        }
    }
}
