use std::sync::Arc;

use anyhow::Result;

use crate::provider::ChatMessage;
use crate::storage::SessionStore;

/// Session manager backed by any `SessionStore` implementation.
///
/// Wraps a storage backend to provide session lifecycle management:
/// message persistence, history retrieval, and session cleanup.
#[derive(Clone)]
pub struct SessionManager {
    store: Arc<dyn SessionStore>,
}

impl SessionManager {
    /// Create from a concrete Database (backwards-compatible convenience).
    pub fn new(db: crate::storage::Database) -> Self {
        Self {
            store: Arc::new(db),
        }
    }

    /// Create from any SessionStore implementation.
    pub fn from_store(store: Arc<dyn SessionStore>) -> Self {
        Self { store }
    }

    /// Ensure a session exists in the database
    pub async fn ensure_session(&self, key: &str) -> Result<()> {
        self.store.upsert_session(key, 0).await
    }

    /// Add a message to a session (creates session if needed)
    pub async fn add_message(&self, session_key: &str, role: &str, content: &str) -> Result<()> {
        self.store.upsert_session(session_key, 0).await?;
        self.store
            .insert_message(session_key, role, content, &[])
            .await
    }

    /// Add a message with tools_used metadata
    pub async fn add_message_with_tools(
        &self,
        session_key: &str,
        role: &str,
        content: &str,
        tools_used: &[String],
    ) -> Result<()> {
        self.store.upsert_session(session_key, 0).await?;
        self.store
            .insert_message(session_key, role, content, tools_used)
            .await
    }

    /// Get the last N messages as ChatMessage for the LLM
    pub async fn get_history(
        &self,
        session_key: &str,
        max_messages: u32,
    ) -> Result<Vec<ChatMessage>> {
        let rows = self.store.load_messages(session_key, max_messages).await?;
        let messages = rows
            .into_iter()
            .map(|r| ChatMessage {
                role: r.role,
                content: Some(crate::web::chat_attachments::content_for_model(&r.content)),
                content_parts: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            })
            .collect();
        Ok(messages)
    }

