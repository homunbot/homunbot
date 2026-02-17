use anyhow::Result;

use crate::provider::ChatMessage;
use crate::storage::Database;

/// Session manager backed by SQLite.
///
/// Follows nanobot's SessionManager pattern but uses SQLite instead of JSONL files.
/// Messages are append-only for LLM cache efficiency.
#[derive(Clone)]
pub struct SessionManager {
    db: Database,
}

impl SessionManager {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Ensure a session exists in the database
    pub async fn ensure_session(&self, key: &str) -> Result<()> {
        self.db.upsert_session(key, 0).await
    }

    /// Add a message to a session (creates session if needed)
    pub async fn add_message(
        &self,
        session_key: &str,
        role: &str,
        content: &str,
    ) -> Result<()> {
        self.db.upsert_session(session_key, 0).await?;
        self.db.insert_message(session_key, role, content, &[]).await
    }

    /// Add a message with tools_used metadata
    pub async fn add_message_with_tools(
        &self,
        session_key: &str,
        role: &str,
        content: &str,
        tools_used: &[String],
    ) -> Result<()> {
        self.db.upsert_session(session_key, 0).await?;
        self.db.insert_message(session_key, role, content, tools_used).await
    }

    /// Get the last N messages as ChatMessage for the LLM
    pub async fn get_history(
        &self,
        session_key: &str,
        max_messages: u32,
    ) -> Result<Vec<ChatMessage>> {
        let rows = self.db.load_messages(session_key, max_messages).await?;
        let messages = rows
            .into_iter()
            .map(|r| ChatMessage {
                role: r.role,
                content: Some(r.content),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            })
            .collect();
        Ok(messages)
    }

    /// Clear all messages for a session (for /new command)
    pub async fn clear(&self, session_key: &str) -> Result<()> {
        self.db.clear_messages(session_key).await
    }
}
