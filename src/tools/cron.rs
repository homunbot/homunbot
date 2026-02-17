use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use super::registry::{Tool, ToolContext, ToolResult};
use crate::scheduler::CronScheduler;

/// Cron tool — lets the agent schedule recurring tasks.
///
/// Actions:
/// - add: Create a new scheduled job
/// - list: Show all scheduled jobs
/// - remove: Delete a job by ID
pub struct CronTool {
    scheduler: Arc<CronScheduler>,
}

impl CronTool {
    pub fn new(scheduler: Arc<CronScheduler>) -> Self {
        Self { scheduler }
    }
}

#[async_trait]
impl Tool for CronTool {
    fn name(&self) -> &str {
        "cron"
    }

    fn description(&self) -> &str {
        "Schedule recurring or one-time tasks. Use action='add' to create a job, 'list' to see all jobs, 'remove' to delete a job."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add", "list", "remove"],
                    "description": "The action to perform"
                },
                "name": {
                    "type": "string",
                    "description": "Name for the job (for action=add)"
                },
                "message": {
                    "type": "string",
                    "description": "The message/task to execute when the job fires (for action=add)"
                },
                "schedule": {
                    "type": "string",
                    "description": "Schedule: 'every:300' (every 300 seconds), 'cron:0 9 * * *' (9 AM daily), 'at:2025-02-20T10:30:00' (one-time)"
                },
                "deliver_to": {
                    "type": "string",
                    "description": "Channel:chat_id to deliver results to (e.g. 'telegram:123456')"
                },
                "job_id": {
                    "type": "string",
                    "description": "Job ID to remove (for action=remove)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let action = args.get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");

        let result = match action {
            "add" => self.add_job(&args, ctx).await,
            "list" => self.list_jobs().await,
            "remove" => self.remove_job(&args).await,
            _ => ToolResult::error(format!(
                "Unknown action: {action}. Use 'add', 'list', or 'remove'."
            )),
        };

        Ok(result)
    }
}

impl CronTool {
    async fn add_job(&self, args: &Value, ctx: &ToolContext) -> ToolResult {
        let name = match args.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return ToolResult::error("Missing 'name' parameter"),
        };

        let message = match args.get("message").and_then(|v| v.as_str()) {
            Some(m) => m,
            None => return ToolResult::error("Missing 'message' parameter"),
        };

        let schedule = match args.get("schedule").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return ToolResult::error(
                "Missing 'schedule' parameter. Examples: 'every:300', 'cron:0 9 * * *', 'at:2025-02-20T10:30:00'"
            ),
        };

        // Auto-set deliver_to from the originating channel if not explicitly provided.
        // This ensures cron responses are sent back to the user on the same channel.
        let explicit_deliver = args.get("deliver_to").and_then(|v| v.as_str()).map(|s| s.to_string());
        let deliver_to = explicit_deliver.unwrap_or_else(|| {
            format!("{}:{}", ctx.channel, ctx.chat_id)
        });

        match self.scheduler.add_job(name, message, schedule, Some(&deliver_to)).await {
            Ok(id) => ToolResult::success(format!(
                "Job created: id={id}, name={name}, schedule={schedule}, deliver_to={deliver_to}"
            )),
            Err(e) => ToolResult::error(format!("Failed to create job: {e}")),
        }
    }

    async fn list_jobs(&self) -> ToolResult {
        let jobs = self.scheduler.list_jobs().await;

        if jobs.is_empty() {
            return ToolResult::success("No scheduled jobs.");
        }

        let mut output = format!("{} scheduled job(s):\n", jobs.len());
        for job in &jobs {
            let status = if job.enabled { "✓" } else { "✗" };
            let last = job.last_run.as_deref().unwrap_or("never");
            output.push_str(&format!(
                "\n[{status}] {id} | {name} | {schedule} | last: {last}\n    → {message}",
                id = job.id,
                name = job.name,
                schedule = job.schedule,
                message = job.message,
            ));
            if let Some(deliver) = &job.deliver_to {
                output.push_str(&format!("\n    deliver: {deliver}"));
            }
        }

        ToolResult::success(output)
    }

    async fn remove_job(&self, args: &Value) -> ToolResult {
        let id = match args.get("job_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return ToolResult::error("Missing 'job_id' parameter"),
        };

        match self.scheduler.remove_job(id).await {
            Ok(true) => ToolResult::success(format!("Job {id} removed.")),
            Ok(false) => ToolResult::error(format!("Job {id} not found.")),
            Err(e) => ToolResult::error(format!("Failed to remove job: {e}")),
        }
    }
}
