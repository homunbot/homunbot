use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::{mpsc, Mutex};

use crate::storage::{AutomationRow, AutomationUpdate, Database};
use crate::workflows::engine::WorkflowEngine;
use crate::workflows::{StepDefinition, WorkflowCreateRequest};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledKind {
    Automation,
}

/// Message sent when a cron job fires
#[derive(Debug, Clone)]
pub struct CronEvent {
    pub kind: ScheduledKind,
    pub job_id: String,
    pub job_name: String,
    pub message: String,
    pub deliver_to: Option<String>,
    /// Present for automation events to track end-to-end lifecycle.
    pub automation_run_id: Option<String>,
}

/// Automation scheduler — fires recurring automations on schedule.
///
/// Single timer checks all automations every 30 seconds.
/// When an automation fires, either:
/// - Creates a workflow (if `workflow_steps_json` present)
/// - Sends a CronEvent through mpsc channel → gateway → agent loop
pub struct CronScheduler {
    db: Database,
    event_tx: mpsc::Sender<CronEvent>,
    /// Late-bound workflow engine — set after AgentLoop is created.
    workflow_engine: Arc<tokio::sync::OnceCell<Arc<WorkflowEngine>>>,
}

impl CronScheduler {
    pub fn new(db: Database, event_tx: mpsc::Sender<CronEvent>) -> Self {
        Self {
            db,
            event_tx,
            workflow_engine: Arc::new(tokio::sync::OnceCell::new()),
        }
    }

    /// Bind the workflow engine (late init — engine is created after CronScheduler).
    pub fn set_workflow_engine(&self, engine: Arc<WorkflowEngine>) {
        let _ = self.workflow_engine.set(engine);
    }

    /// Start the scheduler loop.
    pub async fn start(self: Arc<Self>) -> Result<tokio::task::JoinHandle<()>> {
        let count = self.db.load_automations().await?.iter().filter(|a| a.enabled).count();
        tracing::info!(active_automations = count, "Automation scheduler started");

        let scheduler = self.clone();
        let handle = tokio::spawn(async move {
            scheduler.run_loop().await;
        });

        Ok(handle)
    }

