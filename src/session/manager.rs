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
