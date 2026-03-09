//! Provider health monitoring with circuit breaker.
//!
//! Tracks per-provider request metrics in a fixed-size circular buffer.
//! Providers with high error rates are marked `Down` and skipped by
//! the `ReliableProvider` failover logic.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use serde::Serialize;

/// Number of recent requests to track per provider.
const WINDOW_SIZE: usize = 20;
/// Error rate above which a provider is considered Degraded.
const DEGRADED_THRESHOLD: f64 = 0.5;
/// Error rate above which a provider is considered Down.
const DOWN_THRESHOLD: f64 = 0.8;
/// EMA smoothing factor for latency (0.0–1.0, higher = more weight on recent).
const LATENCY_ALPHA: f64 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderStatus {
    Healthy,
    Degraded,
    Down,
}

/// Snapshot of provider health for API responses.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderHealthSnapshot {
    pub name: String,
    pub status: ProviderStatus,
    pub total_requests: u64,
    pub total_errors: u64,
    pub error_rate_recent: f64,
    pub avg_latency_ms: f64,
    pub last_error_msg: Option<String>,
}

/// Tracks health metrics for all providers.
pub struct ProviderHealthTracker {
    metrics: RwLock<HashMap<String, ProviderMetrics>>,
}

struct ProviderMetrics {
    /// Circular buffer: true = success, false = error.
    outcomes: Vec<Option<bool>>,
    cursor: usize,
    total_requests: u64,
    total_errors: u64,
    avg_latency_ms: f64,
    last_error_msg: Option<String>,
}

impl ProviderMetrics {
    fn new() -> Self {
        Self {
            outcomes: vec![None; WINDOW_SIZE],
            cursor: 0,
            total_requests: 0,
            total_errors: 0,
            avg_latency_ms: 0.0,
            last_error_msg: None,
        }
    }

    fn record(&mut self, success: bool, latency: Duration) {
        self.outcomes[self.cursor] = Some(success);
        self.cursor = (self.cursor + 1) % WINDOW_SIZE;
        self.total_requests += 1;
        if !success {
            self.total_errors += 1;
        }
        let ms = latency.as_millis() as f64;
        if self.total_requests == 1 {
            self.avg_latency_ms = ms;
        } else {
            self.avg_latency_ms = LATENCY_ALPHA * ms + (1.0 - LATENCY_ALPHA) * self.avg_latency_ms;
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

    fn status(&self) -> ProviderStatus {
        let rate = self.error_rate();
        if rate >= DOWN_THRESHOLD {
            ProviderStatus::Down
        } else if rate >= DEGRADED_THRESHOLD {
            ProviderStatus::Degraded
        } else {
            ProviderStatus::Healthy
        }
    }
}

impl ProviderHealthTracker {
    pub fn new() -> Self {
        Self {
            metrics: RwLock::new(HashMap::new()),
        }
    }

    /// Record a successful request.
    pub fn record_success(&self, provider: &str, latency: Duration) {
        let mut map = self.metrics.write().unwrap();
        let m = map
            .entry(provider.to_string())
            .or_insert_with(ProviderMetrics::new);
        m.record(true, latency);
    }

    /// Record a failed request.
    pub fn record_error(&self, provider: &str, latency: Duration, error: &str) {
        let mut map = self.metrics.write().unwrap();
        let m = map
            .entry(provider.to_string())
            .or_insert_with(ProviderMetrics::new);
        m.record(false, latency);
        m.last_error_msg = Some(error.to_string());
    }

    /// Check if a provider is available (not Down).
    pub fn is_available(&self, provider: &str) -> bool {
        let map = self.metrics.read().unwrap();
        match map.get(provider) {
            Some(m) => m.status() != ProviderStatus::Down,
            None => true, // unknown provider = available
        }
    }

    /// Get the current status of a provider.
    pub fn status(&self, provider: &str) -> ProviderStatus {
        let map = self.metrics.read().unwrap();
        match map.get(provider) {
            Some(m) => m.status(),
            None => ProviderStatus::Healthy,
        }
    }

    /// Get health snapshots for all tracked providers.
    pub fn snapshots(&self) -> Vec<ProviderHealthSnapshot> {
        let map = self.metrics.read().unwrap();
        let mut result: Vec<_> = map
            .iter()
            .map(|(name, m)| ProviderHealthSnapshot {
                name: name.clone(),
                status: m.status(),
                total_requests: m.total_requests,
                total_errors: m.total_errors,
                error_rate_recent: m.error_rate(),
                avg_latency_ms: m.avg_latency_ms,
                last_error_msg: m.last_error_msg.clone(),
            })
            .collect();
        result.sort_by(|a, b| a.name.cmp(&b.name));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_healthy_provider() {
        let tracker = ProviderHealthTracker::new();
        for _ in 0..5 {
            tracker.record_success("openai", Duration::from_millis(100));
        }
        assert_eq!(tracker.status("openai"), ProviderStatus::Healthy);
        assert!(tracker.is_available("openai"));
    }

    #[test]
    fn test_degraded_provider() {
        let tracker = ProviderHealthTracker::new();
        // 6 errors, 4 successes = 60% error rate > 50% threshold
        for _ in 0..6 {
            tracker.record_error("openai", Duration::from_millis(100), "rate limit");
        }
        for _ in 0..4 {
            tracker.record_success("openai", Duration::from_millis(100));
        }
        assert_eq!(tracker.status("openai"), ProviderStatus::Degraded);
        assert!(tracker.is_available("openai")); // degraded != down
    }

    #[test]
    fn test_down_provider() {
        let tracker = ProviderHealthTracker::new();
        // 9 errors, 1 success = 90% error rate > 80% threshold
        for _ in 0..9 {
            tracker.record_error("openai", Duration::from_millis(100), "server error");
        }
        tracker.record_success("openai", Duration::from_millis(100));
        assert_eq!(tracker.status("openai"), ProviderStatus::Down);
        assert!(!tracker.is_available("openai"));
    }

    #[test]
    fn test_recovery_via_window() {
        let tracker = ProviderHealthTracker::new();
        // Fill window with errors → Down
        for _ in 0..WINDOW_SIZE {
            tracker.record_error("openai", Duration::from_millis(100), "err");
        }
        assert_eq!(tracker.status("openai"), ProviderStatus::Down);

        // Now fill with successes → should recover
        for _ in 0..WINDOW_SIZE {
            tracker.record_success("openai", Duration::from_millis(50));
        }
        assert_eq!(tracker.status("openai"), ProviderStatus::Healthy);
    }

    #[test]
    fn test_unknown_provider_is_available() {
        let tracker = ProviderHealthTracker::new();
        assert!(tracker.is_available("nonexistent"));
        assert_eq!(tracker.status("nonexistent"), ProviderStatus::Healthy);
    }

    #[test]
    fn test_snapshots() {
        let tracker = ProviderHealthTracker::new();
        tracker.record_success("a", Duration::from_millis(100));
        tracker.record_error("b", Duration::from_millis(200), "err");
        let snaps = tracker.snapshots();
        assert_eq!(snaps.len(), 2);
        assert_eq!(snaps[0].name, "a");
        assert_eq!(snaps[1].name, "b");
    }
}
