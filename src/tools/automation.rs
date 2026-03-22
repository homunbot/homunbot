//! LLM tool for creating and managing recurring automations.

use anyhow::{bail, Result};
use async_trait::async_trait;
use regex::Regex;
use serde_json::{json, Value};

use super::registry::{
    get_optional_bool, get_optional_string, get_string_param, Tool, ToolContext, ToolResult,
};
use crate::config::Config;
use crate::scheduler::AutomationSchedule;
use crate::storage::Database;
use crate::utils::text::truncate_str;

/// Automation tool — create, list, manage, and inspect recurring automations.
///
/// Persists to the `automations` table used by Web UI and scheduler,
/// so all automations are visible and manageable from any interface.
pub struct AutomationTool {
    db: Database,
}

impl AutomationTool {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl Tool for AutomationTool {
    fn name(&self) -> &str {
        "automation"
    }

    fn description(&self) -> &str {
        "Create and manage recurring automations with execution tracking, trigger evaluation, \
         and run history. Use for tasks that should run on a schedule (e.g. 'every morning \
         check my email'). For simple reminders without tracking, use the cron tool instead. \
         For one-shot multi-step tasks, use the workflow tool instead."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create", "list", "status", "history", "enable", "disable", "update", "delete"],
                    "description": "Action to perform"
                },
                "automation_id": {
                    "type": "string",
                    "description": "Automation ID (for actions: status, history, enable, disable, update, delete)"
                },
                "name": {
                    "type": "string",
                    "description": "Automation name (for action=create, or to rename with action=update)"
                },
                "prompt": {
                    "type": "string",
                    "description": "Task instructions the automation should execute (for action=create/update)"
                },
                "schedule": {
                    "type": "string",
                    "description": "Schedule: 'cron:MIN HOUR DOM MON DOW' or 'every:SECONDS'. Natural language accepted: 'ogni mattina alle 8' -> 'cron:0 8 * * *', 'every 6 hours' -> 'every:21600'"
                },
                "deliver_to": {
                    "type": "string",
                    "description": "Destination in format channel:chat_id. Defaults to current chat."
                },
                "trigger": {
                    "type": "string",
                    "enum": ["always", "on_change", "contains"],
                    "description": "Notification trigger: always (every run), on_change (only when output changes), contains (only when output contains trigger_value)"
                },
                "trigger_value": {
                    "type": "string",
                    "description": "Required when trigger='contains'. Text to detect in run output."
                },
                "filter": {
                    "type": "string",
                    "enum": ["all", "active", "paused", "error"],
                    "description": "Status filter for action=list. Default: all"
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

        match action {
            "create" => self.handle_create(&args, ctx).await,
            "list" => self.handle_list(&args).await,
            "status" => self.handle_status(&args).await,
            "history" => self.handle_history(&args).await,
            "enable" => self.handle_toggle(&args, true).await,
            "disable" => self.handle_toggle(&args, false).await,
            "update" => self.handle_update(&args, ctx).await,
            "delete" => self.handle_delete(&args).await,
            other => Ok(ToolResult::error(format!(
                "Unknown action: {other}. Use create, list, status, history, enable, disable, update, or delete."
            ))),
        }
    }
}

// ── Action handlers ─────────────────────────────────────────────────

impl AutomationTool {
    async fn handle_create(&self, args: &Value, ctx: &ToolContext) -> Result<ToolResult> {
        let prompt = get_string_param(args, "prompt")?;
        let schedule_raw = get_string_param(args, "schedule")?;
        if prompt.trim().is_empty() {
            return Ok(ToolResult::error("Prompt cannot be empty"));
        }
        let prompt = crate::scheduler::automations::normalize_runtime_prompt(&prompt);

        let schedule = match normalize_schedule(&schedule_raw) {
            Ok(v) => v,
            Err(e) => return Ok(ToolResult::error(e.to_string())),
        };

        let requested_name = get_optional_string(args, "name");
        let name = requested_name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string())
            .unwrap_or_else(|| derive_name(&prompt));

        let deliver_to = get_optional_string(args, "deliver_to")
            .unwrap_or_else(|| format!("{}:{}", ctx.channel, ctx.chat_id));
        if let Err(e) = parse_deliver_to(&deliver_to) {
            return Ok(ToolResult::error(e));
        }

