//! Per-conversation browser tab session management.
//!
//! Each conversation gets its own browser tab for isolation.
//! [`TabSessionManager`] maps session keys to [`TabSession`] instances
//! and handles tab lifecycle (create, select, close, index adjustment).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::{json, Value};
use tokio::sync::{Mutex, RwLock};

use crate::tools::mcp::McpPeer;

// ─── Per-conversation tab state ─────────────────────────────────

/// State for a single conversation's browser tab.
pub struct TabSession {
    /// Current tab index in Playwright's tab list.
    /// `None` before the tab is created.
    pub tab_index: RwLock<Option<usize>>,
    /// Last navigated URL (for continuation hints).
    pub last_url: RwLock<Option<String>>,
    /// Timestamp of the last browser action (for idle cleanup).
    pub last_action_at: RwLock<Option<Instant>>,
    /// Consecutive snapshot guard (prevents duplicate snapshots).
    pub last_was_snapshot: AtomicBool,
}

impl TabSession {
    fn new() -> Self {
        Self {
            tab_index: RwLock::new(None),
            last_url: RwLock::new(None),
            last_action_at: RwLock::new(None),
            last_was_snapshot: AtomicBool::new(false),
        }
    }

    /// Record that an action was performed, updating timestamp and optional URL.
    pub async fn note_action(&self, url: Option<&str>) {
        *self.last_action_at.write().await = Some(Instant::now());
        if let Some(u) = url {
            *self.last_url.write().await = Some(u.to_string());
        }
    }

    /// Check if this tab has been idle longer than `timeout`.
    pub async fn is_idle(&self, timeout: Duration) -> bool {
        match *self.last_action_at.read().await {
            Some(t) => t.elapsed() > timeout,
            None => false, // never used → not idle
        }
    }

    /// Returns true if the tab has been created (has an index).
    pub async fn is_active(&self) -> bool {
        self.tab_index.read().await.is_some()
    }
}

// ─── Manager ────────────────────────────────────────────────────

/// Manages browser tab sessions across conversations.
///
/// Thread-safe: the inner `sessions` map is behind a [`RwLock`].
/// Callers must hold the `operation_mutex` from [`BrowserTool`] when
/// performing tab mutations (create, close) to keep indices consistent.
pub struct TabSessionManager {
    sessions: RwLock<HashMap<String, Arc<TabSession>>>,
}

impl TabSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create a tab session for the given session key.
    ///
    /// If this is the first browser action for this conversation,
    /// a new tab is opened via `browser_tabs(new)`.
    /// The caller must hold the `operation_mutex`.
    pub async fn get_or_create(
        &self,
        session_key: &str,
        peer: &McpPeer,
    ) -> Result<Arc<TabSession>> {
        // Fast path: session already exists with an active tab
        {
            let sessions = self.sessions.read().await;
            if let Some(tab) = sessions.get(session_key) {
                if tab.is_active().await {
                    return Ok(Arc::clone(tab));
                }
            }
        }

        // Slow path: create a new tab
        let tab = {
            let mut sessions = self.sessions.write().await;
            let tab = sessions
                .entry(session_key.to_string())
                .or_insert_with(|| Arc::new(TabSession::new()));
            Arc::clone(tab)
        };

        // Only create if not yet active (another task may have raced)
        if !tab.is_active().await {
            let result = peer
                .call_tool("browser_tabs", json!({"action": "new"}))
                .await?;

            // Parse the new tab index from the response.
            // Playwright MCP returns tab info; we need the index.
            let new_index = parse_new_tab_index(&result).await;
            *tab.tab_index.write().await = Some(new_index);
            tracing::debug!(
                session_key,
                tab_index = new_index,
                "Created browser tab for session"
            );
        }

        Ok(tab)
    }

    /// Close and remove a session's browser tab.
    ///
    /// Adjusts indices of all remaining sessions with higher indices.
    /// The caller must hold the `operation_mutex`.
    pub async fn close_session(&self, session_key: &str, peer: &McpPeer) {
        let tab_index = {
            let sessions = self.sessions.read().await;
            match sessions.get(session_key) {
                Some(tab) => *tab.tab_index.read().await,
                None => return,
            }
        };

        if let Some(index) = tab_index {
            // Don't close the last remaining tab (Playwright needs at least one)
            let tab_count = self.active_tab_count().await;
            if tab_count <= 1 {
                // Just clear the session but keep the tab
                let mut sessions = self.sessions.write().await;
                sessions.remove(session_key);
                tracing::debug!(session_key, "Kept last browser tab open, removed session");
                return;
            }

            // Close the tab via MCP
            if let Err(e) = peer
                .call_tool("browser_tabs", json!({"action": "close", "index": index}))
                .await
            {
                tracing::warn!(
                    session_key,
                    tab_index = index,
                    error = %e,
                    "Failed to close browser tab"
                );
            }

            // Adjust indices for all sessions with index > closed
            self.adjust_indices_after_close(index).await;
        }

        // Remove from map
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_key);
        tracing::debug!(session_key, "Closed browser tab for session");
    }

    /// Close all tabs that have been idle longer than `timeout`.
    /// The caller must hold the `operation_mutex`.
    pub async fn close_idle_tabs(&self, timeout: Duration, peer: &McpPeer) {
        let idle_keys: Vec<String> = {
            let sessions = self.sessions.read().await;
            let mut keys = Vec::new();
            for (key, tab) in sessions.iter() {
                if tab.is_idle(timeout).await {
                    keys.push(key.clone());
                }
            }
            keys
        };

        for key in idle_keys {
            tracing::info!(session_key = %key, "Closing idle browser tab");
            self.close_session(&key, peer).await;
        }
    }

    /// Generate a continuation hint for a specific session.
    /// Returns `None` if the session has no active browser tab.
    pub async fn continuation_hint_for(&self, session_key: &str) -> Option<String> {
        let sessions = self.sessions.read().await;
        let tab = sessions.get(session_key)?;

        if !tab.is_active().await {
            return None;
        }

        let url = tab.last_url.read().await;
        let url_str = url.as_deref()?;

        Some(format!(
            "Browser is still open on: {url_str}\n\
             You can continue from the current page — call snapshot() to see it.\n\
             Do NOT navigate again to the same site unless you need a different page."
        ))
    }

    /// Check if any session has an active browser tab.
    pub async fn has_any_active(&self) -> bool {
        let sessions = self.sessions.read().await;
        for tab in sessions.values() {
            if tab.is_active().await {
                return true;
            }
        }
        false
    }

    /// Count of active tabs across all sessions.
    async fn active_tab_count(&self) -> usize {
        let sessions = self.sessions.read().await;
        let mut count = 0;
        for tab in sessions.values() {
            if tab.is_active().await {
                count += 1;
            }
        }
        count
    }

    /// After closing a tab at `closed_index`, decrement all sessions
    /// whose tab index was higher (indices shift down).
    async fn adjust_indices_after_close(&self, closed_index: usize) {
        let sessions = self.sessions.read().await;
        for tab in sessions.values() {
            let mut idx = tab.tab_index.write().await;
            if let Some(ref mut i) = *idx {
                if *i > closed_index {
                    *i -= 1;
                }
            }
        }
    }
}