    /// Main scheduler loop — checks automations every 30 seconds.
    async fn run_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            interval.tick().await;
            if let Err(e) = self.check_and_fire_automations().await {
                tracing::error!(error = %e, "Automation scheduler error");
            }
        }
    }

    /// Check all enabled automations and fire any that are due.
    ///
    /// Two execution paths, unified completion:
    /// - **Prompt-based** (no workflow_steps): sends CronEvent → gateway → agent loop
    ///   → `evaluate_and_complete_automation_run()` in gateway post-processing.
    /// - **Workflow-based** (has workflow_steps): calls `WorkflowEngine::create_and_start()`
    ///   directly → automation run marked "running" → workflow engine calls
    ///   `evaluate_and_complete_automation_run()` on completion/failure.
    ///
    /// Both paths converge on the same trigger evaluation and run completion semantics.
    async fn check_and_fire_automations(&self) -> Result<()> {
        let now = chrono::Utc::now();
        let automations = self.db.load_automations().await?;
        let runtime_config = crate::config::Config::load().ok();

        for automation in automations {
            if !automation.enabled {
                continue;
            }

            let mut effective_status = automation.status.clone();
            if let Some(cfg) = runtime_config.as_ref() {
                let compiled =
                    crate::scheduler::automations::compile_automation_plan(&automation.prompt, cfg);
                let plan_json = compiled.plan_json();
                let dependencies_json = compiled.dependencies_json();
                let validation_errors_json = compiled.validation_errors_json();
                let desired_status = if compiled.is_valid() {
                    "active".to_string()
                } else {
                    "invalid_config".to_string()
                };

                let needs_update = automation.plan_json.as_deref() != Some(plan_json.as_str())
                    || automation.dependencies_json != dependencies_json
                    || automation.plan_version != compiled.plan.version
                    || automation.validation_errors.as_deref() != validation_errors_json.as_deref()
                    || automation.status != desired_status;

                if needs_update {
                    let mut update = AutomationUpdate {
                        status: Some(desired_status.clone()),
                        plan_json: Some(Some(plan_json)),
                        dependencies_json: Some(Some(dependencies_json)),
                        plan_version: Some(compiled.plan.version),
                        validation_errors: Some(validation_errors_json.clone()),
                        ..Default::default()
                    };
                    if !compiled.is_valid() {
                        update.last_result = Some(Some(format!(
                            "Automation configuration invalid: {}",
                            compiled.validation_errors.join(" | ")
                        )));
                    }
                    let _ = self.db.update_automation(&automation.id, update).await;
                }
                effective_status = desired_status;
            }

            if effective_status.eq_ignore_ascii_case("paused")
                || effective_status.eq_ignore_ascii_case("invalid_config")
            {
                continue;
            }

            let overdue =
                is_schedule_overdue(&automation.schedule, automation.last_run.as_deref(), &now);
            let should_fire = overdue
                || should_fire_schedule(&automation.schedule, automation.last_run.as_deref(), &now);
            if !should_fire {
                continue;
            }
            if overdue {
                tracing::info!(
                    automation_id = %automation.id,
                    name = %automation.name,
                    "Catching up missed automation schedule"
                );
            }

            let run_id = uuid::Uuid::new_v4().to_string();
            let queued_msg = "Scheduled run queued".to_string();

            if let Err(e) = self
                .db
                .insert_automation_run(&run_id, &automation.id, "queued", Some(&queued_msg))
                .await
            {
                tracing::error!(
                    error = %e,
                    automation_id = %automation.id,
                    "Failed to insert automation run"
                );
                continue;
            }

            let _ = self
                .db
                .update_automation(
                    &automation.id,
                    AutomationUpdate {
                        status: Some("active".to_string()),
                        last_result: Some(Some(queued_msg.clone())),
                        touch_last_run: true,
                        ..Default::default()
                    },
                )
                .await;

            tracing::info!(
                automation_id = %automation.id,
                automation_name = %automation.name,
                run_id = %run_id,
                "Automation firing"
            );

            // If automation has workflow steps, create a workflow instead of sending prompt
            if let Some(ref steps_json) = automation.workflow_steps_json {
                if let Some(engine) = self.workflow_engine.get() {
                    match serde_json::from_str::<Vec<StepDefinition>>(steps_json) {
                        Ok(steps) if !steps.is_empty() => {
                            let req = WorkflowCreateRequest {
                                name: automation.name.clone(),
                                objective: automation.prompt.clone(),
                                steps,
                                deliver_to: automation.deliver_to.clone(),
                                automation_id: Some(automation.id.clone()),
                                automation_run_id: Some(run_id.clone()),
                            };
                            let channel = automation.deliver_to.as_deref().unwrap_or("automation");
                            match engine.create_and_start(req, channel, channel).await {
                                Ok(wf_id) => {
                                    // Mark as "running" — will be completed by
                                    // WorkflowEngine when the workflow finishes.
                                    let msg = format!("Workflow {wf_id} in progress");
                                    let _ = self
                                        .db
                                        .complete_automation_run(&run_id, "running", Some(&msg))
                                        .await;
                                    let _ = self
                                        .db
                                        .update_automation(
                                            &automation.id,
                                            AutomationUpdate {
                                                last_result: Some(Some(msg)),
                                                ..Default::default()
                                            },
                                        )
                                        .await;
                                }
                                Err(e) => {
                                    let msg = format!("Failed to start workflow: {e}");
                                    tracing::error!(automation_id = %automation.id, %e, "Workflow creation failed");
                                    let _ = self
                                        .db
                                        .complete_automation_run(&run_id, "error", Some(&msg))
                                        .await;
                                    let _ = self
                                        .db
                                        .update_automation(
                                            &automation.id,
                                            AutomationUpdate {
                                                status: Some("error".to_string()),
                                                last_result: Some(Some(msg)),
                                                ..Default::default()
                                            },
                                        )
                                        .await;
                                }
                            }
                            continue; // Skip normal prompt path
                        }
                        Ok(_) => {
                            tracing::warn!(automation_id = %automation.id, "Empty workflow steps, falling back to prompt");
                        }
                        Err(e) => {
                            tracing::warn!(automation_id = %automation.id, error = %e, "Invalid workflow steps JSON, falling back to prompt");
                        }
                    }
                } else {
                    tracing::warn!(automation_id = %automation.id, "Workflow engine not available, falling back to prompt");
                }
            }

            // Build effective prompt: for multi-step automations, compose from workflow steps
            let effective_prompt =
                crate::scheduler::automations::build_effective_prompt_from_row(&automation);
            let runtime_prompt = crate::scheduler::automations::build_runtime_run_input_from_plan(
                automation.plan_json.as_deref(),
                &effective_prompt,
            );

            let event = CronEvent {
                kind: ScheduledKind::Automation,
                job_id: automation.id.clone(),
                job_name: automation.name.clone(),
                message: runtime_prompt,
                deliver_to: automation.deliver_to.clone(),
                automation_run_id: Some(run_id.clone()),
            };

            if let Err(e) = self.event_tx.send(event).await {
                tracing::error!(error = %e, "Failed to send automation event");
                let _ = self
                    .db
                    .complete_automation_run(
                        &run_id,
                        "error",
                        Some("Failed to enqueue automation event"),
                    )
                    .await;
                let _ = self
                    .db
                    .update_automation(
                        &automation.id,
                        AutomationUpdate {
                            status: Some("error".to_string()),
                            last_result: Some(Some(
                                "Failed to enqueue automation event".to_string(),
                            )),
                            ..Default::default()
                        },
                    )
                    .await;
            }
        }

        Ok(())
    }

}

