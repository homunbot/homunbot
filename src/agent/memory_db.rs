//! Database operations for the memory subsystem.
//!
//! Extension `impl Database` for memory chunk and summary CRUD + FTS5 search.
//! Follows the pattern in `business/db.rs` and `contacts/db.rs`.

use anyhow::{Context, Result};

use crate::storage::{Database, MemoryChunkRow, MemorySummaryRow};

impl Database {
    /// Insert a memory chunk and return its row ID (for vector indexing).
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_memory_chunk(
        &self,
        date: &str,
        source: &str,
        heading: &str,
        content: &str,
        memory_type: &str,
        contact_id: Option<i64>,
        agent_id: Option<&str>,
        importance: i32,
        profile_id: Option<i64>,
    ) -> Result<i64> {
        let result = sqlx::query(
            "INSERT INTO memory_chunks (date, source, heading, content, memory_type, contact_id, agent_id, importance, profile_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(date)
        .bind(source)
        .bind(heading)
        .bind(content)
        .bind(memory_type)
        .bind(contact_id)
        .bind(agent_id)
        .bind(importance)
        .bind(profile_id)
        .execute(self.pool())
        .await
        .context("Failed to insert memory chunk")?;

        Ok(result.last_insert_rowid())
    }

    /// Load memory chunks by their IDs (after vector search returns matching IDs).
    pub async fn load_chunks_by_ids(&self, ids: &[i64]) -> Result<Vec<MemoryChunkRow>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
        let query = format!(
            "SELECT id, date, source, heading, content, memory_type, created_at, contact_id, agent_id, importance, profile_id, profile_id
             FROM memory_chunks WHERE id IN ({})
             ORDER BY created_at DESC",
            placeholders.join(",")
        );

        let mut q = sqlx::query_as::<_, MemoryChunkRow>(&query);
        for id in ids {
            q = q.bind(id);
        }

        let rows = q
            .fetch_all(self.pool())
            .await
            .context("Failed to load memory chunks by IDs")?;

        Ok(rows)
    }

    /// Full-text search on memory chunks using FTS5 BM25 ranking.
    /// Returns `(chunk_id, bm25_score)` pairs, best matches first.
    pub async fn fts5_search(&self, query: &str, limit: usize) -> Result<Vec<(i64, f64)>> {
        let rows: Vec<(i64, f64)> = sqlx::query_as(
            "SELECT rowid, rank
             FROM memory_fts
             WHERE memory_fts MATCH ?
             ORDER BY rank
             LIMIT ?",
        )
        .bind(query)
        .bind(limit as i64)
        .fetch_all(self.pool())
        .await
        .context("FTS5 search failed")?;

        Ok(rows)
    }

    /// Count total memory chunks.
    pub async fn count_memory_chunks(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_chunks")
            .fetch_one(self.pool())
            .await
            .context("Failed to count memory chunks")?;
        Ok(count)
    }