/// Parse the tab index from a `browser_tabs(new)` response.
///
/// Playwright MCP returns text like:
///   "Opened new tab\n- [0] (current) about:blank\n- [1] (current) about:blank"
/// We take the highest index as the newly created tab.
async fn parse_new_tab_index(response: &str) -> usize {
    let mut max_index: usize = 0;
    for line in response.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("- [") {
            if let Some(idx_end) = rest.find(']') {
                if let Ok(idx) = rest[..idx_end].parse::<usize>() {
                    max_index = max_index.max(idx);
                }
            }
        }
    }
    max_index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_new_tab_index_single() {
        let response = "Opened new tab\n- [0] (current) about:blank";
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(parse_new_tab_index(response));
        assert_eq!(result, 0);
    }

    #[test]
    fn test_parse_new_tab_index_multiple() {
        let response =
            "Opened new tab\n- [0] https://google.com\n- [1] about:blank\n- [2] (current) about:blank";
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(parse_new_tab_index(response));
        assert_eq!(result, 2);
    }

    #[test]
    fn test_parse_new_tab_index_empty() {
        let response = "No tabs";
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(parse_new_tab_index(response));
        assert_eq!(result, 0);
    }

    #[tokio::test]
    async fn test_tab_session_idle() {
        let tab = TabSession::new();
        assert!(!tab.is_idle(Duration::from_secs(5)).await); // never used
        tab.note_action(Some("https://example.com")).await;
        assert!(!tab.is_idle(Duration::from_secs(300)).await); // just used
        assert_eq!(
            tab.last_url.read().await.as_deref(),
            Some("https://example.com")
        );
    }

    #[tokio::test]
    async fn test_tab_session_manager_adjust_indices() {
        let manager = TabSessionManager::new();

        // Manually insert sessions with known indices
        {
            let mut sessions = manager.sessions.write().await;
            let tab_a = Arc::new(TabSession::new());
            *tab_a.tab_index.write().await = Some(0);
            sessions.insert("a".to_string(), tab_a);

            let tab_b = Arc::new(TabSession::new());
            *tab_b.tab_index.write().await = Some(1);
            sessions.insert("b".to_string(), tab_b);

            let tab_c = Arc::new(TabSession::new());
            *tab_c.tab_index.write().await = Some(2);
            sessions.insert("c".to_string(), tab_c);
        }

        // Close tab at index 1 → tab C should shift from 2 to 1
        manager.adjust_indices_after_close(1).await;

        let sessions = manager.sessions.read().await;
        assert_eq!(*sessions["a"].tab_index.read().await, Some(0));
        assert_eq!(*sessions["b"].tab_index.read().await, Some(1)); // was 1, not > 1
        assert_eq!(*sessions["c"].tab_index.read().await, Some(1)); // was 2, now 1
    }

    #[tokio::test]
    async fn test_continuation_hint_no_session() {
        let manager = TabSessionManager::new();
        assert!(manager.continuation_hint_for("missing").await.is_none());
    }

    #[tokio::test]
    async fn test_continuation_hint_no_url() {
        let manager = TabSessionManager::new();
        {
            let mut sessions = manager.sessions.write().await;
            let tab = Arc::new(TabSession::new());
            *tab.tab_index.write().await = Some(0);
            sessions.insert("test".to_string(), tab);
        }
        // Active tab but no URL → no hint
        assert!(manager.continuation_hint_for("test").await.is_none());
    }

    #[tokio::test]
    async fn test_continuation_hint_with_url() {
        let manager = TabSessionManager::new();
        {
            let mut sessions = manager.sessions.write().await;
            let tab = Arc::new(TabSession::new());
            *tab.tab_index.write().await = Some(0);
            tab.note_action(Some("https://example.com")).await;
            sessions.insert("test".to_string(), tab);
        }
        let hint = manager.continuation_hint_for("test").await;
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("https://example.com"));
    }
}