        let trigger = get_optional_string(args, "trigger");
        let trigger_value = get_optional_string(args, "trigger_value");
        let (trigger_kind, trigger_value) =
            match normalize_trigger(trigger.as_deref(), trigger_value.as_deref()) {
                Ok(v) => v,
                Err(e) => return Ok(ToolResult::error(e)),
            };

        let config = match Config::load() {
            Ok(cfg) => cfg,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Failed to load config for automation validation: {e}"
                )));
            }
        };
        let compiled_plan =
            crate::scheduler::automations::compile_automation_plan(&prompt, &config);
        let status = if compiled_plan.is_valid() {
            "active"
        } else {
            "invalid_config"
        };
        let id = uuid::Uuid::new_v4().to_string();
        let plan_json = compiled_plan.plan_json();
        let dependencies_json = compiled_plan.dependencies_json();
        let validation_errors_json = compiled_plan.validation_errors_json();

        if let Err(e) = self
            .db
            .insert_automation_with_plan(
                &id,
                name.trim(),
                prompt.trim(),
                &schedule,
                true,
                status,
                Some(&deliver_to),
                &trigger_kind,
                trigger_value.as_deref(),
                Some(&plan_json),
                &dependencies_json,
                compiled_plan.plan.version,
                validation_errors_json.as_deref(),
            )
            .await
        {
            return Ok(ToolResult::error(format!(
                "Failed to create automation: {e}"
            )));
        }

        let validation_note = if compiled_plan.is_valid() {
            String::new()
        } else {
            format!(
                "\nvalidation_errors={}",
                compiled_plan.validation_errors.join(" | ")
            )
        };
        Ok(ToolResult::success(format!(
            "Automation created.\n\
             id={id}\n\
             name={name}\n\
             schedule={schedule}\n\
             deliver_to={deliver_to}\n\
             trigger={trigger_kind}\n\
             status={status}{}{}",
            trigger_value
                .as_deref()
                .map(|v| format!("\ntrigger_value={v}"))
                .unwrap_or_default(),
            validation_note
        )))
    }

    async fn handle_list(&self, args: &Value) -> Result<ToolResult> {
        let filter = args
            .get("filter")
            .and_then(|v| v.as_str())
            .unwrap_or("all");

        let automations = match self.db.load_automations().await {
            Ok(a) => a,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Failed to load automations: {e}"
                )))
            }
        };

        let filtered: Vec<_> = automations
            .into_iter()
            .filter(|a| match filter {
                "active" => a.status == "active" && a.enabled,
                "paused" => a.status == "paused" || !a.enabled,
                "error" => a.status == "error" || a.status == "invalid_config",
                _ => true,
            })
            .collect();

        if filtered.is_empty() {
            return Ok(ToolResult::success(match filter {
                "all" => "No automations found.".to_string(),
                other => format!("No {other} automations found."),
            }));
        }

        let mut lines = Vec::new();
        for a in &filtered {
            let enabled = if a.enabled { "on" } else { "off" };
            let last = a
                .last_result
                .as_deref()
                .map(|r| truncate_str(r, 60, "..."))
                .unwrap_or_else(|| "—".to_string());
            lines.push(format!(
                "- {} ({}) [{}/{}] schedule={} trigger={} last: {}",
                a.name, a.id, a.status, enabled, a.schedule, a.trigger_kind, last
            ));
        }

        Ok(ToolResult::success(lines.join("\n")))
    }

    async fn handle_status(&self, args: &Value) -> Result<ToolResult> {
        let id = match args.get("automation_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required field: automation_id")),
        };

        let automation = match self.db.load_automation(id).await {
            Ok(Some(a)) => a,
            Ok(None) => return Ok(ToolResult::error(format!("Automation '{id}' not found"))),
            Err(e) => return Ok(ToolResult::error(format!("Failed to load automation: {e}"))),
        };

        let mut lines = Vec::new();
        lines.push(format!("Automation: {} ({})", automation.name, automation.id));
        lines.push(format!(
            "Status: {} | Enabled: {}",
            automation.status, automation.enabled
        ));
        lines.push(format!("Schedule: {}", automation.schedule));
        lines.push(format!("Prompt: {}", truncate_str(&automation.prompt, 200, "...")));
        lines.push(format!(
            "Trigger: {}{}",
            automation.trigger_kind,
            automation
                .trigger_value
                .as_deref()
                .map(|v| format!(" (value: {v})"))
                .unwrap_or_default()
        ));
        if let Some(ref deliver) = automation.deliver_to {
            lines.push(format!("Deliver to: {deliver}"));
        }
        if let Some(ref last_run) = automation.last_run {
            lines.push(format!("Last run: {last_run}"));
        }
        if let Some(ref last_result) = automation.last_result {
            lines.push(format!(
                "Last result: {}",
                truncate_str(last_result, 300, "...")
            ));
        }
        if let Some(ref errors) = automation.validation_errors {
            let parsed = crate::scheduler::automations::parse_validation_errors_json(Some(errors));
            if !parsed.is_empty() {
                lines.push(format!("Validation errors: {}", parsed.join(" | ")));
            }
        }

        Ok(ToolResult::success(lines.join("\n")))
    }

    async fn handle_history(&self, args: &Value) -> Result<ToolResult> {
        let id = match args.get("automation_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required field: automation_id")),
        };

        let runs = match self.db.load_automation_runs(id, 10).await {
            Ok(r) => r,
            Err(e) => return Ok(ToolResult::error(format!("Failed to load history: {e}"))),
        };

        if runs.is_empty() {
            return Ok(ToolResult::success(format!(
                "No run history for automation '{id}'."
            )));
        }

        let mut lines = vec![format!("Last {} runs for '{id}':", runs.len())];
        for run in &runs {
            let result = run
                .result
                .as_deref()
                .map(|r| truncate_str(r, 80, "..."))
                .unwrap_or_else(|| "—".to_string());
            let finished = run.finished_at.as_deref().unwrap_or("running");
            lines.push(format!(
                "- [{}] {} → {} | {}",
                run.status, run.started_at, finished, result
            ));
        }

        Ok(ToolResult::success(lines.join("\n")))
    }

    async fn handle_toggle(&self, args: &Value, enable: bool) -> Result<ToolResult> {
        let id = match args.get("automation_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required field: automation_id")),
        };

        let automation = match self.db.load_automation(id).await {
            Ok(Some(a)) => a,
            Ok(None) => return Ok(ToolResult::error(format!("Automation '{id}' not found"))),
            Err(e) => return Ok(ToolResult::error(format!("Failed to load automation: {e}"))),
        };

        let new_status = if enable { "active" } else { "paused" };
        if let Err(e) = self
            .db
            .update_automation(
                id,
                crate::storage::AutomationUpdate {
                    enabled: Some(enable),
                    status: Some(new_status.to_string()),
                    ..Default::default()
                },
            )
            .await
        {
            return Ok(ToolResult::error(format!(
                "Failed to update automation: {e}"
            )));
        }

        let action_word = if enable { "enabled" } else { "disabled" };
        Ok(ToolResult::success(format!(
            "Automation \"{}\" ({id}) {action_word}.",
            automation.name
        )))
    }

    async fn handle_update(&self, args: &Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let id = match args.get("automation_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required field: automation_id")),
        };

        if self.db.load_automation(id).await?.is_none() {
            return Ok(ToolResult::error(format!("Automation '{id}' not found")));
        }

        let mut update = crate::storage::AutomationUpdate::default();
        let mut changed = Vec::new();

        if let Some(name) = get_optional_string(args, "name") {
            update.name = Some(name.clone());
            changed.push(format!("name={name}"));
        }
        if let Some(prompt) = get_optional_string(args, "prompt") {
            let cleaned = crate::scheduler::automations::normalize_runtime_prompt(&prompt);
            update.prompt = Some(cleaned.clone());
            changed.push("prompt updated".to_string());
        }
        if let Some(schedule_raw) = get_optional_string(args, "schedule") {
            let schedule = match normalize_schedule(&schedule_raw) {
                Ok(v) => v,
                Err(e) => return Ok(ToolResult::error(e.to_string())),
            };
            update.schedule = Some(schedule.clone());
            changed.push(format!("schedule={schedule}"));
        }
        if let Some(deliver_to) = get_optional_string(args, "deliver_to") {
            if let Err(e) = parse_deliver_to(&deliver_to) {
                return Ok(ToolResult::error(e));
            }
            update.deliver_to = Some(Some(deliver_to.clone()));
            changed.push(format!("deliver_to={deliver_to}"));
        }
        if let Some(trigger) = get_optional_string(args, "trigger") {
            let trigger_value = get_optional_string(args, "trigger_value");
            let (kind, value) =
                match normalize_trigger(Some(&trigger), trigger_value.as_deref()) {
                    Ok(v) => v,
                    Err(e) => return Ok(ToolResult::error(e)),
                };
            update.trigger_kind = Some(kind.clone());
            update.trigger_value = Some(value.clone());
            changed.push(format!("trigger={kind}"));
        }

        if changed.is_empty() {
            return Ok(ToolResult::error(
                "No fields to update. Provide name, prompt, schedule, deliver_to, or trigger.",
            ));
        }

        if let Err(e) = self.db.update_automation(id, update).await {
            return Ok(ToolResult::error(format!(
                "Failed to update automation: {e}"
            )));
        }

        Ok(ToolResult::success(format!(
            "Automation '{id}' updated: {}",
            changed.join(", ")
        )))
    }

    async fn handle_delete(&self, args: &Value) -> Result<ToolResult> {
        let id = match args.get("automation_id").and_then(|v| v.as_str()) {
            Some(id) => id,
            None => return Ok(ToolResult::error("Missing required field: automation_id")),
        };

        let automation = match self.db.load_automation(id).await {
            Ok(Some(a)) => a,
            Ok(None) => return Ok(ToolResult::error(format!("Automation '{id}' not found"))),
            Err(e) => return Ok(ToolResult::error(format!("Failed to load automation: {e}"))),
        };

        if let Err(e) = self.db.delete_automation(id).await {
            return Ok(ToolResult::error(format!(
                "Failed to delete automation: {e}"
            )));
        }

        Ok(ToolResult::success(format!(
            "Automation \"{}\" ({id}) deleted.",
            automation.name
        )))
    }
}

