//! Global stop flag for cancelling long-running agent operations.
//!
//! This module provides a simple mechanism to request cancellation of
//! the current agent operation (browser automation, long tool execution, etc.)
//! from any part of the system (WebSocket, REST API, etc.).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use tokio::sync::Notify;

/// Global stop flag - set to true to request cancellation.
static STOP_REQUESTED: OnceLock<AtomicBool> = OnceLock::new();
static STOP_NOTIFY: OnceLock<Notify> = OnceLock::new();

fn get_stop_flag() -> &'static AtomicBool {
    STOP_REQUESTED.get_or_init(|| AtomicBool::new(false))
}

fn get_stop_notify() -> &'static Notify {
    STOP_NOTIFY.get_or_init(Notify::new)
}

/// Request the agent to stop its current operation.
/// Called by the REST API when user clicks the "Stop" button.
pub fn request_stop() {
    get_stop_flag().store(true, Ordering::SeqCst);
    get_stop_notify().notify_waiters();
    tracing::info!("Stop requested for current agent operation");
}

/// Check if a stop has been requested.
/// Called by the agent loop and tools to check for cancellation.
pub fn is_stop_requested() -> bool {
    get_stop_flag().load(Ordering::SeqCst)
}

/// Clear the stop flag (call when starting a new operation).
/// Called when a new message arrives to reset the cancellation state.
pub fn clear_stop() {
    get_stop_flag().store(false, Ordering::SeqCst);
}

/// Wait until a stop has been requested.
pub async fn wait_for_stop() {
    if is_stop_requested() {
        return;
    }
    loop {
        get_stop_notify().notified().await;
        if is_stop_requested() {
            return;
        }
    }
}

/// Convert the stop flag into a standard cancellation error.
pub fn cancellation_error() -> anyhow::Error {
    anyhow::anyhow!("Stopped by user")
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn wait_for_stop_resolves_after_request() {
        super::clear_stop();
        let waiter = tokio::spawn(async { super::wait_for_stop().await });
        tokio::task::yield_now().await;
        super::request_stop();
        waiter.await.expect("waiter join");
        super::clear_stop();
    }
}
