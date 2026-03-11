//! Emergency Stop — hard kill switch for all agent activity.
//!
//! When activated:
//! 1. Sets the global stop flag (`request_stop()`)
//! 2. Takes the network offline (`set_network_online(false)`)
//! 3. Closes the browser via MCP peer
//! 4. Shuts down all MCP server connections
//! 5. Aborts all running subagent tasks
//!
//! The `/api/v1/emergency-stop` endpoint triggers this and returns a report.
//! The `/api/v1/resume` endpoint clears the stop flag and restores network.

use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::agent::stop;
use crate::utils::retry;

/// Handles to resources that need to be killed during emergency stop.
///
/// Populated at gateway startup. All fields are optional because some
/// features may not be enabled (no browser, no MCP, etc.).
#[derive(Default)]
pub struct EStopHandles {
    /// Browser session — call close to kill the browser.
    #[cfg(feature = "mcp")]
    pub browser_session: Option<Arc<crate::tools::BrowserSession>>,
    /// MCP manager — shutdown all server connections.
    #[cfg(feature = "mcp")]
    pub mcp_manager: Option<Arc<crate::tools::mcp::McpManager>>,
    /// Subagent manager — cancel all running background tasks.
    pub subagent_manager: Option<Arc<crate::agent::SubagentManager>>,
}

/// Report of what the emergency stop did.
#[derive(Debug, Serialize)]
pub struct EStopReport {
    pub stop_requested: bool,
    pub network_offline: bool,
    pub browser_closed: bool,
    pub mcp_shutdown: bool,
    pub subagents_cancelled: usize,
}

/// Execute emergency stop: kill everything.
pub async fn emergency_stop(handles: &RwLock<EStopHandles>) -> EStopReport {
    tracing::warn!("🚨 EMERGENCY STOP activated");

    // 1. Request agent loop stop
    stop::request_stop();

    // 2. Take network offline (prevents retries from reconnecting)
    retry::set_network_online(false);

    let h = handles.read().await;

    // 3. Close browser
    let mut browser_closed = false;
    #[cfg(feature = "mcp")]
    if let Some(ref session) = h.browser_session {
        // Force close regardless of idle state
        if session.close_if_idle(0).await {
            browser_closed = true;
            tracing::info!("Emergency stop: browser closed");
        }
    }

    // 4. Shutdown MCP servers
    let mut mcp_shutdown = false;
    #[cfg(feature = "mcp")]
    if let Some(ref manager) = h.mcp_manager {
        manager.shutdown().await;
        mcp_shutdown = true;
        tracing::info!("Emergency stop: MCP servers shut down");
    }

    // 5. Cancel subagents
    let subagents_cancelled = if let Some(ref manager) = h.subagent_manager {
        let cancelled = manager.cancel_all().await;
        if cancelled > 0 {
            tracing::info!(count = cancelled, "Emergency stop: subagents cancelled");
        }
        cancelled
    } else {
        0
    };

    let report = EStopReport {
        stop_requested: true,
        network_offline: true,
        browser_closed,
        mcp_shutdown,
        subagents_cancelled,
    };

    tracing::warn!(?report, "Emergency stop completed");
    report
}

/// Resume after emergency stop: clear stop flag and restore network.
pub fn resume() {
    stop::clear_stop();
    retry::set_network_online(true);
    tracing::info!("Resumed after emergency stop — network online, stop flag cleared");
}