// ── Schedule & trigger helpers (unchanged) ──────────────────────────

fn normalize_schedule(raw: &str) -> Result<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        bail!("Schedule cannot be empty");
    }

    if raw.starts_with("cron:") || raw.starts_with("every:") {
        return Ok(AutomationSchedule::parse_stored(raw)?.as_stored());
    }

    // Accept bare cron format and normalize to `cron:...`.
    let parts = raw.split_whitespace().count();
    if parts == 5 || parts == 6 {
        return Ok(AutomationSchedule::from_cron(raw)?.as_stored());
    }

    // Helpful natural-language shortcuts for common cases.
    if let Some(secs) = parse_natural_interval(raw) {
        return Ok(AutomationSchedule::from_every(secs)?.as_stored());
    }
    if let Some((h, m, weekdays)) = parse_natural_daily(raw) {
        let dow = if weekdays { "1-5" } else { "*" };
        return Ok(format!("cron:{m} {h} * * {dow}"));
    }

    bail!(
        "Invalid schedule '{raw}'. Use 'cron:MIN HOUR DOM MON DOW' or 'every:SECONDS' (e.g. cron:0 8 * * * / every:21600)."
    )
}

fn parse_natural_interval(raw: &str) -> Option<u64> {
    let re_en = Regex::new(
        r"(?i)^\s*every\s+(\d+)\s*(second|seconds|sec|s|minute|minutes|min|m|hour|hours|h)\s*$",
    )
    .ok()?;
    if let Some(c) = re_en.captures(raw) {
        let n = c.get(1)?.as_str().parse::<u64>().ok()?;
        let unit = c.get(2)?.as_str().to_ascii_lowercase();
        return match unit.as_str() {
            "second" | "seconds" | "sec" | "s" => Some(n),
            "minute" | "minutes" | "min" | "m" => Some(n * 60),
            "hour" | "hours" | "h" => Some(n * 3600),
            _ => None,
        };
    }

    let re_it =
        Regex::new(r"(?i)^\s*ogni\s+(\d+)\s*(secondo|secondi|minuto|minuti|ora|ore)\s*$").ok()?;
    if let Some(c) = re_it.captures(raw) {
        let n = c.get(1)?.as_str().parse::<u64>().ok()?;
        let unit = c.get(2)?.as_str().to_ascii_lowercase();
        return match unit.as_str() {
            "secondo" | "secondi" => Some(n),
            "minuto" | "minuti" => Some(n * 60),
            "ora" | "ore" => Some(n * 3600),
            _ => None,
        };
    }

    None
}