/// Check if a schedule is overdue — server was down when it should have fired.
///
/// Returns true if enough time has passed since `last_run` that at least one
/// scheduled firing was missed. Used for catch-up on server restart.
fn is_schedule_overdue(
    schedule: &str,
    last_run: Option<&str>,
    now: &chrono::DateTime<chrono::Utc>,
) -> bool {
    let Some(last_str) = last_run else {
        // Never ran before — not overdue, first run will happen naturally
        return false;
    };
    let last_time = parse_last_run_timestamp(last_str);
    let Some(last_time) = last_time else {
        return false;
    };

    // Use AutomationSchedule to compute when the next run should have been
    use crate::scheduler::automations::AutomationSchedule;
    let Ok(parsed) = AutomationSchedule::parse_stored(schedule) else {
        return false;
    };
    match parsed.next_run_at(last_time, Some(last_str)) {
        Some(next) => next < *now, // next run is in the past → missed
        None => false,
    }
}

/// Parse a last_run timestamp (SQLite format or RFC3339).
fn parse_last_run_timestamp(raw: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(naive) =
        chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
    {
        return Some(naive.and_utc());
    }
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

fn should_fire_schedule(
    schedule: &str,
    last_run: Option<&str>,
    now: &chrono::DateTime<chrono::Utc>,
) -> bool {
    match parse_schedule(schedule) {
        ScheduleKind::Every(secs) => match last_run {
            Some(last) => {
                if let Ok(last_time) =
                    chrono::NaiveDateTime::parse_from_str(last, "%Y-%m-%d %H:%M:%S")
                {
                    let last_utc = last_time.and_utc();
                    (now.to_owned() - last_utc).num_seconds() >= secs as i64
                } else {
                    true
                }
            }
            None => true,
        },
        ScheduleKind::Cron(expr) => {
            if !cron_matches_now(&expr, now) {
                return false;
            }
            // Guard: don't fire again if we already fired this minute
            if let Some(last) = last_run {
                if let Ok(last_time) =
                    chrono::NaiveDateTime::parse_from_str(last, "%Y-%m-%d %H:%M:%S")
                {
                    let last_utc = last_time.and_utc();
                    let elapsed = (*now - last_utc).num_seconds();
                    if elapsed < 60 {
                        return false;
                    }
                }
            }
            true
        }
        ScheduleKind::At(target) => {
            if let Ok(target_time) =
                chrono::NaiveDateTime::parse_from_str(&target, "%Y-%m-%dT%H:%M:%S")
            {
                let target_utc = target_time.and_utc();
                *now >= target_utc && last_run.is_none()
            } else {
                false
            }
        }
        ScheduleKind::Unknown => {
            tracing::warn!(schedule = %schedule, "Unknown schedule format");
            false
        }
    }
}

// --- Schedule parsing ---

enum ScheduleKind {
    Every(u64),   // seconds
    Cron(String), // cron expression
    At(String),   // ISO timestamp
    Unknown,
}

fn parse_schedule(schedule: &str) -> ScheduleKind {
    if let Some(secs) = schedule.strip_prefix("every:") {
        if let Ok(s) = secs.trim().parse::<u64>() {
            return ScheduleKind::Every(s);
        }
    }

    if let Some(expr) = schedule.strip_prefix("cron:") {
        if let Some(normalized) = normalize_cron_expr(expr) {
            return ScheduleKind::Cron(normalized);
        }
        tracing::warn!(schedule = %schedule, "Invalid cron schedule format");
        return ScheduleKind::Unknown;
    }

    if let Some(ts) = schedule.strip_prefix("at:") {
        return ScheduleKind::At(ts.trim().to_string());
    }

    // Try to guess: if it looks like a cron expression (has spaces and numbers)
    if let Some(normalized) = normalize_cron_expr(schedule) {
        return ScheduleKind::Cron(normalized);
    }

    // Try as seconds (bare number fallback) — enforce minimum 60s
    if let Ok(s) = schedule.parse::<u64>() {
        let secs = s.max(60);
        if secs != s {
            tracing::warn!(
                original = s,
                enforced = secs,
                "Bare number schedule below 60s minimum, enforcing 60s"
            );
        }
        return ScheduleKind::Every(secs);
    }

    ScheduleKind::Unknown
}

/// Normalize cron expression to 5-field format (MIN HOUR DOM MON DOW).
///
/// Supports two 6-field legacy variants for compatibility:
/// - trailing wildcard mistake: `0 8 * * * *` -> `0 8 * * *`
/// - leading seconds format: `0 0 8 * * *` -> `0 8 * * *`
fn normalize_cron_expr(expr: &str) -> Option<String> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    match parts.len() {
        5 => Some(parts.join(" ")),
        6 if parts[2] == "*" => Some(parts[..5].join(" ")),
        6 if parts[0] == "0" => Some(parts[1..].join(" ")),
        _ => None,
    }
}

