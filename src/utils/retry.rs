//! Retry utilities with exponential backoff and jitter.
//!
//! Provides a generic retry mechanism for network operations that:
//! - Uses exponential backoff with configurable jitter
//! - Supports custom retry conditions
//! - Tracks network connectivity state
//! - Logs retry attempts with context
//!
//! # Example
//! ```rust
//! use homun::utils::retry::{retry_with_backoff, RetryConfig};
//!
//! let config = RetryConfig::default();
//! let result = retry_with_backoff(
//!     || async { some_network_call().await },
//!     &config,
//!     "fetch_data",
//! ).await;
//! ```

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

/// Global network connectivity state.
/// Shared across all components to avoid redundant connectivity checks.
static NETWORK_ONLINE: AtomicBool = AtomicBool::new(true);

/// Check if we believe the network is online.
pub fn is_network_online() -> bool {
    NETWORK_ONLINE.load(Ordering::Relaxed)
}

/// Update the global network state.
pub fn set_network_online(online: bool) {
    let was_online = NETWORK_ONLINE.swap(online, Ordering::Relaxed);
    if was_online != online {
        if online {
            tracing::info!("Network connectivity restored");
        } else {
            tracing::warn!("Network connectivity lost");
        }
    }
}

/// Configuration for retry behavior.
#[derive(Clone, Debug)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (0 = no retries, just try once)
    pub max_retries: u32,
    /// Initial delay before first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries (caps exponential growth)
    pub max_delay: Duration,
    /// Multiplier for exponential backoff (e.g., 2.0 doubles each time)
    pub multiplier: f64,
    /// Jitter factor (0.0 = no jitter, 1.0 = full jitter)
    /// Adds randomness to prevent thundering herd
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            jitter_factor: 0.3,
        }
    }
}

impl RetryConfig {
    /// Create a config for quick operations (fast retries)
    pub fn fast() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            multiplier: 2.0,
            jitter_factor: 0.2,
        }
    }

    /// Create a config for slow operations (patient retries)
    pub fn patient() -> Self {
        Self {
            max_retries: 10,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(120),
            multiplier: 1.5,
            jitter_factor: 0.3,
        }
    }

    /// Create a config for critical operations (many retries)
    pub fn persistent() -> Self {
        Self {
            max_retries: 20,
            initial_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(300),
            multiplier: 1.3,
            jitter_factor: 0.4,
        }
    }

    /// Calculate delay for a given attempt number (0-indexed)
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        self.delay_for_attempt_with_jitter(attempt, Instant::now().elapsed().as_nanos() as u64)
    }

    /// Calculate delay with a seed for deterministic jitter (useful for testing)
    pub fn delay_for_attempt_with_jitter(&self, attempt: u32, seed: u64) -> Duration {
        let base_delay = self.initial_delay.as_secs_f64()
            * self.multiplier.powi(attempt as i32);

        let capped_delay = base_delay.min(self.max_delay.as_secs_f64());

        // Add jitter using simple deterministic pseudo-random based on seed
        // This avoids needing the rand crate while still providing jitter
        let jitter = if self.jitter_factor > 0.0 {
            let jitter_range = capped_delay * self.jitter_factor;
            // Simple hash-like computation for deterministic "randomness"
            let pseudo_random = ((seed.wrapping_mul(attempt as u64 + 1)) % 1000) as f64 / 1000.0;
            let jitter_amount = (pseudo_random - 0.5) * 2.0 * jitter_range; // -jitter_range to +jitter_range
            jitter_amount
        } else {
            0.0
        };

        let final_delay = (capped_delay + jitter).max(0.0);
        Duration::from_secs_f64(final_delay)
    }
}

/// Error classification for retry decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Retry the operation
    Retry,
    /// Don't retry, fail immediately
    Fail,
    /// Wait for network to come back, then retry
    WaitForNetwork,
}