fn parse_natural_daily(raw: &str) -> Option<(u32, u32, bool)> {
    let re_en = Regex::new(
        r"(?i)^\s*every\s+(day|weekday|weekdays|morning)\s*(?:at)?\s*(\d{1,2})(?::(\d{2}))?\s*$",
    )
    .ok()?;
    if let Some(c) = re_en.captures(raw) {
        let h = c.get(2)?.as_str().parse::<u32>().ok()?;
        let m = c
            .get(3)
            .and_then(|v| v.as_str().parse::<u32>().ok())
            .unwrap_or(0);
        if h > 23 || m > 59 {
            return None;
        }
        let day = c.get(1)?.as_str().to_ascii_lowercase();
        let weekdays = day.contains("weekday");
        return Some((h, m, weekdays));
    }

    let re_it = Regex::new(
        r"(?i)^\s*ogni\s+giorno(?:\s+feriale)?\s*(?:alle)?\s*(\d{1,2})(?::(\d{2}))?\s*$",
    )
    .ok()?;
    if let Some(c) = re_it.captures(raw) {
        let h = c.get(1)?.as_str().parse::<u32>().ok()?;
        let m = c
            .get(2)
            .and_then(|v| v.as_str().parse::<u32>().ok())
            .unwrap_or(0);
        if h > 23 || m > 59 {
            return None;
        }
        let weekdays = raw.to_ascii_lowercase().contains("feriale");
        return Some((h, m, weekdays));
    }

    None
}