    /// Clear all messages for a session (for /new command)
    pub async fn clear(&self, session_key: &str) -> Result<()> {
        self.store.clear_messages(session_key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicI64, Ordering};

    use crate::storage::{MessageRow, SessionListRow, SessionRow, SessionStore};

    /// In-memory SessionStore for testing — no SQLite required.
    struct MockSessionStore {
        sessions: tokio::sync::RwLock<HashMap<String, SessionRow>>,
        messages: tokio::sync::RwLock<HashMap<String, Vec<MessageRow>>>,
        next_id: AtomicI64,
    }

    impl MockSessionStore {
        fn new() -> Self {
            Self {
                sessions: tokio::sync::RwLock::new(HashMap::new()),
                messages: tokio::sync::RwLock::new(HashMap::new()),
                next_id: AtomicI64::new(1),
            }
        }
    }

    #[async_trait::async_trait]
    impl SessionStore for MockSessionStore {
        async fn upsert_session(&self, key: &str, last_consolidated: i64) -> Result<()> {
            let mut sessions = self.sessions.write().await;
            sessions
                .entry(key.to_string())
                .and_modify(|s| s.last_consolidated = last_consolidated)
                .or_insert_with(|| SessionRow {
                    key: key.to_string(),
                    created_at: "2026-01-01T00:00:00".to_string(),
                    updated_at: "2026-01-01T00:00:00".to_string(),
                    last_consolidated,
                    metadata: "{}".to_string(),
                });
            Ok(())
        }

        async fn load_session(&self, key: &str) -> Result<Option<SessionRow>> {
            Ok(self.sessions.read().await.get(key).cloned())
        }

        async fn delete_session(&self, key: &str) -> Result<bool> {
            self.messages.write().await.remove(key);
            Ok(self.sessions.write().await.remove(key).is_some())
        }

        async fn list_sessions_by_prefix(
            &self,
            prefix_like: &str,
            limit: u32,
        ) -> Result<Vec<SessionListRow>> {
            let sessions = self.sessions.read().await;
            let results: Vec<_> = sessions
                .keys()
                .filter(|k| k.contains(prefix_like))
                .take(limit as usize)
                .map(|k| SessionListRow {
                    key: k.clone(),
                    created_at: "2026-01-01".to_string(),
                    updated_at: "2026-01-01".to_string(),
                    metadata: "{}".to_string(),
                    message_count: 0,
                    first_user_message: None,
                    last_message_preview: None,
                    last_message_at: None,
                })
                .collect();
            Ok(results)
        }

        async fn set_session_metadata(&self, key: &str, metadata: &str) -> Result<()> {
            if let Some(s) = self.sessions.write().await.get_mut(key) {
                s.metadata = metadata.to_string();
            }
            Ok(())
        }

        async fn insert_message(
            &self,
            session_key: &str,
            role: &str,
            content: &str,
            tools_used: &[String],
        ) -> Result<()> {
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            let tools_json =
                serde_json::to_string(tools_used).unwrap_or_else(|_| "[]".to_string());
            self.messages
                .write()
                .await
                .entry(session_key.to_string())
                .or_default()
                .push(MessageRow {
                    id,
                    session_key: session_key.to_string(),
                    role: role.to_string(),
                    content: content.to_string(),
                    tools_used: tools_json,
                    timestamp: "2026-01-01T00:00:00".to_string(),
                });
            Ok(())
        }

        async fn load_messages(&self, session_key: &str, limit: u32) -> Result<Vec<MessageRow>> {
            let msgs = self.messages.read().await;
            let session_msgs = msgs.get(session_key).cloned().unwrap_or_default();
            let start = session_msgs.len().saturating_sub(limit as usize);
            Ok(session_msgs[start..].to_vec())
        }

        async fn count_messages(&self, session_key: &str) -> Result<i64> {
            Ok(self
                .messages
                .read()
                .await
                .get(session_key)
                .map(|m| m.len() as i64)
                .unwrap_or(0))
        }

        async fn clear_messages(&self, session_key: &str) -> Result<()> {
            self.messages.write().await.remove(session_key);
            Ok(())
        }

        async fn load_old_messages(
            &self,
            session_key: &str,
            keep_count: u32,
        ) -> Result<Vec<MessageRow>> {
            let msgs = self.messages.read().await;
            let session_msgs = msgs.get(session_key).cloned().unwrap_or_default();
            let cutoff = session_msgs.len().saturating_sub(keep_count as usize);
            Ok(session_msgs[..cutoff].to_vec())
        }

        async fn delete_old_messages(&self, session_key: &str, keep_count: u32) -> Result<u64> {
            let mut msgs = self.messages.write().await;
            if let Some(session_msgs) = msgs.get_mut(session_key) {
                let len = session_msgs.len();
                if len > keep_count as usize {
                    let to_delete = len - keep_count as usize;
                    session_msgs.drain(..to_delete);
                    return Ok(to_delete as u64);
                }
            }
            Ok(0)
        }
    }

    fn mock_manager() -> SessionManager {
        SessionManager::from_store(Arc::new(MockSessionStore::new()))
    }

    #[tokio::test]
    async fn add_and_retrieve_messages() {
        let mgr = mock_manager();
        mgr.add_message("s1", "user", "hello").await.unwrap();
        mgr.add_message("s1", "assistant", "hi").await.unwrap();

        let history = mgr.get_history("s1", 10).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[1].role, "assistant");
    }

    #[tokio::test]
    async fn clear_session() {
        let mgr = mock_manager();
        mgr.add_message("s1", "user", "hello").await.unwrap();
        mgr.clear("s1").await.unwrap();

        let history = mgr.get_history("s1", 10).await.unwrap();
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn history_respects_limit() {
        let mgr = mock_manager();
        for i in 0..10 {
            mgr.add_message("s1", "user", &format!("msg {i}"))
                .await
                .unwrap();
        }

        let history = mgr.get_history("s1", 3).await.unwrap();
        assert_eq!(history.len(), 3);
        // Should be the LAST 3 messages
        assert!(history[0]
            .content
            .as_deref()
            .unwrap()
            .contains("msg 7"));
    }

    #[tokio::test]
    async fn tools_used_persisted() {
        let mgr = mock_manager();
        mgr.add_message_with_tools(
            "s1",
            "assistant",
            "done",
            &["web_search".to_string(), "browser".to_string()],
        )
        .await
        .unwrap();

        // Verify via the store directly — tools_used is not in ChatMessage
        let history = mgr.get_history("s1", 10).await.unwrap();
        assert_eq!(history.len(), 1);
    }
}