/// Simple cron expression matching against current time.
/// Supports: minute hour day_of_month month day_of_week
/// Supports: * (any), specific numbers, comma-separated lists
fn cron_matches_now(expr: &str, now: &chrono::DateTime<chrono::Utc>) -> bool {
    let normalized = match normalize_cron_expr(expr) {
        Some(v) => v,
        None => return false,
    };

    let parts: Vec<&str> = normalized.split_whitespace().collect();
    if parts.len() != 5 {
        return false;
    }

    let minute = now.format("%M").to_string().parse::<u32>().unwrap_or(0);
    let hour = now.format("%H").to_string().parse::<u32>().unwrap_or(0);
    let day = now.format("%d").to_string().parse::<u32>().unwrap_or(0);
    let month = now.format("%m").to_string().parse::<u32>().unwrap_or(0);
    let weekday = now.format("%u").to_string().parse::<u32>().unwrap_or(0); // 1=Mon, 7=Sun

    field_matches(parts[0], minute)
        && field_matches(parts[1], hour)
        && field_matches(parts[2], day)
        && field_matches(parts[3], month)
        && field_matches(parts[4], weekday)
}

fn field_matches(field: &str, value: u32) -> bool {
    if field == "*" {
        return true;
    }

    // Comma-separated values: "1,15,30"
    for part in field.split(',') {
        let part = part.trim();

        // Step values: "*/15" or "1-30/5"
        if let Some((range_part, step_str)) = part.split_once('/') {
            if let Ok(step) = step_str.parse::<u32>() {
                if step == 0 {
                    continue; // Avoid division by zero
                }
                if range_part == "*" {
                    // */N: matches when value is divisible by step
                    if value % step == 0 {
                        return true;
                    }
                } else if let Some((start, end)) = range_part.split_once('-') {
                    // start-end/step
                    if let (Ok(s), Ok(e)) = (start.parse::<u32>(), end.parse::<u32>()) {
                        if value >= s && value <= e && (value - s) % step == 0 {
                            return true;
                        }
                    }
                }
            }
            continue;
        }

        // Range: "1-5"
        if let Some((start, end)) = part.split_once('-') {
            if let (Ok(s), Ok(e)) = (start.parse::<u32>(), end.parse::<u32>()) {
                if value >= s && value <= e {
                    return true;
                }
            }
        } else if let Ok(v) = part.parse::<u32>() {
            // Exact match
            if v == value {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_schedule_every() {
        match parse_schedule("every:300") {
            ScheduleKind::Every(s) => assert_eq!(s, 300),
            _ => panic!("Expected Every"),
        }
    }

    #[test]
    fn test_parse_schedule_cron() {
        match parse_schedule("cron:0 9 * * *") {
            ScheduleKind::Cron(e) => assert_eq!(e, "0 9 * * *"),
            _ => panic!("Expected Cron"),
        }
    }

    #[test]
    fn test_parse_schedule_at() {
        match parse_schedule("at:2025-02-20T10:30:00") {
            ScheduleKind::At(t) => assert_eq!(t, "2025-02-20T10:30:00"),
            _ => panic!("Expected At"),
        }
    }

    #[test]
    fn test_parse_schedule_bare_cron() {
        match parse_schedule("0 9 * * *") {
            ScheduleKind::Cron(e) => assert_eq!(e, "0 9 * * *"),
            _ => panic!("Expected Cron"),
        }
    }

    #[test]
    fn test_parse_schedule_cron_trailing_wildcard_normalized() {
        match parse_schedule("cron:0 8 * * * *") {
            ScheduleKind::Cron(e) => assert_eq!(e, "0 8 * * *"),
            _ => panic!("Expected Cron"),
        }
    }

    #[test]
    fn test_parse_schedule_cron_leading_seconds_normalized() {
        match parse_schedule("cron:0 0 8 * * *") {
            ScheduleKind::Cron(e) => assert_eq!(e, "0 8 * * *"),
            _ => panic!("Expected Cron"),
        }
    }

    #[test]
    fn test_parse_schedule_bare_seconds() {
        match parse_schedule("600") {
            ScheduleKind::Every(s) => assert_eq!(s, 600),
            _ => panic!("Expected Every"),
        }
    }

    #[test]
    fn test_field_matches_star() {
        assert!(field_matches("*", 42));
    }

    #[test]
    fn test_field_matches_exact() {
        assert!(field_matches("9", 9));
        assert!(!field_matches("9", 10));
    }

    #[test]
    fn test_field_matches_list() {
        assert!(field_matches("1,5,10", 5));
        assert!(!field_matches("1,5,10", 6));
    }

    #[test]
    fn test_field_matches_range() {
        assert!(field_matches("1-5", 3));
        assert!(!field_matches("1-5", 6));
    }

    #[test]
    fn test_field_matches_step() {
        // */15: matches 0, 15, 30, 45
        assert!(field_matches("*/15", 0));
        assert!(field_matches("*/15", 15));
        assert!(field_matches("*/15", 30));
        assert!(field_matches("*/15", 45));
        assert!(!field_matches("*/15", 10));
        assert!(!field_matches("*/15", 7));
    }

    #[test]
    fn test_field_matches_range_step() {
        // 1-30/5: matches 1, 6, 11, 16, 21, 26
        assert!(field_matches("1-30/5", 1));
        assert!(field_matches("1-30/5", 6));
        assert!(field_matches("1-30/5", 11));
        assert!(!field_matches("1-30/5", 2));
        assert!(!field_matches("1-30/5", 31));
    }

    #[test]
    fn test_cron_matches_all_stars() {
        let now = chrono::Utc::now();
        assert!(cron_matches_now("* * * * *", &now));
    }

    #[test]
    fn test_cron_every_30_min() {
        // "*/30 * * * *" should match minute 0 and 30
        let t = chrono::NaiveDate::from_ymd_opt(2026, 3, 3)
            .unwrap()
            .and_hms_opt(10, 30, 0)
            .unwrap()
            .and_utc();
        assert!(cron_matches_now("*/30 * * * *", &t));

        let t2 = chrono::NaiveDate::from_ymd_opt(2026, 3, 3)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap()
            .and_utc();
        assert!(cron_matches_now("*/30 * * * *", &t2));

        let t3 = chrono::NaiveDate::from_ymd_opt(2026, 3, 3)
            .unwrap()
            .and_hms_opt(10, 15, 0)
            .unwrap()
            .and_utc();
        assert!(!cron_matches_now("*/30 * * * *", &t3));
    }

    #[test]
    fn test_cron_no_duplicate_fire_within_same_minute() {
        // Regression: scheduler checks every 30s, so the same cron minute
        // would match twice (e.g. 09:35:03 and 09:35:33).
        // should_fire_schedule must reject the second fire.
        let now = chrono::NaiveDate::from_ymd_opt(2026, 3, 18)
            .unwrap()
            .and_hms_opt(9, 35, 33)
            .unwrap()
            .and_utc();

        // last_run was 30 seconds ago (same minute) — must NOT fire again
        let last_run = "2026-03-18 09:35:03";
        assert!(!should_fire_schedule(
            "cron:35 9 * * *",
            Some(last_run),
            &now
        ));

        // last_run was yesterday — should fire
        let last_run_old = "2026-03-17 09:35:03";
        assert!(should_fire_schedule(
            "cron:35 9 * * *",
            Some(last_run_old),
            &now
        ));

        // no last_run — should fire (first time)
        assert!(should_fire_schedule("cron:35 9 * * *", None, &now));
    }

    #[test]
    fn test_is_schedule_overdue_every_missed() {
        // Schedule: every 3600s (hourly). Last ran 2 hours ago → overdue.
        let now = chrono::NaiveDate::from_ymd_opt(2026, 3, 21)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc();
        let last_run = "2026-03-21 10:00:00"; // 2 hours ago
        assert!(is_schedule_overdue("every:3600", Some(last_run), &now));
    }

    #[test]
    fn test_is_schedule_overdue_every_not_missed() {
        // Schedule: every 3600s. Last ran 30 min ago → not overdue.
        let now = chrono::NaiveDate::from_ymd_opt(2026, 3, 21)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc();
        let last_run = "2026-03-21 11:30:00"; // 30 min ago
        assert!(!is_schedule_overdue("every:3600", Some(last_run), &now));
    }

    #[test]
    fn test_is_schedule_overdue_cron_missed() {
        // Schedule: cron 0 9 * * * (daily at 9am). Last ran yesterday at 9am.
        // Now it's 11am today → overdue (missed today's 9am).
        let now = chrono::NaiveDate::from_ymd_opt(2026, 3, 21)
            .unwrap()
            .and_hms_opt(11, 0, 0)
            .unwrap()
            .and_utc();
        let last_run = "2026-03-20 09:00:00"; // yesterday
        assert!(is_schedule_overdue("cron:0 9 * * *", Some(last_run), &now));
    }

    #[test]
    fn test_is_schedule_overdue_no_last_run() {
        // No last_run → not overdue (first run will happen naturally).
        let now = chrono::Utc::now();
        assert!(!is_schedule_overdue("every:3600", None, &now));
    }
}
