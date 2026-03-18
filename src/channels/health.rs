//! Channel health monitoring with circuit breaker.
//!
//! Mirrors the `ProviderHealthTracker` pattern: a fixed-size circular buffer
//! of outcomes per channel, with error-rate thresholds for Degraded / Down.
//! Adds channel-specific fields: started_at, restart_count, uptime.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Instant;

use chrono::Utc;
use serde::Serialize;

/// Number of recent events to track per channel.
const WINDOW_SIZE: usize = 20;
/// Error rate above which a channel is Degraded.
const DEGRADED_THRESHOLD: f64 = 0.5;
/// Error rate above which a channel is Down.
const DOWN_THRESHOLD: f64 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelStatus {
    Healthy,
    Degraded,
    Down,
    /// Channel is not running (disabled, crashed, or max restarts exceeded).
    Stopped,
}

/// Snapshot of channel health for API responses.
#[derive(Debug, Clone, Serialize)]
pub struct ChannelHealthSnapshot {
    pub name: String,
    pub status: ChannelStatus,
    pub enabled: bool,
    pub total_messages: u64,
    pub total_errors: u64,
    pub error_rate_recent: f64,
    pub last_error: Option<String>,
    pub last_error_at: Option<String>,
    pub started_at: Option<String>,
    pub restart_count: u32,
    pub uptime_secs: u64,
}

/// Tracks health metrics for all channels.
pub struct ChannelHealthTracker {
    metrics: RwLock<HashMap<String, ChannelMetrics>>,
}

struct ChannelMetrics {
    /// Circular buffer: true = success (message processed), false = error.
    outcomes: Vec<Option<bool>>,
    cursor: usize,
    total_messages: u64,
    total_errors: u64,
    last_error: Option<String>,
    last_error_at: Option<chrono::DateTime<Utc>>,
    enabled: bool,
    started_at: Option<Instant>,
    started_at_wall: Option<chrono::DateTime<Utc>>,
    restart_count: u32,
    running: bool,
}

impl ChannelMetrics {
    fn new() -> Self {
        Self {
            outcomes: vec![None; WINDOW_SIZE],
            cursor: 0,
            total_messages: 0,
            total_errors: 0,
            last_error: None,
            last_error_at: None,
            enabled: true,
            started_at: None,
            started_at_wall: None,
            restart_count: 0,
            running: false,
        }
    }

    fn record(&mut self, success: bool) {
        self.outcomes[self.cursor] = Some(success);
        self.cursor = (self.cursor + 1) % WINDOW_SIZE;
        if success {
            self.total_messages += 1;
        } else {
            self.total_errors += 1;
        }
    }

    fn error_rate(&self) -> f64 {
        let filled: Vec<bool> = self.outcomes.iter().filter_map(|o| *o).collect();
        if filled.is_empty() {
            return 0.0;
        }
        let errors = filled.iter().filter(|s| !**s).count();
        errors as f64 / filled.len() as f64
    }

    fn status(&self) -> ChannelStatus {
        if !self.running {
            return ChannelStatus::Stopped;
        }
        let rate = self.error_rate();
        if rate >= DOWN_THRESHOLD {
            ChannelStatus::Down
        } else if rate >= DEGRADED_THRESHOLD {
            ChannelStatus::Degraded
        } else {
            ChannelStatus::Healthy
        }
    }

    fn uptime_secs(&self) -> u64 {
        self.started_at.map(|s| s.elapsed().as_secs()).unwrap_or(0)
    }
}

impl ChannelHealthTracker {
    pub fn new() -> Self {
        Self {
            metrics: RwLock::new(HashMap::new()),
        }
    }