    /// Count memory chunks visible to a specific profile (profile's own + global NULL).
    pub async fn count_memory_chunks_for_profile(&self, profile_id: i64) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM memory_chunks WHERE profile_id IS NULL OR profile_id = ?",
        )
        .bind(profile_id)
        .fetch_one(self.pool())
        .await
        .context("Failed to count memory chunks for profile")?;
        Ok(count)
    }

    /// List memory history chunks (type='history'), ordered by newest first.
    pub async fn list_memory_history(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<MemoryChunkRow>> {
        let rows = sqlx::query_as::<_, MemoryChunkRow>(
            "SELECT id, date, source, heading, content, memory_type, created_at, contact_id, agent_id, importance, profile_id \
             FROM memory_chunks WHERE memory_type = 'history' \
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await
        .context("Failed to list memory history")?;
        Ok(rows)
    }

    /// Load all memory chunks (for re-embedding after model change).
    pub async fn load_all_memory_chunks(&self) -> Result<Vec<MemoryChunkRow>> {
        let rows = sqlx::query_as::<_, MemoryChunkRow>(
            "SELECT id, date, source, heading, content, memory_type, created_at, contact_id, agent_id, importance, profile_id
             FROM memory_chunks ORDER BY id",
        )
        .fetch_all(self.pool())
        .await
        .context("Failed to load all memory chunks")?;
        Ok(rows)
    }

    /// Prune lowest-scoring memory chunks to stay within budget.
    ///
    /// Keeps the `keep_count` most valuable chunks, deleting the rest.
    /// Score is approximated as `importance * recency` where recency penalizes old chunks.
    /// Returns the IDs of deleted chunks (for HNSW index cleanup).
    pub async fn prune_memory_chunks_to_budget(&self, keep_count: u32) -> Result<Vec<i64>> {
        let deleted_ids: Vec<(i64,)> = sqlx::query_as(
            "SELECT id FROM memory_chunks
             ORDER BY importance ASC, created_at ASC
             LIMIT (SELECT MAX(0, COUNT(*) - ?) FROM memory_chunks)",
        )
        .bind(keep_count as i64)
        .fetch_all(self.pool())
        .await
        .context("Failed to identify chunks to prune")?;

        if deleted_ids.is_empty() {
            return Ok(Vec::new());
        }

        let ids: Vec<i64> = deleted_ids.iter().map(|r| r.0).collect();

        for chunk in ids.chunks(100) {
            let placeholders: Vec<String> = chunk.iter().map(|_| "?".to_string()).collect();
            let sql = format!(
                "DELETE FROM memory_chunks WHERE id IN ({})",
                placeholders.join(",")
            );
            let mut q = sqlx::query(&sql);
            for id in chunk {
                q = q.bind(id);
            }
            q.execute(self.pool())
                .await
                .context("Failed to delete pruned memory chunks")?;
        }

        tracing::info!(
            pruned = ids.len(),
            kept = keep_count,
            "Pruned memory chunks to budget"
        );
        Ok(ids)
    }

    // --- Memory summary operations (hierarchical summarization) ---

    /// Insert a hierarchical memory summary (weekly/monthly digest).
    pub async fn insert_memory_summary(
        &self,
        period: &str,
        start_date: &str,
        end_date: &str,
        content: &str,
        contact_id: Option<i64>,
        agent_id: Option<&str>,
    ) -> Result<i64> {
        let result = sqlx::query(
            "INSERT INTO memory_summaries (period, start_date, end_date, content, contact_id, agent_id)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(period)
        .bind(start_date)
        .bind(end_date)
        .bind(content)
        .bind(contact_id)
        .bind(agent_id)
        .execute(self.pool())
        .await
        .context("Failed to insert memory summary")?;
        Ok(result.last_insert_rowid())
    }

    /// Check if a summary already exists for the given period and date range.
    pub async fn has_memory_summary(&self, period: &str, start_date: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM memory_summaries WHERE period = ? AND start_date = ?",
        )
        .bind(period)
        .bind(start_date)
        .fetch_one(self.pool())
        .await
        .context("Failed to check memory summary existence")?;
        Ok(count > 0)
    }

    /// Load memory chunks within a date range (for summarization).
    pub async fn load_chunks_in_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<MemoryChunkRow>> {
        let rows = sqlx::query_as::<_, MemoryChunkRow>(
            "SELECT id, date, source, heading, content, memory_type, created_at, contact_id, agent_id, importance, profile_id
             FROM memory_chunks WHERE date >= ? AND date <= ?
             ORDER BY date ASC, created_at ASC",
        )
        .bind(start_date)
        .bind(end_date)
        .fetch_all(self.pool())
        .await
        .context("Failed to load chunks in date range")?;
        Ok(rows)
    }

    /// Load memory summaries matching a date range (for search augmentation).
    pub async fn load_summaries_in_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<MemorySummaryRow>> {
        let rows = sqlx::query_as::<_, MemorySummaryRow>(
            "SELECT id, period, start_date, end_date, content, contact_id, agent_id, created_at
             FROM memory_summaries WHERE start_date >= ? AND end_date <= ?
             ORDER BY start_date ASC",
        )
        .bind(start_date)
        .bind(end_date)
        .fetch_all(self.pool())
        .await
        .context("Failed to load memory summaries")?;
        Ok(rows)
    }

    /// Delete all memory data from the database (memory_chunks, memories, messages).
    pub async fn reset_all_memory(&self) -> Result<()> {
        sqlx::query("DELETE FROM memory_chunks")
            .execute(self.pool())
            .await
            .context("Failed to clear memory_chunks")?;
        sqlx::query("DELETE FROM memories")
            .execute(self.pool())
            .await
            .context("Failed to clear memories")?;
        sqlx::query("DELETE FROM messages")
            .execute(self.pool())
            .await
            .context("Failed to clear messages")?;
        Ok(())
    }
}
