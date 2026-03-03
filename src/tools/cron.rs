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
        "Schedule recurring or one-time tasks. Actions: add, list, remove. \
         Schedule format: 'every:SECONDS' (e.g. every:1800 for 30 minutes, every:3600 for 1 hour), \
         'cron:MIN HOUR DOM MON DOW' (e.g. cron:*/30 * * * * for every 30 min, cron:0 9 * * * for 9 AM daily), \
         'at:ISO_TIMESTAMP' for one-time. IMPORTANT: 'every:' uses SECONDS not minutes."
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
                    "description": "Schedule format (ALWAYS use a prefix): 'every:SECONDS' (e.g. every:1800 = 30 min, every:3600 = 1 hour), 'cron:MIN HOUR DOM MON DOW' (e.g. cron:*/30 * * * * = every 30 min), 'at:YYYY-MM-DDTHH:MM:SS' (one-time). The every: value is in SECONDS."
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
        let action = args
            .get("action")
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
                "Missing 'schedule' parameter. Use 'every:SECONDS' (e.g. every:1800 for 30 min), \
                 'cron:MIN HOUR DOM MON DOW' (e.g. cron:0 9 * * *), or 'at:ISO_TIMESTAMP'.",
            ),
        };

        // Validate schedule format to prevent common mistakes
        if let Some(err) = validate_schedule(schedule) {
            return ToolResult::error(err);
        }

        // Auto-set deliver_to from the originating channel if not explicitly provided.
        // This ensures cron responses are sent back to the user on the same channel.
        let explicit_deliver = args
            .get("deliver_to")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let deliver_to =
            explicit_deliver.unwrap_or_else(|| format!("{}:{}", ctx.channel, ctx.chat_id));

        match self
            .scheduler
            .add_job(name, message, schedule, Some(&deliver_to))
            .await
        {
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

/// Minimum allowed interval for `every:` schedules (60 seconds).
const MIN_INTERVAL_SECS: u64 = 60;

/// Validate a schedule string before creating a cron job.
/// Returns `Some(error_message)` if invalid, `None` if OK.
fn validate_schedule(schedule: &str) -> Option<String> {
    // Reject bare numbers — too ambiguous (seconds? minutes?)
    if schedule.parse::<u64>().is_ok() {
        return Some(format!(
            "Ambiguous schedule '{}'. Use an explicit prefix: 'every:{}' for {} seconds, \
             or 'every:{}' for {} minutes. The every: value is always in SECONDS.",
            schedule,
            schedule,
            schedule,
            schedule.parse::<u64>().unwrap_or(0) * 60,
            schedule,
        ));
    }

    // Validate every: interval — must be at least MIN_INTERVAL_SECS
    if let Some(secs_str) = schedule.strip_prefix("every:") {
        if let Ok(secs) = secs_str.trim().parse::<u64>() {
            if secs < MIN_INTERVAL_SECS {
                return Some(format!(
                    "Interval too short: every:{secs} is {secs} seconds. \
                     Minimum is {MIN_INTERVAL_SECS} seconds (1 minute). \
                     Did you mean every:{} for {} minutes?",
                    secs * 60,
                    secs,
                ));
            }
        } else {
            return Some(format!(
                "Invalid interval: 'every:{secs_str}'. Must be a number of seconds (e.g. every:1800 for 30 minutes)."
            ));
        }
    }

    // Validate cron: expression has 5 fields
    if let Some(expr) = schedule.strip_prefix("cron:") {
        let fields: Vec<&str> = expr.split_whitespace().collect();
        if fields.len() != 5 {
            return Some(format!(
                "Invalid cron expression: '{}'. Must have 5 fields: MIN HOUR DOM MON DOW \
                 (e.g. cron:0 9 * * * for 9 AM daily, cron:*/30 * * * * for every 30 min).",
                expr.trim()
            ));
        }
    }

    // Validate at: timestamp format
    if let Some(ts) = schedule.strip_prefix("at:") {
        if chrono::NaiveDateTime::parse_from_str(ts.trim(), "%Y-%m-%dT%H:%M:%S").is_err() {
            return Some(format!(
                "Invalid timestamp: 'at:{ts}'. Must be ISO format: at:YYYY-MM-DDTHH:MM:SS \
                 (e.g. at:2026-03-03T10:30:00)."
            ));
        }
    }

    // Reject completely unknown formats
    if !schedule.starts_with("every:")
        && !schedule.starts_with("cron:")
        && !schedule.starts_with("at:")
    {
        // Allow bare 5-field cron expressions (auto-detected by parser)
        let parts: Vec<&str> = schedule.split_whitespace().collect();
        if parts.len() != 5 {
            return Some(format!(
                "Unknown schedule format: '{schedule}'. Use 'every:SECONDS', 'cron:MIN HOUR DOM MON DOW', \
                 or 'at:YYYY-MM-DDTHH:MM:SS'."
            ));
        }
    }

    None
}
