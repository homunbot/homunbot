//! Global stop flag for cancelling long-running agent operations.
//!
//! This module provides a simple mechanism to request cancellation of
//! the current agent operation (browser automation, long tool execution, etc.)
//! from any part of the system (WebSocket, REST API, etc.).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// Global stop flag - set to true to request cancellation.
static STOP_REQUESTED: OnceLock<AtomicBool> = OnceLock::new();

fn get_stop_flag() -> &'static AtomicBool {
    STOP_REQUESTED.get_or_init(|| AtomicBool::new(false))
}

/// Request the agent to stop its current operation.
/// Called by the REST API when user clicks the "Stop" button.
pub fn request_stop() {
    get_stop_flag().store(true, Ordering::SeqCst);
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
