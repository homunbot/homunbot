//! Domain-scoped storage traits for testability and abstraction.
//!
//! Instead of a monolithic `Storage` trait with 110+ methods, we define
//! focused traits per domain. Each consumer depends only on the trait(s)
//! it needs (Interface Segregation Principle).
//!
//! `Database` implements all traits. Consumers can be migrated to trait
//! bounds incrementally — no big-bang refactoring required.

use anyhow::Result;
use async_trait::async_trait;

use super::db::{
    MemoryChunkRow, MemoryRow, MemorySummaryRow, MessageRow, RagChunkRow, RagSourceRow,
    SessionListRow, SessionRow,
};

// ── SessionStore ────────────────────────────────────────────────

/// Session and message CRUD operations.
///
/// Used by: `SessionManager`, `AgentLoop`, `MemoryConsolidator`.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Create or update a session record.
    async fn upsert_session(&self, key: &str, last_consolidated: i64) -> Result<()>;

    /// Load a session by key.
    async fn load_session(&self, key: &str) -> Result<Option<SessionRow>>;

    /// Delete a session and all its messages. Returns true if deleted.
    async fn delete_session(&self, key: &str) -> Result<bool>;

    /// List sessions matching a key prefix (for sidebar/session list).
    async fn list_sessions_by_prefix(
        &self,
        prefix_like: &str,
        limit: u32,
    ) -> Result<Vec<SessionListRow>>;

    /// Update session metadata JSON.
    async fn set_session_metadata(&self, key: &str, metadata: &str) -> Result<()>;

    /// Insert a message into a session.
    async fn insert_message(
        &self,
        session_key: &str,
        role: &str,
        content: &str,
        tools_used: &[String],
    ) -> Result<()>;

    /// Load the last N messages from a session.
    async fn load_messages(&self, session_key: &str, limit: u32) -> Result<Vec<MessageRow>>;

    /// Count messages in a session.
    async fn count_messages(&self, session_key: &str) -> Result<i64>;

    /// Delete all messages in a session.
    async fn clear_messages(&self, session_key: &str) -> Result<()>;

    /// Load messages older than the keep window (for compaction).
    async fn load_old_messages(
        &self,
        session_key: &str,
        keep_count: u32,
    ) -> Result<Vec<MessageRow>>;

    /// Delete messages older than the keep window. Returns count deleted.
    async fn delete_old_messages(&self, session_key: &str, keep_count: u32) -> Result<u64>;
}

// ── MemoryStore ─────────────────────────────────────────────────

/// Memory chunk and summary storage operations.
///
/// Used by: `MemoryConsolidator`, `MemorySearcher`, web API.
#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait MemoryStore: Send + Sync {
    /// Insert a short-term memory record.
    async fn insert_memory(
        &self,
        session_key: Option<&str>,
        content: &str,
        memory_type: &str,
    ) -> Result<()>;

    /// Load short-term memories for a session.
    async fn load_memories(&self, session_key: &str) -> Result<Vec<MemoryRow>>;

    /// Load the consolidated long-term memory (MEMORY.md equivalent).
    async fn load_long_term_memory(&self) -> Result<Option<String>>;

    /// Upsert the consolidated long-term memory.
    async fn upsert_long_term_memory(&self, content: &str) -> Result<()>;

    /// Insert a detailed memory chunk (from consolidation).
    async fn insert_memory_chunk(
        &self,
        date: &str,
        source: &str,
        heading: &str,
        content: &str,
        memory_type: &str,
        contact_id: Option<i64>,
        agent_id: Option<&str>,
        importance: i32,
    ) -> Result<i64>;

    /// Load chunks by their IDs (for vector search result hydration).
    async fn load_chunks_by_ids(&self, ids: &[i64]) -> Result<Vec<MemoryChunkRow>>;

    /// Full-text search on memory chunks. Returns (chunk_id, bm25_score).
    async fn fts5_search(&self, query: &str, limit: usize) -> Result<Vec<(i64, f64)>>;

    /// Count total memory chunks.
    async fn count_memory_chunks(&self) -> Result<i64>;

    /// List memory chunks with pagination (for UI).
    async fn list_memory_history(&self, limit: i64, offset: i64) -> Result<Vec<MemoryChunkRow>>;

    /// Load all memory chunks (for HNSW reindex).
    async fn load_all_memory_chunks(&self) -> Result<Vec<MemoryChunkRow>>;

    /// Prune low-importance chunks to stay within budget. Returns pruned IDs.
    async fn prune_memory_chunks_to_budget(&self, keep_count: u32) -> Result<Vec<i64>>;

    /// Load chunks in a date range (for period summarization).
    async fn load_chunks_in_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<MemoryChunkRow>>;

    /// Delete all memory data (chunks, summaries, long-term).
    async fn reset_all_memory(&self) -> Result<()>;

    /// Insert a hierarchical summary (weekly/monthly digest). Returns the row ID.
    async fn insert_memory_summary(
        &self,
        period: &str,
        start_date: &str,
        end_date: &str,
        content: &str,
        contact_id: Option<i64>,
        agent_id: Option<&str>,
    ) -> Result<i64>;

    /// Check if a summary already exists for a period.
    async fn has_memory_summary(&self, period: &str, start_date: &str) -> Result<bool>;

    /// Load summaries overlapping a date range.
    async fn load_summaries_in_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<MemorySummaryRow>>;
}