    fn get_or_create(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<String, ChannelMetrics>> {
        self.metrics.write().unwrap()
    }

    /// Mark a channel as started (or restarted).
    pub fn mark_started(&self, channel: &str) {
        let mut map = self.get_or_create();
        let m = map
            .entry(channel.to_string())
            .or_insert_with(ChannelMetrics::new);
        // If the channel was started before (even if currently stopped), this is a restart.
        if m.started_at_wall.is_some() {
            m.restart_count += 1;
        }
        m.running = true;
        m.started_at = Some(Instant::now());
        m.started_at_wall = Some(Utc::now());
    }

    /// Mark a channel as stopped (crashed or cleanly exited).
    pub fn mark_stopped(&self, channel: &str, error: Option<&str>) {
        let mut map = self.get_or_create();
        let m = map
            .entry(channel.to_string())
            .or_insert_with(ChannelMetrics::new);
        m.running = false;
        if let Some(err) = error {
            m.last_error = Some(err.to_string());
            m.last_error_at = Some(Utc::now());
            m.record(false);
        }
    }

    /// Set the enabled flag (from config).
    pub fn mark_enabled(&self, channel: &str, enabled: bool) {
        let mut map = self.get_or_create();
        let m = map
            .entry(channel.to_string())
            .or_insert_with(ChannelMetrics::new);
        m.enabled = enabled;
    }

    /// Record a successfully processed inbound message.
    pub fn record_message(&self, channel: &str) {
        let mut map = self.get_or_create();
        if let Some(m) = map.get_mut(channel) {
            m.record(true);
        }
    }

    /// Record a channel error (message delivery failure, API error, etc.).
    pub fn record_error(&self, channel: &str, error: &str) {
        let mut map = self.get_or_create();
        let m = map
            .entry(channel.to_string())
            .or_insert_with(ChannelMetrics::new);
        m.record(false);
        m.last_error = Some(error.to_string());
        m.last_error_at = Some(Utc::now());
    }

    /// Get the current status of a channel.
    pub fn status(&self, channel: &str) -> ChannelStatus {
        let map = self.metrics.read().unwrap();
        match map.get(channel) {
            Some(m) => m.status(),
            None => ChannelStatus::Stopped,
        }
    }

    /// Check if a channel is available (not Down or Stopped).
    pub fn is_available(&self, channel: &str) -> bool {
        let status = self.status(channel);
        status == ChannelStatus::Healthy || status == ChannelStatus::Degraded
    }

    /// Get a snapshot for a single channel.
    pub fn snapshot(&self, channel: &str) -> Option<ChannelHealthSnapshot> {
        let map = self.metrics.read().unwrap();
        map.get(channel).map(|m| Self::build_snapshot(channel, m))
    }

    /// Get health snapshots for all tracked channels.
    pub fn snapshots(&self) -> Vec<ChannelHealthSnapshot> {
        let map = self.metrics.read().unwrap();
        let mut result: Vec<_> = map
            .iter()
            .map(|(name, m)| Self::build_snapshot(name, m))
            .collect();
        result.sort_by(|a, b| a.name.cmp(&b.name));
        result
    }

    fn build_snapshot(name: &str, m: &ChannelMetrics) -> ChannelHealthSnapshot {
        ChannelHealthSnapshot {
            name: name.to_string(),
            status: m.status(),
            enabled: m.enabled,
            total_messages: m.total_messages,
            total_errors: m.total_errors,
            error_rate_recent: m.error_rate(),
            last_error: m.last_error.clone(),
            last_error_at: m.last_error_at.map(|t| t.to_rfc3339()),
            started_at: m.started_at_wall.map(|t| t.to_rfc3339()),
            restart_count: m.restart_count,
            uptime_secs: m.uptime_secs(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_channel() {
        let t = ChannelHealthTracker::new();
        t.mark_started("telegram");
        for _ in 0..5 {
            t.record_message("telegram");
        }
        assert_eq!(t.status("telegram"), ChannelStatus::Healthy);
        assert!(t.is_available("telegram"));
    }

    #[test]
    fn degraded_channel() {
        let t = ChannelHealthTracker::new();
        t.mark_started("slack");
        // 6 errors + 4 successes = 60% error rate > 50%
        for _ in 0..6 {
            t.record_error("slack", "api timeout");
        }
        for _ in 0..4 {
            t.record_message("slack");
        }
        assert_eq!(t.status("slack"), ChannelStatus::Degraded);
        assert!(t.is_available("slack")); // degraded != down
    }

    #[test]
    fn down_channel() {
        let t = ChannelHealthTracker::new();
        t.mark_started("discord");
        // 9 errors + 1 success = 90% > 80%
        for _ in 0..9 {
            t.record_error("discord", "ws closed");
        }
        t.record_message("discord");
        assert_eq!(t.status("discord"), ChannelStatus::Down);
        assert!(!t.is_available("discord"));
    }

    #[test]
    fn stopped_channel() {
        let t = ChannelHealthTracker::new();
        t.mark_started("email");
        t.mark_stopped("email", Some("IMAP timeout"));
        assert_eq!(t.status("email"), ChannelStatus::Stopped);
        assert!(!t.is_available("email"));
    }

    #[test]
    fn restart_tracking() {
        let t = ChannelHealthTracker::new();
        t.mark_started("telegram");
        assert_eq!(t.snapshot("telegram").unwrap().restart_count, 0);

        // Simulate crash + restart
        t.mark_stopped("telegram", Some("connection lost"));
        t.mark_started("telegram");
        assert_eq!(t.snapshot("telegram").unwrap().restart_count, 1);

        t.mark_stopped("telegram", Some("timeout"));
        t.mark_started("telegram");
        assert_eq!(t.snapshot("telegram").unwrap().restart_count, 2);
    }

    #[test]
    fn recovery_via_window() {
        let t = ChannelHealthTracker::new();
        t.mark_started("slack");
        // Fill window with errors → Down
        for _ in 0..WINDOW_SIZE {
            t.record_error("slack", "err");
        }
        assert_eq!(t.status("slack"), ChannelStatus::Down);

        // Fill with successes → Healthy
        for _ in 0..WINDOW_SIZE {
            t.record_message("slack");
        }
        assert_eq!(t.status("slack"), ChannelStatus::Healthy);
    }

    #[test]
    fn unknown_channel_is_stopped() {
        let t = ChannelHealthTracker::new();
        assert_eq!(t.status("nonexistent"), ChannelStatus::Stopped);
        assert!(!t.is_available("nonexistent"));
    }

    #[test]
    fn snapshots_sorted() {
        let t = ChannelHealthTracker::new();
        t.mark_started("telegram");
        t.mark_started("discord");
        t.mark_started("email");
        let snaps = t.snapshots();
        assert_eq!(snaps.len(), 3);
        assert_eq!(snaps[0].name, "discord");
        assert_eq!(snaps[1].name, "email");
        assert_eq!(snaps[2].name, "telegram");
    }
}
