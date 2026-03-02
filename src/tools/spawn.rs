use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use super::registry::{Tool, ToolContext, ToolResult};
use crate::agent::subagent::SubagentManager;

/// Spawn tool — lets the agent create background tasks via subagents.
///
/// Actions:
/// - spawn: Start a new background task
/// - list: Show running tasks
///
/// Uses a late-bound manager via `OnceCell` to break the circular dependency
/// between AgentLoop (owns ToolRegistry) and SubagentManager (needs Arc<AgentLoop>).
pub struct SpawnTool {
    manager: Arc<tokio::sync::OnceCell<Arc<SubagentManager>>>,
}

impl SpawnTool {
    /// Create a new SpawnTool with a shared cell for late-binding the SubagentManager.
    /// Call this before creating AgentLoop, then set the manager after.
    pub fn new(manager: Arc<tokio::sync::OnceCell<Arc<SubagentManager>>>) -> Self {
        Self { manager }
    }

    fn get_manager(&self) -> Result<&SubagentManager> {
        self.manager
            .get()
            .map(|m| m.as_ref())
            .ok_or_else(|| anyhow::anyhow!("Subagent manager not initialized yet"))
    }
}

#[async_trait]
impl Tool for SpawnTool {
    fn name(&self) -> &str {
        "spawn_subagent"
    }

    fn description(&self) -> &str {
        "Spawn a background task that runs independently. Use for long-running tasks that shouldn't block the conversation. Action 'spawn' creates a new task, 'list' shows running tasks."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["spawn", "list"],
                    "description": "Action to perform"
                },
                "description": {
                    "type": "string",
                    "description": "Brief description of the task (for action=spawn)"
                },
                "message": {
                    "type": "string",
                    "description": "The task instructions for the subagent (for action=spawn)"
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

        let manager = match self.get_manager() {
            Ok(m) => m,
            Err(e) => return Ok(ToolResult::error(format!("{e}"))),
        };

        let result = match action {
            "spawn" => {
                let description = args
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Background task");
                let message = match args.get("message").and_then(|v| v.as_str()) {
                    Some(m) => m,
                    None => return Ok(ToolResult::error("Missing 'message' parameter")),
                };

                match manager
                    .spawn(description, message, &ctx.channel, &ctx.chat_id)
                    .await
                {
                    Ok(task_id) => ToolResult::success(format!(
                        "Background task spawned: id={task_id}, description={description}"
                    )),
                    Err(e) => ToolResult::error(format!("Failed to spawn task: {e}")),
                }
            }
            "list" => {
                let running: Vec<(String, String)> = manager.list_running().await;
                if running.is_empty() {
                    ToolResult::success("No background tasks running.")
                } else {
                    let mut output = format!("{} running task(s):\n", running.len());
                    for (id, desc) in &running {
                        output.push_str(&format!("  [{id}] {desc}\n"));
                    }
                    ToolResult::success(output)
                }
            }
            _ => ToolResult::error(format!("Unknown action: {action}. Use 'spawn' or 'list'.")),
        };

        Ok(result)
    }
}