fn parse_deliver_to(value: &str) -> Result<(String, String), String> {
    let (channel, chat_id) = value.rsplit_once(':').ok_or_else(|| {
        "deliver_to must be in format channel:chat_id (example: telegram:123456)".to_string()
    })?;
    let channel = channel.trim();
    let chat_id = chat_id.trim();
    if channel.is_empty() || chat_id.is_empty() {
        return Err(
            "deliver_to must be in format channel:chat_id (example: telegram:123456)".to_string(),
        );
    }
    Ok((channel.to_string(), chat_id.to_string()))
}

fn normalize_trigger(
    trigger: Option<&str>,
    trigger_value: Option<&str>,
) -> Result<(String, Option<String>), String> {
    let trigger = trigger
        .unwrap_or("always")
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_");

    match trigger.as_str() {
        "always" => Ok(("always".to_string(), None)),
        "on_change" | "changed" => Ok(("on_change".to_string(), None)),
        "contains" => {
            let value = trigger_value
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| "trigger_value is required when trigger=contains".to_string())?;
            Ok(("contains".to_string(), Some(value.to_string())))
        }
        _ => Err("trigger must be one of: always, on_change, contains".to_string()),
    }
}

fn derive_name(prompt: &str) -> String {
    let source = prompt.lines().next().unwrap_or("").trim();
    let mut words = source
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|w| !w.is_empty())
        .take(4)
        .collect::<Vec<_>>();

    if words.is_empty() {
        return "Automation".to_string();
    }

    for word in &mut words {
        if let Some(first) = word.get(0..1) {
            let rest = word.get(1..).unwrap_or("");
            *word = format!("{}{}", first.to_uppercase(), rest.to_lowercase());
        }
    }

    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> ToolContext {
        ToolContext {
            workspace: "/tmp".to_string(),
            channel: "telegram".to_string(),
            chat_id: "123456".to_string(),
            message_tx: None,
            approval_manager: None,
            skill_env: None,
            profile_id: None,
            profile_brain_dir: None,
            profile_slug: None,
        }
    }

    #[test]
    fn test_normalize_schedule_variants() {
        assert_eq!(normalize_schedule("every:300").unwrap(), "every:300");
        assert_eq!(
            normalize_schedule("*/15 * * * *").unwrap(),
            "cron:*/15 * * * *"
        );
        assert_eq!(normalize_schedule("0 8 * * * *").unwrap(), "cron:0 8 * * *");
        assert_eq!(normalize_schedule("every 6 hours").unwrap(), "every:21600");
        assert_eq!(
            normalize_schedule("ogni giorno alle 8").unwrap(),
            "cron:0 8 * * *"
        );
    }

    #[test]
    fn test_normalize_trigger_contains_requires_value() {
        let err = normalize_trigger(Some("contains"), None).unwrap_err();
        assert!(err.contains("trigger_value"));
    }

    #[test]
    fn test_sanitize_prompt_for_creation_style_input() {
        let prompt = "Crea una automation chiamata Email Digest: controlla le email non lette e fammi un riassunto";
        let cleaned = crate::scheduler::automations::normalize_runtime_prompt(prompt);
        assert_eq!(cleaned, "controlla le email non lette e fammi un riassunto");
    }

    #[tokio::test]
    async fn test_create_automation_success() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).await.unwrap();
        let tool = AutomationTool::new(db.clone());

        let args = json!({
            "action": "create",
            "name": "Email Digest",
            "prompt": "Vai su Gmail, leggi le email non lette e riassumi.",
            "schedule": "ogni giorno alle 9",
            "trigger": "always"
        });

        let res = tool.execute(args, &test_ctx()).await.unwrap();
        assert!(!res.is_error, "create failed: {}", res.output);
        assert!(res.output.contains("Automation created"));

        let rows = db.load_automations().await.unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.name, "Email Digest");
        assert_eq!(row.schedule, "cron:0 9 * * *");
        assert_eq!(row.deliver_to.as_deref(), Some("telegram:123456"));
        assert_eq!(row.trigger_kind, "always");
    }

    #[tokio::test]
    async fn test_list_and_status_actions() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).await.unwrap();
        let tool = AutomationTool::new(db.clone());

        // Create one
        let args = json!({
            "action": "create",
            "name": "Test Auto",
            "prompt": "do something",
            "schedule": "every:3600"
        });
        let res = tool.execute(args, &test_ctx()).await.unwrap();
        assert!(!res.is_error);

        // List all
        let res = tool
            .execute(json!({"action": "list"}), &test_ctx())
            .await
            .unwrap();
        assert!(!res.is_error);
        assert!(res.output.contains("Test Auto"));

        // Status by ID
        let rows = db.load_automations().await.unwrap();
        let id = &rows[0].id;
        let res = tool
            .execute(
                json!({"action": "status", "automation_id": id}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(!res.is_error);
        assert!(res.output.contains("Test Auto"));
        assert!(res.output.contains("every:3600"));
    }

    #[tokio::test]
    async fn test_enable_disable_delete() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("test.db")).await.unwrap();
        let tool = AutomationTool::new(db.clone());

        let args = json!({
            "action": "create",
            "name": "Toggle Test",
            "prompt": "check stuff",
            "schedule": "every:600"
        });
        tool.execute(args, &test_ctx()).await.unwrap();
        let id = db.load_automations().await.unwrap()[0].id.clone();

        // Disable
        let res = tool
            .execute(
                json!({"action": "disable", "automation_id": &id}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(!res.is_error);
        assert!(res.output.contains("disabled"));

        let auto = db.load_automation(&id).await.unwrap().unwrap();
        assert!(!auto.enabled);
        assert_eq!(auto.status, "paused");

        // Enable
        let res = tool
            .execute(
                json!({"action": "enable", "automation_id": &id}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(!res.is_error);
        assert!(res.output.contains("enabled"));

        // Delete
        let res = tool
            .execute(
                json!({"action": "delete", "automation_id": &id}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(!res.is_error);
        assert!(res.output.contains("deleted"));

        assert!(db.load_automation(&id).await.unwrap().is_none());
    }
}