/// Trait for classifying errors for retry decisions.
pub trait RetryableError: std::fmt::Display {
    /// Determine if this error should trigger a retry.
    fn retry_decision(&self) -> RetryDecision {
        let err_str = self.to_string().to_lowercase();

        // Network-related errors that indicate connectivity issues
        if err_str.contains("timeout")
            || err_str.contains("connection")
            || err_str.contains("network")
            || err_str.contains("dns")
            || err_str.contains("socket")
            || err_str.contains("refused")
            || err_str.contains("unreachable")
            || err_str.contains("timed out")
            || err_str.contains("broken pipe")
            || err_str.contains("reset")
        {
            RetryDecision::WaitForNetwork
        }
        // Rate limiting - should retry with backoff
        else if err_str.contains("429")
            || err_str.contains("rate limit")
            || err_str.contains("too many requests")
        {
            RetryDecision::Retry
        }
        // Server errors - might be temporary
        else if err_str.contains("500")
            || err_str.contains("502")
            || err_str.contains("503")
            || err_str.contains("504")
            || err_str.contains("internal server error")
            || err_str.contains("bad gateway")
            || err_str.contains("service unavailable")
            || err_str.contains("gateway timeout")
        {
            RetryDecision::Retry
        }
        // Client errors - don't retry (bad request, auth, etc.)
        else if err_str.contains("400")
            || err_str.contains("401")
            || err_str.contains("403")
            || err_str.contains("404")
            || err_str.contains("invalid")
            || err_str.contains("unauthorized")
            || err_str.contains("forbidden")
            || err_str.contains("not found")
        {
            RetryDecision::Fail
        }
        // Unknown errors - be conservative and retry
        else {
            RetryDecision::Retry
        }
    }
}

// Blanket implementation for any Error type
impl<T: std::fmt::Display> RetryableError for T {}

