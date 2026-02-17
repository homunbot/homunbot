use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::sync::{mpsc, Mutex};

use crate::storage::{CronJobRow, Database};

/// Message sent when a cron job fires
#[derive(Debug, Clone)]
pub struct CronEvent {
    pub job_id: String,
    pub job_name: String,
    pub message: String,
    pub deliver_to: Option<String>,
}

/// Cron scheduler — manages recurring and one-shot jobs.
///
/// Architecture (following nanobot):
/// - Single timer checks all jobs each cycle
/// - Jobs are stored in SQLite, loaded at startup
/// - When a job fires, sends CronEvent through mpsc channel
/// - The gateway routes events to the agent loop
///
/// Schedule format:
/// - "every:300" → run every 300 seconds
/// - "cron:0 9 * * *" → standard cron expression (9 AM daily)
/// - "at:2025-02-20T10:30:00" → one-time execution
pub struct CronScheduler {
    db: Database,
    event_tx: mpsc::Sender<CronEvent>,
    jobs: Arc<Mutex<Vec<CronJobRow>>>,
}

impl CronScheduler {
    pub fn new(db: Database, event_tx: mpsc::Sender<CronEvent>) -> Self {
        Self {
            db,
            event_tx,
            jobs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Load jobs from DB and start the scheduler loop.
    /// Returns a JoinHandle for the background task.
    pub async fn start(self: Arc<Self>) -> Result<tokio::task::JoinHandle<()>> {
        // Load jobs from DB
        let db_jobs = self.db.load_cron_jobs().await?;
        let enabled_count = db_jobs.iter().filter(|j| j.enabled).count();

        {
            let mut jobs = self.jobs.lock().await;
            *jobs = db_jobs;
        }

        tracing::info!(
            total_jobs = enabled_count,
            "Cron scheduler loaded jobs"
        );

        let scheduler = self.clone();
        let handle = tokio::spawn(async move {
            scheduler.run_loop().await;
        });

        Ok(handle)
    }

    /// Main scheduler loop — checks jobs every 30 seconds
    async fn run_loop(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            interval.tick().await;
            if let Err(e) = self.check_and_fire().await {
                tracing::error!(error = %e, "Cron scheduler error");
            }
        }
    }

    /// Check all enabled jobs and fire any that are due
    async fn check_and_fire(&self) -> Result<()> {
        let jobs = self.jobs.lock().await;
        let now = chrono::Utc::now();

        for job in jobs.iter() {
            if !job.enabled {
                continue;
            }

            let should_fire = match parse_schedule(&job.schedule) {
                ScheduleKind::Every(secs) => {
                    // Check if enough time has passed since last run
                    match &job.last_run {
                        Some(last) => {
                            if let Ok(last_time) = chrono::NaiveDateTime::parse_from_str(last, "%Y-%m-%d %H:%M:%S") {
                                let last_utc = last_time.and_utc();
                                (now - last_utc).num_seconds() >= secs as i64
                            } else {
                                true // Can't parse last_run, fire
                            }
                        }
                        None => true, // Never run before
                    }
                }
                ScheduleKind::Cron(expr) => {
                    // Simple cron check: match current time against expression
                    cron_matches_now(&expr, &now)
                }
                ScheduleKind::At(target) => {
                    // One-time: fire if we're past the target time and never run
                    if let Ok(target_time) = chrono::NaiveDateTime::parse_from_str(&target, "%Y-%m-%dT%H:%M:%S") {
                        let target_utc = target_time.and_utc();
                        now >= target_utc && job.last_run.is_none()
                    } else {
                        false
                    }
                }
                ScheduleKind::Unknown => {
                    tracing::warn!(schedule = %job.schedule, "Unknown schedule format");
                    false
                }
            };

            if should_fire {
                tracing::info!(
                    job_id = %job.id,
                    job_name = %job.name,
                    "Cron job firing"
                );

                let event = CronEvent {
                    job_id: job.id.clone(),
                    job_name: job.name.clone(),
                    message: job.message.clone(),
                    deliver_to: job.deliver_to.clone(),
                };

                if let Err(e) = self.event_tx.send(event).await {
                    tracing::error!(error = %e, "Failed to send cron event");
                }

                // Update last_run in DB
                if let Err(e) = self.db.update_cron_last_run(&job.id).await {
                    tracing::error!(error = %e, "Failed to update cron job last_run");
                }
            }
        }

        Ok(())
    }

    /// Reload jobs from DB (call after add/remove)
    pub async fn reload(&self) -> Result<()> {
        let db_jobs = self.db.load_cron_jobs().await?;
        let mut jobs = self.jobs.lock().await;
        *jobs = db_jobs;
        Ok(())
    }

    /// Add a job to DB and reload
    pub async fn add_job(
        &self,
        name: &str,
        message: &str,
        schedule: &str,
        deliver_to: Option<&str>,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        self.db.insert_cron_job(&id, name, message, schedule, deliver_to).await?;
        self.reload().await?;
        tracing::info!(id = %id, name = %name, schedule = %schedule, "Cron job added");
        Ok(id)
    }

    /// Remove a job from DB and reload
    pub async fn remove_job(&self, id: &str) -> Result<bool> {
        let removed = self.db.delete_cron_job(id).await?;
        if removed {
            self.reload().await?;
            tracing::info!(id = %id, "Cron job removed");
        }
        Ok(removed)
    }

    /// List all jobs
    pub async fn list_jobs(&self) -> Vec<CronJobRow> {
        self.jobs.lock().await.clone()
    }
}

// --- Schedule parsing ---

enum ScheduleKind {
    Every(u64),     // seconds
    Cron(String),   // cron expression
    At(String),     // ISO timestamp
    Unknown,
}

fn parse_schedule(schedule: &str) -> ScheduleKind {
    if let Some(secs) = schedule.strip_prefix("every:") {
        if let Ok(s) = secs.trim().parse::<u64>() {
            return ScheduleKind::Every(s);
        }
    }

    if let Some(expr) = schedule.strip_prefix("cron:") {
        return ScheduleKind::Cron(expr.trim().to_string());
    }

    if let Some(ts) = schedule.strip_prefix("at:") {
        return ScheduleKind::At(ts.trim().to_string());
    }

    // Try to guess: if it looks like a cron expression (has spaces and numbers)
    let parts: Vec<&str> = schedule.split_whitespace().collect();
    if parts.len() == 5 {
        return ScheduleKind::Cron(schedule.to_string());
    }

    // Try as seconds
    if let Ok(s) = schedule.parse::<u64>() {
        return ScheduleKind::Every(s);
    }

    ScheduleKind::Unknown
}

/// Simple cron expression matching against current time.
/// Supports: minute hour day_of_month month day_of_week
/// Supports: * (any), specific numbers, comma-separated lists
fn cron_matches_now(expr: &str, now: &chrono::DateTime<chrono::Utc>) -> bool {
    let parts: Vec<&str> = expr.split_whitespace().collect();
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
    fn test_cron_matches_all_stars() {
        let now = chrono::Utc::now();
        assert!(cron_matches_now("* * * * *", &now));
    }
}
