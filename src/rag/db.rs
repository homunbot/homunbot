//! Database operations for the RAG knowledge base.
//!
//! Extension `impl Database` following the pattern in `business/db.rs`
//! and `contacts/db.rs`. Handles source and chunk CRUD + FTS5 search.

use anyhow::{Context, Result};

use crate::storage::{Database, RagChunkRow, RagSourceRow};

impl Database {
    // ─── RAG Knowledge Base ──────────────────────────────────────

    /// Insert a new document source. Returns the source ID.
    pub async fn insert_rag_source(
        &self,
        file_path: &str,
        file_name: &str,
        file_hash: &str,
        doc_type: &str,
        file_size: i64,
        source_channel: Option<&str>,
    ) -> Result<i64> {
        let result = sqlx::query(
            "INSERT INTO rag_sources (file_path, file_name, file_hash, doc_type, file_size, source_channel)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(file_path)
        .bind(file_name)
        .bind(file_hash)
        .bind(doc_type)
        .bind(file_size)
        .bind(source_channel)
        .execute(self.pool())
        .await
        .context("Failed to insert RAG source")?;

        Ok(result.last_insert_rowid())
    }

    /// Find a source by its content hash (deduplication).
    pub async fn find_rag_source_by_hash(&self, file_hash: &str) -> Result<Option<RagSourceRow>> {
        let row = sqlx::query_as::<_, RagSourceRow>(
            "SELECT id, file_path, file_name, file_hash, doc_type, file_size,
                    chunk_count, status, error_message, source_channel, created_at, updated_at
             FROM rag_sources WHERE file_hash = ?",
        )
        .bind(file_hash)
        .fetch_optional(self.pool())
        .await
        .context("Failed to find RAG source by hash")?;

        Ok(row)
    }

    /// Find a source by its file path.
    pub async fn find_rag_source_by_path(&self, file_path: &str) -> Result<Option<RagSourceRow>> {
        let row = sqlx::query_as::<_, RagSourceRow>(
            "SELECT id, file_path, file_name, file_hash, doc_type, file_size,
                    chunk_count, status, error_message, source_channel, created_at, updated_at
             FROM rag_sources WHERE file_path = ?",
        )
        .bind(file_path)
        .fetch_optional(self.pool())
        .await
        .context("Failed to find RAG source by path")?;

        Ok(row)
    }

    /// Update source processing status and chunk count.
    pub async fn update_rag_source_status(
        &self,
        id: i64,
        status: &str,
        error_message: Option<&str>,
        chunk_count: i64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE rag_sources SET status = ?, error_message = ?, chunk_count = ?,
                    updated_at = datetime('now') WHERE id = ?",
        )
        .bind(status)
        .bind(error_message)
        .bind(chunk_count)
        .bind(id)
        .execute(self.pool())
        .await
        .context("Failed to update RAG source status")?;

        Ok(())
    }

    /// Delete a source and its chunks. Returns true if deleted.
    pub async fn delete_rag_source(&self, id: i64) -> Result<bool> {
        let result = sqlx::query("DELETE FROM rag_sources WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .context("Failed to delete RAG source")?;

        Ok(result.rows_affected() > 0)
    }

    /// List all document sources.
    pub async fn list_rag_sources(&self) -> Result<Vec<RagSourceRow>> {
        let rows = sqlx::query_as::<_, RagSourceRow>(
            "SELECT id, file_path, file_name, file_hash, doc_type, file_size,
                    chunk_count, status, error_message, source_channel, created_at, updated_at
             FROM rag_sources ORDER BY created_at DESC",
        )
        .fetch_all(self.pool())
        .await
        .context("Failed to list RAG sources")?;

        Ok(rows)
    }

    /// Insert a document chunk. Returns the chunk ID.
    pub async fn insert_rag_chunk(
        &self,
        source_id: i64,
        chunk_index: i64,
        heading: &str,
        content: &str,
        token_count: i64,
        sensitive: bool,
        profile_id: Option<i64>,
    ) -> Result<i64> {
        let result = sqlx::query(
            "INSERT INTO rag_chunks (source_id, chunk_index, heading, content, token_count, sensitive, profile_id)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(source_id)
        .bind(chunk_index)
        .bind(heading)
        .bind(content)
        .bind(token_count)
        .bind(sensitive)
        .bind(profile_id)
        .execute(self.pool())
        .await
        .context("Failed to insert RAG chunk")?;

        Ok(result.last_insert_rowid())
    }

    /// Update a chunk's heading (after LLM enrichment).
    pub async fn update_rag_chunk_heading(&self, chunk_id: i64, heading: &str) -> Result<()> {
        sqlx::query("UPDATE rag_chunks SET heading = ? WHERE id = ?")
            .bind(heading)
            .bind(chunk_id)
            .execute(self.pool())
            .await
            .context("Failed to update RAG chunk heading")?;
        Ok(())
    }

    /// Load chunks by their IDs (for vector search result hydration).
    pub async fn load_rag_chunks_by_ids(&self, ids: &[i64]) -> Result<Vec<RagChunkRow>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
        let query = format!(
            "SELECT id, source_id, chunk_index, heading, content, token_count, sensitive, created_at, profile_id
             FROM rag_chunks WHERE id IN ({})
             ORDER BY created_at DESC",
            placeholders.join(",")
        );

        let mut q = sqlx::query_as::<_, RagChunkRow>(&query);
        for id in ids {
            q = q.bind(id);
        }

        let rows = q
            .fetch_all(self.pool())
            .await
            .context("Failed to load RAG chunks by IDs")?;

        Ok(rows)
    }

    /// Full-text search on RAG chunks. Returns (chunk_id, bm25_score).
    pub async fn rag_fts5_search(&self, query: &str, limit: usize) -> Result<Vec<(i64, f64)>> {
        let rows: Vec<(i64, f64)> = sqlx::query_as(
            "SELECT rowid, rank
             FROM rag_fts
             WHERE rag_fts MATCH ?
             ORDER BY rank
             LIMIT ?",
        )
        .bind(query)
        .bind(limit as i64)
        .fetch_all(self.pool())
        .await
        .context("RAG FTS5 search failed")?;

        Ok(rows)
    }

    /// Count total RAG chunks.
    pub async fn count_rag_chunks(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rag_chunks")
            .fetch_one(self.pool())
            .await
            .context("Failed to count RAG chunks")?;
        Ok(count)
    }

    /// Count total document sources.
    pub async fn count_rag_sources(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rag_sources")
            .fetch_one(self.pool())
            .await
            .context("Failed to count RAG sources")?;
        Ok(count)
    }

    /// Load all chunks for a specific source.
    pub async fn load_rag_chunks_by_source(&self, source_id: i64) -> Result<Vec<RagChunkRow>> {
        let rows = sqlx::query_as::<_, RagChunkRow>(
            "SELECT id, source_id, chunk_index, heading, content, token_count, sensitive, created_at, profile_id
             FROM rag_chunks WHERE source_id = ? ORDER BY chunk_index",
        )
        .bind(source_id)
        .fetch_all(self.pool())
        .await
        .context("Failed to load RAG chunks by source")?;

        Ok(rows)
    }

    /// Delete all chunks for a source. Returns count deleted.
    pub async fn delete_rag_chunks_by_source(&self, source_id: i64) -> Result<u64> {
        let result = sqlx::query("DELETE FROM rag_chunks WHERE source_id = ?")
            .bind(source_id)
            .execute(self.pool())
            .await
            .context("Failed to delete RAG chunks by source")?;

        Ok(result.rows_affected())
    }
}