// ── RagStore ────────────────────────────────────────────────────

/// RAG knowledge base source and chunk storage operations.
///
/// Used by: `RagEngine`, web API.
#[async_trait]
pub trait RagStore: Send + Sync {
    /// Insert a new document source. Returns the source ID.
    async fn insert_rag_source(
        &self,
        file_path: &str,
        file_name: &str,
        file_hash: &str,
        doc_type: &str,
        file_size: i64,
        source_channel: Option<&str>,
    ) -> Result<i64>;

    /// Find a source by its content hash (deduplication).
    async fn find_rag_source_by_hash(&self, file_hash: &str) -> Result<Option<RagSourceRow>>;

    /// Find a source by its file path.
    async fn find_rag_source_by_path(&self, file_path: &str) -> Result<Option<RagSourceRow>>;

    /// Update source processing status and chunk count.
    async fn update_rag_source_status(
        &self,
        id: i64,
        status: &str,
        error_message: Option<&str>,
        chunk_count: i64,
    ) -> Result<()>;

    /// Delete a source and its chunks. Returns true if deleted.
    async fn delete_rag_source(&self, id: i64) -> Result<bool>;

    /// List all document sources.
    async fn list_rag_sources(&self) -> Result<Vec<RagSourceRow>>;

    /// Count total document sources.
    async fn count_rag_sources(&self) -> Result<i64>;

    /// Insert a document chunk. Returns the chunk ID.
    async fn insert_rag_chunk(
        &self,
        source_id: i64,
        chunk_index: i64,
        heading: &str,
        content: &str,
        token_count: i64,
        sensitive: bool,
    ) -> Result<i64>;

    /// Update a chunk's heading (after LLM enrichment).
    async fn update_rag_chunk_heading(&self, chunk_id: i64, heading: &str) -> Result<()>;

    /// Load chunks by their IDs (for vector search result hydration).
    async fn load_rag_chunks_by_ids(&self, ids: &[i64]) -> Result<Vec<RagChunkRow>>;

    /// Full-text search on RAG chunks. Returns (chunk_id, bm25_score).
    async fn rag_fts5_search(&self, query: &str, limit: usize) -> Result<Vec<(i64, f64)>>;

    /// Count total RAG chunks.
    async fn count_rag_chunks(&self) -> Result<i64>;

    /// Load all chunks for a specific source.
    async fn load_rag_chunks_by_source(&self, source_id: i64) -> Result<Vec<RagChunkRow>>;

    /// Delete all chunks for a source. Returns count deleted.
    async fn delete_rag_chunks_by_source(&self, source_id: i64) -> Result<u64>;
}

// ── MemoryBackend ───────────────────────────────────────────────

/// Combined trait for consumers that need both session and memory operations.
///
/// Used by: `MemoryConsolidator` (reads session messages, writes memory chunks).
/// Blanket-implemented for any type that implements both `SessionStore` and `MemoryStore`.
pub trait MemoryBackend: SessionStore + MemoryStore {}

impl<T: SessionStore + MemoryStore> MemoryBackend for T {}
