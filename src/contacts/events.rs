//! Contact event scanner — background task for birthday/anniversary notifications.
//!
//! Spawns a periodic checker that scans `contact_events` for upcoming dates
//! and fires notifications (+ optional auto-greetings) via the CronEvent channel.

use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{mpsc, RwLock};

use crate::config::Config;
use crate::scheduler::cron::CronEvent;
use crate::storage::Database;

/// Start the background event scanner.
///
/// Runs every 6 hours, scanning for contact events within each event's
/// `notify_days_before` window. Fires CronEvents for owner notification
/// and (optionally) auto-greetings.
pub fn start_event_scanner(
    db: Database,
    config: Arc<RwLock<Config>>,
    event_tx: mpsc::Sender<CronEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Initial delay: 60 seconds after startup
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(6 * 3600));
        interval.tick().await; // First tick is immediate

        loop {
            if let Err(e) = scan_and_notify(&db, &config, &event_tx).await {
                tracing::error!(error = %e, "Contact event scan failed");
            }
            interval.tick().await;
        }
    })
}

async fn scan_and_notify(
    db: &Database,
    config: &Arc<RwLock<Config>>,
    event_tx: &mpsc::Sender<CronEvent>,
) -> Result<()> {
    // Scan for events in the next 7 days (covers most notify_days_before values)
    let upcoming = db.load_upcoming_contact_events(7).await?;

    if upcoming.is_empty() {
        return Ok(());
    }

    tracing::info!(count = upcoming.len(), "Found upcoming contact events");

    for ue in &upcoming {
        let ev = &ue.event;

        // Notify owner
        let label = ev.label.as_deref().unwrap_or(&ev.event_type);
        let message = format!(
            "📅 Contact event: {} — {} ({})",
            ue.contact_name, label, ev.date,
        );

        let cron_event = CronEvent {
            kind: crate::scheduler::cron::ScheduledKind::Cron,
            job_id: format!("contact_event_{}", ev.id),
            job_name: format!("Contact: {} {}", ue.contact_name, label),
            message: message.clone(),
            deliver_to: None, // Deliver to owner's default channel
            automation_run_id: None,
        };

        if let Err(e) = event_tx.send(cron_event).await {
            tracing::warn!(error = %e, "Failed to send contact event notification");
        }

        // Auto-greet if configured
        if ev.auto_greet == 1 {
            let greeting = generate_greeting(config, &ue.contact_name, ev).await;
            let contact = db.load_contact(ev.contact_id).await.ok().flatten();
            let deliver_to = contact
                .and_then(|c| c.preferred_channel)
                .unwrap_or_default();

            if !deliver_to.is_empty() {
                let greet_event = CronEvent {
                    kind: crate::scheduler::cron::ScheduledKind::Cron,
                    job_id: format!("contact_greet_{}", ev.id),
                    job_name: format!("Auto-greet: {}", ue.contact_name),
                    message: greeting,
                    deliver_to: Some(deliver_to),
                    automation_run_id: None,
                };
                let _ = event_tx.send(greet_event).await;
            }
        }
    }

    Ok(())
}

async fn generate_greeting(
    config: &Arc<RwLock<Config>>,
    contact_name: &str,
    event: &crate::contacts::ContactEvent,
) -> String {
    let label = event.label.as_deref().unwrap_or(&event.event_type);

    // If there's a custom template, use it directly
    if let Some(tpl) = &event.greet_template {
        return tpl
            .replace("{name}", contact_name)
            .replace("{event}", label);
    }

    // Generate via LLM
    let config = config.read().await.clone();
    let req = crate::provider::one_shot::OneShotRequest {
        system_prompt: "Write a warm, short greeting message (1-2 sentences). \
            Be natural and heartfelt. Write in Italian unless the name suggests otherwise."
            .to_string(),
        user_message: format!(
            "Contact: {contact_name}\nEvent: {label}\nDate: {}",
            event.date
        ),
        max_tokens: 128,
        temperature: 0.7,
        timeout_secs: 15,
        ..Default::default()
    };

    match crate::provider::one_shot::llm_one_shot(&config, req).await {
        Ok(resp) => resp.content.trim().to_string(),
        Err(e) => {
            tracing::warn!(error = %e, "Failed to generate greeting, using fallback");
            format!("Auguri per il tuo {label}, {contact_name}! 🎉")
        }
    }
}

/// Pure function: check if an event date (MM-DD) falls within the next N days.
pub fn is_event_upcoming_mmdd(event_mmdd: &str, today: (u32, u32), days_ahead: u32) -> bool {
    let parts: Vec<&str> = event_mmdd.split('-').collect();
    if parts.len() != 2 {
        return false;
    }
    let (em, ed) = match (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
        (Ok(m), Ok(d)) => (m, d),
        _ => return false,
    };
    let (tm, td) = today;

    // Simple day-of-year comparison (approximate, ignores leap years)
    let event_doy = em * 31 + ed;
    let today_doy = tm * 31 + td;
    let end_doy = today_doy + days_ahead;

    // Handle year wrap-around
    if end_doy > 12 * 31 {
        event_doy >= today_doy || event_doy <= (end_doy - 12 * 31)
    } else {
        event_doy >= today_doy && event_doy <= end_doy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_event_upcoming_basic() {
        // March 18, event on March 21, 7 day window
        assert!(is_event_upcoming_mmdd("03-21", (3, 18), 7));
        // March 18, event on March 30, 7 day window — outside
        assert!(!is_event_upcoming_mmdd("03-30", (3, 18), 7));
        // March 18, event on March 18, 0 day window
        assert!(is_event_upcoming_mmdd("03-18", (3, 18), 0));
    }

    #[test]
    fn test_is_event_upcoming_year_wrap() {
        // Dec 28, event on Jan 2, 7 day window — should match
        assert!(is_event_upcoming_mmdd("01-02", (12, 28), 7));
        // Dec 28, event on Jan 10, 7 day window — outside
        assert!(!is_event_upcoming_mmdd("01-10", (12, 28), 7));
    }
}