/// Execute an async operation with retry and exponential backoff.
///
/// # Arguments
/// * `operation` - The async operation to execute
/// * `config` - Retry configuration
/// * `operation_name` - Name for logging purposes
///
/// # Returns
/// The result of the operation, or the last error after all retries exhausted.
pub async fn retry_with_backoff<F, Fut, T>(
    operation: F,
    config: &RetryConfig,
    operation_name: &str,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut attempt = 0u32;

    loop {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    tracing::info!(
                        operation = %operation_name,
                        attempts = attempt + 1,
                        "Operation succeeded after retry"
                    );
                    set_network_online(true);
                }
                return Ok(result);
            }
            Err(e) => {
                let decision = e.to_string().retry_decision();

                match decision {
                    RetryDecision::Fail => {
                        tracing::debug!(
                            operation = %operation_name,
                            error = %e,
                            "Non-retryable error, failing immediately"
                        );
                        return Err(e.context(format!(
                            "{} failed with non-retryable error",
                            operation_name
                        )));
                    }
                    RetryDecision::WaitForNetwork => {
                        set_network_online(false);
                    }
                    RetryDecision::Retry => {
                        // Network state unchanged
                    }
                }

                if attempt >= config.max_retries {
                    tracing::warn!(
                        operation = %operation_name,
                        attempts = attempt + 1,
                        max_retries = config.max_retries,
                        error = %e,
                        "Operation failed after all retries"
                    );
                    return Err(e.context(format!(
                        "{} failed after {} retries",
                        operation_name,
                        attempt + 1
                    )));
                }

                let delay = config.delay_for_attempt(attempt);

                tracing::warn!(
                    operation = %operation_name,
                    attempt = attempt + 1,
                    max_retries = config.max_retries,
                    delay_ms = delay.as_millis(),
                    error = %e,
                    "Operation failed, retrying..."
                );

                tokio::time::sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

/// Execute an async operation with retry, using a custom retry condition.
///
/// # Arguments
/// * `operation` - The async operation to execute
/// * `config` - Retry configuration
/// * `operation_name` - Name for logging purposes
/// * `should_retry` - Custom function to determine if retry should happen
pub async fn retry_with_condition<F, Fut, T, P>(
    operation: F,
    config: &RetryConfig,
    operation_name: &str,
    should_retry: P,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
    P: Fn(&anyhow::Error) -> bool,
{
    let mut attempt = 0u32;

    loop {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    tracing::info!(
                        operation = %operation_name,
                        attempts = attempt + 1,
                        "Operation succeeded after retry"
                    );
                }
                return Ok(result);
            }
            Err(e) => {
                if !should_retry(&e) {
                    return Err(e);
                }

                if attempt >= config.max_retries {
                    return Err(e.context(format!(
                        "{} failed after {} retries",
                        operation_name,
                        attempt + 1
                    )));
                }

                let delay = config.delay_for_attempt(attempt);
                tracing::warn!(
                    operation = %operation_name,
                    attempt = attempt + 1,
                    delay_ms = delay.as_millis(),
                    "Retrying after error"
                );

                tokio::time::sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

/// A wrapper that adds retry capability to any async operation.
pub struct RetryWrapper<F> {
    operation: F,
    config: RetryConfig,
    name: String,
}

impl<F> RetryWrapper<F> {
    pub fn new(operation: F, config: RetryConfig, name: impl Into<String>) -> Self {
        Self {
            operation,
            config,
            name: name.into(),
        }
    }
}

impl<F, Fut, T> RetryWrapper<F>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    pub async fn execute(self) -> Result<T> {
        retry_with_backoff(self.operation, &self.config, &self.name).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delay_calculation() {
        let config = RetryConfig::default();

        // First attempt should be close to initial_delay
        let d0 = config.delay_for_attempt(0);
        assert!(d0 >= Duration::from_millis(200));
        assert!(d0 <= Duration::from_millis(800));

        // Second attempt should be roughly double
        let d1 = config.delay_for_attempt(1);
        assert!(d1 >= Duration::from_millis(500));

        // Should cap at max_delay
        let d10 = config.delay_for_attempt(10);
        assert!(d10 <= config.max_delay);
    }

    #[test]
    fn test_fast_config() {
        let config = RetryConfig::fast();
        assert_eq!(config.max_retries, 3);
        assert!(config.initial_delay < Duration::from_secs(1));
    }

    #[test]
    fn test_patient_config() {
        let config = RetryConfig::patient();
        assert_eq!(config.max_retries, 10);
        assert!(config.max_delay >= Duration::from_secs(60));
    }

    #[test]
    fn test_retry_decision_timeout() {
        let decision = "Connection timed out".retry_decision();
        assert_eq!(decision, RetryDecision::WaitForNetwork);
    }

    #[test]
    fn test_retry_decision_rate_limit() {
        let decision = "Error 429: Too many requests".retry_decision();
        assert_eq!(decision, RetryDecision::Retry);
    }

    #[test]
    fn test_retry_decision_client_error() {
        let decision = "Error 404: Not found".retry_decision();
        assert_eq!(decision, RetryDecision::Fail);
    }

    #[test]
    fn test_retry_decision_server_error() {
        let decision = "Error 503: Service unavailable".retry_decision();
        assert_eq!(decision, RetryDecision::Retry);
    }

    #[tokio::test]
    async fn test_retry_success_on_second_attempt() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_with_backoff(
            move || {
                let attempts = attempts_clone.clone();
                async move {
                    let count = attempts.fetch_add(1, Ordering::SeqCst);
                    if count == 0 {
                        Err(anyhow::anyhow!("First attempt failed"))
                    } else {
                        Ok("success")
                    }
                }
            },
            &RetryConfig::fast(),
            "test_operation",
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let result = retry_with_backoff(
            || async { Err::<(), _>(anyhow::anyhow!("Always fails")) },
            &RetryConfig {
                max_retries: 2,
                initial_delay: Duration::from_millis(10),
                ..Default::default()
            },
            "test_operation",
        )
        .await;

        assert!(result.is_err());
        // With max_retries: 2, we have 3 total attempts (1 initial + 2 retries)
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("3 retries"), "Error message was: {}", err_msg);
    }

    #[tokio::test]
    async fn test_non_retryable_error() {
        let result = retry_with_backoff(
            || async { Err::<(), _>(anyhow::anyhow!("Error 404: Not found")) },
            &RetryConfig::default(),
            "test_operation",
        )
        .await;

        assert!(result.is_err());
        // Should fail immediately without retries
        let err = result.unwrap_err().to_string();
        assert!(err.contains("non-retryable"));
    }

    #[test]
    fn test_network_state() {
        set_network_online(true);
        assert!(is_network_online());

        set_network_online(false);
        assert!(!is_network_online());

        set_network_online(true); // Reset for other tests
    }
}
