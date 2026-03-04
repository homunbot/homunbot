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

/// Tool to create automations directly from natural conversation.
///
/// It persists to the same `automations` table used by Web UI/CLI,
/// so the scheduler and runtime pipeline are reused with no extra service.
pub struct CreateAutomationTool {
    db: Database,
}

impl CreateAutomationTool {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl Tool for CreateAutomationTool {
    fn name(&self) -> &str {
        "create_automation"
    }

    fn description(&self) -> &str {
        "Create a recurring automation from conversation. \
         Convert natural user timing into a schedule before calling: \
         prefer 'cron:MIN HOUR DOM MON DOW' or 'every:SECONDS'. \
         Examples: 'ogni mattina alle 8' -> 'cron:0 8 * * *', \
         'every 6 hours' -> 'every:21600'."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Short automation name. If omitted, generated from prompt."
                },
                "prompt": {
                    "type": "string",
                    "description": "Task instructions the automation should execute."
                },
                "schedule": {
                    "type": "string",
                    "description": "Schedule in 'cron:MIN HOUR DOM MON DOW' or 'every:SECONDS'. Bare cron (5 fields) is also accepted."
                },
                "deliver_to": {
                    "type": "string",
                    "description": "Optional destination in format channel:chat_id. Defaults to current chat."
                },
                "trigger": {
                    "type": "string",
                    "enum": ["always", "on_change", "contains"],
                    "description": "Notification trigger behavior."
                },
                "trigger_value": {
                    "type": "string",
                    "description": "Required when trigger='contains'. Text to detect in run output."
                },
                "enabled": {
                    "type": "boolean",
                    "description": "Whether automation starts active. Default true."
                }
            },
            "required": ["prompt", "schedule"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let prompt = get_string_param(&args, "prompt")?;
        let schedule_raw = get_string_param(&args, "schedule")?;
        if prompt.trim().is_empty() {
            return Ok(ToolResult::error("Prompt cannot be empty"));
        }
        let prompt = crate::scheduler::automations::normalize_runtime_prompt(&prompt);

        let schedule = match normalize_schedule(&schedule_raw) {
            Ok(v) => v,
            Err(e) => return Ok(ToolResult::error(e.to_string())),
        };

        let requested_name = get_optional_string(&args, "name");
        let name = requested_name
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string())
            .unwrap_or_else(|| derive_name(&prompt));

        let deliver_to = get_optional_string(&args, "deliver_to")
            .unwrap_or_else(|| format!("{}:{}", ctx.channel, ctx.chat_id));
        if let Err(e) = parse_deliver_to(&deliver_to) {
            return Ok(ToolResult::error(e));
        }

        let trigger = get_optional_string(&args, "trigger");
        let trigger_value = get_optional_string(&args, "trigger_value");
        let (trigger_kind, trigger_value) =
            match normalize_trigger(trigger.as_deref(), trigger_value.as_deref()) {
                Ok(v) => v,
                Err(e) => return Ok(ToolResult::error(e)),
            };

        let enabled = get_optional_bool(&args, "enabled").unwrap_or(true);
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
        let status = if !enabled {
            "paused"
        } else if compiled_plan.is_valid() {
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
                enabled,
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
}

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
    // English: every 6 hours / every 30 minutes
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

    // Italian: ogni 6 ore / ogni 30 minuti / ogni 10 secondi
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
    // every day at 08[:30]
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

    // ogni giorno alle 8[:30] / ogni giorno feriale alle 9
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
        let tool = CreateAutomationTool::new(db.clone());

        let args = json!({
            "name": "Email Digest",
            "prompt": "Vai su Gmail, leggi le email non lette e riassumi.",
            "schedule": "ogni giorno alle 9",
            "trigger": "always"
        });

        let res = tool.execute(args, &test_ctx()).await.unwrap();
        assert!(!res.is_error);
        assert!(res.output.contains("Automation created"));

        let rows = db.load_automations().await.unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.name, "Email Digest");
        assert_eq!(row.schedule, "cron:0 9 * * *");
        assert_eq!(row.deliver_to.as_deref(), Some("telegram:123456"));
        assert_eq!(row.trigger_kind, "always");
    }
}
