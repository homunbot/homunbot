//! Hybrid memory search — combines USearch vector similarity with SQLite FTS5 BM25.
//!
//! Strategy:
//! 1. USearch → top 20 (cosine similarity, O(log N))
//! 2. FTS5 → top 20 (BM25 keyword matching)
//! 3. Reciprocal Rank Fusion: merge both result sets
//! 4. Apply temporal decay (penalize old content)
//! 5. Load top K chunks from SQLite

use std::collections::HashMap;

use anyhow::Result;

use crate::storage::{Database, MemoryChunkRow};

use super::embeddings::EmbeddingEngine;

/// Number of candidates to pull from each search method before merging.
const CANDIDATES_PER_SOURCE: usize = 20;

/// RRF constant (standard value from the original paper).
const RRF_K: f64 = 60.0;

/// Default half-life for temporal decay (in days).
/// After this many days, a chunk's score is multiplied by 0.5.
const DEFAULT_HALF_LIFE_DAYS: f64 = 30.0;

/// Hybrid memory searcher — vector + full-text search over memory chunks.
pub struct MemorySearcher {
    db: Database,
    engine: EmbeddingEngine,
}

/// A search result with its merged relevance score.
#[derive(Debug)]
pub struct SearchResult {
    pub chunk: MemoryChunkRow,
    pub score: f64,
}

impl MemorySearcher {
    /// Create a new searcher with the given database and embedding engine.
    pub fn new(db: Database, engine: EmbeddingEngine) -> Self {
        Self { db, engine }
    }

    /// Get mutable access to the embedding engine (for indexing new chunks).
    pub fn engine_mut(&mut self) -> &mut EmbeddingEngine {
        &mut self.engine
    }

    /// Get a reference to the embedding engine.
    pub fn engine(&self) -> &EmbeddingEngine {
        &self.engine
    }

    /// Search memory chunks using hybrid vector + FTS5 approach.
    ///
    /// Returns the top `top_k` most relevant chunks, sorted by score (highest first).
    ///
    /// If FTS5 fails (e.g., special characters in query), falls back to vector-only search.
    ///
    /// When `contact_id` is provided, results are filtered to include both
    /// contact-scoped chunks (matching the contact) and global chunks (contact_id IS NULL).
    pub async fn search(&mut self, query: &str, top_k: usize) -> Result<Vec<SearchResult>> {
        self.search_scoped(query, top_k, None).await
    }

    /// Search with optional contact and agent scoping.
    ///
    /// When `contact_id` is provided, results include global + contact-specific chunks.
    /// When `agent_id` is provided, results include global + agent-specific chunks.
    pub async fn search_scoped(
        &mut self,
        query: &str,
        top_k: usize,
        contact_id: Option<i64>,
    ) -> Result<Vec<SearchResult>> {
        self.search_scoped_full(query, top_k, contact_id, None).await
    }

    /// Search with full scoping: contact + agent.
    pub async fn search_scoped_full(
        &mut self,
        query: &str,
        top_k: usize,
        contact_id: Option<i64>,
        agent_id: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        // Run vector search (always works)
        let vector_results = self.engine.search(query, CANDIDATES_PER_SOURCE).await?;

        // Try FTS5 search, but gracefully handle failures
        let fts_results = match self
            .db
            .fts5_search(&sanitize_fts5_query(query), CANDIDATES_PER_SOURCE)
            .await
        {
            Ok(results) => results,
            Err(e) => {
                tracing::debug!(
                    error = %e,
                    query = %query,
                    "FTS5 search failed, using vector-only results"
                );
                // Fallback: use vector results only
                return self.search_vector_only(&vector_results, top_k, contact_id, agent_id).await;
            }
        };

        // Merge using Reciprocal Rank Fusion
        let merged_ids = rrf_merge(&vector_results, &fts_results, top_k);

        if merged_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Load full chunks from SQLite
        let chunk_ids: Vec<i64> = merged_ids.iter().map(|&(id, _)| id).collect();
        let chunks = self.db.load_chunks_by_ids(&chunk_ids).await?;

        // Attach scores with temporal decay, preserving merge order
        let chunk_map: HashMap<i64, MemoryChunkRow> =
            chunks.into_iter().map(|c| (c.id, c)).collect();

        let now = chrono::Utc::now();
        let results: Vec<SearchResult> = merged_ids
            .into_iter()
            .filter_map(|(id, score)| {
                chunk_map.get(&id).and_then(|chunk| {
                    // Contact scoping: include global chunks + contact-specific chunks
                    if let Some(cid) = contact_id {
                        if chunk.contact_id.is_some() && chunk.contact_id != Some(cid) {
                            return None; // belongs to a different contact
                        }
                    }
                    // Agent scoping: include global chunks + agent-specific chunks
                    if let Some(aid) = agent_id {
                        if chunk.agent_id.is_some() && chunk.agent_id.as_deref() != Some(aid) {
                            return None; // belongs to a different agent
                        }
                    }
                    // Apply temporal decay and importance weighting
                    let decayed_score =
                        apply_temporal_decay(score, &chunk.date, now, DEFAULT_HALF_LIFE_DAYS);
                    // Importance multiplier: 3 = neutral (1.0x), 5 = 1.67x, 1 = 0.33x
                    let importance_factor = chunk.importance as f64 / 3.0;
                    Some(SearchResult {
                        chunk: chunk.clone(),
                        score: decayed_score * importance_factor,
                    })
                })
            })
            .collect();

        // Re-sort by decayed score (highest first)
        let mut results = results;
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results)
    }

    /// Fallback: search using vector results only (when FTS5 fails).
    async fn search_vector_only(
        &mut self,
        vector_results: &[(i64, f32)],
        top_k: usize,
        contact_id: Option<i64>,
        agent_id: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        if vector_results.is_empty() {
            return Ok(Vec::new());
        }

        // Take top_k from vector results
        let top_ids: Vec<(i64, f64)> = vector_results
            .iter()
            .take(top_k)
            .enumerate()
            .map(|(rank, &(id, _dist))| (id, 1.0 / (RRF_K + rank as f64 + 1.0)))
            .collect();

        let chunk_ids: Vec<i64> = top_ids.iter().map(|&(id, _)| id).collect();
        let chunks = self.db.load_chunks_by_ids(&chunk_ids).await?;

        let chunk_map: HashMap<i64, MemoryChunkRow> =
            chunks.into_iter().map(|c| (c.id, c)).collect();

        let results = top_ids
            .into_iter()
            .filter_map(|(id, score)| {
                chunk_map.get(&id).and_then(|chunk| {
                    // Contact scoping
                    if let Some(cid) = contact_id {
                        if chunk.contact_id.is_some() && chunk.contact_id != Some(cid) {
                            return None;
                        }
                    }
                    // Agent scoping
                    if let Some(aid) = agent_id {
                        if chunk.agent_id.is_some() && chunk.agent_id.as_deref() != Some(aid) {
                            return None;
                        }
                    }
                    Some(SearchResult {
                        chunk: chunk.clone(),
                        score,
                    })
                })
            })
            .collect();

        Ok(results)
    }

    /// Save the vector index to disk.
    pub fn save_index(&self) -> Result<()> {
        self.engine.save()
    }

    /// Replace the embedding engine's provider (for model change + reindex).
    pub fn reset_engine(
        &mut self,
        provider: Box<dyn super::embeddings::EmbeddingProvider>,
    ) -> Result<()> {
        self.engine.reset_with_provider(provider)
    }

    /// Rebuild the HNSW index from all memory chunks in the database.
    ///
    /// Used after `reset_engine()` when the user changes embedding model.
    pub async fn reindex_all(&mut self) -> Result<usize> {
        let chunks = self.db.load_all_memory_chunks().await?;
        let mut total = 0;
        for chunk in &chunks {
            self.engine.index_chunk(chunk.id, &chunk.content).await?;
            total += 1;
        }
        self.engine.save()?;
        tracing::info!(vectors = total, "Memory index rebuilt");
        Ok(total)
    }
}

/// Sanitize a query string for FTS5 MATCH.
///
/// FTS5 has special syntax that can cause parse errors:
/// - Quotes must be balanced
/// - Column names followed by `:` have special meaning
/// - Parentheses, `*`, `^`, and some operators are special
///
/// This function strips problematic characters while preserving keywords.
fn sanitize_fts5_query(query: &str) -> String {
    // Remove or escape FTS5 special characters
    let sanitized: String = query
        .chars()
        .filter(|c| {
            c.is_alphanumeric()
                || *c == ' '
                || *c == '-'
                || *c == '_'
                || *c == '.'
                || *c == ','
                || (*c >= 'à' && *c <= 'ÿ')  // Accented lowercase
                || (*c >= 'À' && *c <= 'ß') // Accented uppercase
        })
        .collect();

    // If we stripped everything, return a simple tokenized version
    if sanitized.trim().is_empty() {
        // Just keep alphanumeric and spaces
        query
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == ' ')
            .collect()
    } else {
        sanitized
    }
}

/// Reciprocal Rank Fusion — merges two ranked result lists into one.
///
/// For each result, the RRF score is: `sum(1 / (k + rank_i))` across lists.
/// This naturally balances results that appear in both lists vs. one.
fn rrf_merge(
    vector_results: &[(i64, f32)],
    fts_results: &[(i64, f64)],
    top_k: usize,
) -> Vec<(i64, f64)> {
    let mut scores: HashMap<i64, f64> = HashMap::new();

    // Vector results: rank 1, 2, 3, ...
    for (rank, &(id, _distance)) in vector_results.iter().enumerate() {
        *scores.entry(id).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
    }

    // FTS5 results: rank 1, 2, 3, ...
    for (rank, &(id, _bm25)) in fts_results.iter().enumerate() {
        *scores.entry(id).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
    }

    // Sort by RRF score descending, take top K
    let mut sorted: Vec<(i64, f64)> = scores.into_iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    sorted.truncate(top_k);
    sorted
}

/// Apply temporal decay to a search score based on content age.
///
/// Uses exponential decay: score * 0.5^(age / half_life)
/// - Fresh content (< 1 day): minimal decay
/// - Content at half_life: 50% of original score
/// - Content at 2x half_life: 25% of original score
///
/// Inspired by OpenClaw's temporal-decay.ts implementation.
fn apply_temporal_decay(
    score: f64,
    chunk_date: &str, // Format: YYYY-MM-DD
    now: chrono::DateTime<chrono::Utc>,
    half_life_days: f64,
) -> f64 {
    // Parse chunk date
    let chunk_date = match chrono::NaiveDate::parse_from_str(chunk_date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return score, // Invalid date, no decay
    };

    // Calculate age in days
    let chunk_datetime = chunk_date.and_hms_opt(0, 0, 0).unwrap();
    let now_naive = now.naive_utc();
    let age_duration = now_naive.signed_duration_since(chunk_datetime);
    let age_days = age_duration.num_days() as f64;

    // No decay for negative age (future dates) or very fresh content
    if age_days <= 0.0 {
        return score;
    }

    // Exponential decay: 0.5^(age / half_life)
    let decay = 0.5_f64.powf(age_days / half_life_days);

    score * decay
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_merge_both_sources() {
        // Chunk 1 appears in both lists → should rank highest
        let vector = vec![(1, 0.1_f32), (2, 0.2), (3, 0.3)];
        let fts = vec![(1, -5.0), (4, -3.0), (5, -2.0)];

        let merged = rrf_merge(&vector, &fts, 5);

        // Chunk 1 should be first (appears in both)
        assert_eq!(merged[0].0, 1);
        assert!(merged[0].1 > merged[1].1);
    }

    #[test]
    fn test_rrf_merge_empty() {
        let merged = rrf_merge(&[], &[], 5);
        assert!(merged.is_empty());
    }

    #[test]
    fn test_rrf_merge_single_source() {
        let vector = vec![(10, 0.1_f32), (20, 0.2)];
        let fts: Vec<(i64, f64)> = vec![];

        let merged = rrf_merge(&vector, &fts, 5);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].0, 10); // lower distance = higher rank
    }

    #[test]
    fn test_rrf_top_k_limit() {
        let vector: Vec<(i64, f32)> = (1..=10).map(|i| (i, i as f32 * 0.1)).collect();
        let fts: Vec<(i64, f64)> = (11..=20).map(|i| (i, -(i as f64))).collect();

        let merged = rrf_merge(&vector, &fts, 3);
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn test_sanitize_fts5_simple() {
        let query = "hello world";
        assert_eq!(sanitize_fts5_query(query), "hello world");
    }

    #[test]
    fn test_sanitize_fts5_special_chars() {
        // Parentheses, quotes, and special chars are removed
        let query = "user's (password) \"secret\"";
        let sanitized = sanitize_fts5_query(query);
        assert!(!sanitized.contains('('));
        assert!(!sanitized.contains(')'));
        assert!(!sanitized.contains('"'));
        assert!(!sanitized.contains('\''));
        assert!(sanitized.contains("user"));
        assert!(sanitized.contains("password"));
    }

    #[test]
    fn test_sanitize_fts5_colon() {
        // Colons have special meaning in FTS5 (column:term)
        let query = "api:key value";
        let sanitized = sanitize_fts5_query(query);
        assert!(!sanitized.contains(':'));
        assert!(sanitized.contains("api"));
        assert!(sanitized.contains("key"));
    }

    #[test]
    fn test_sanitize_fts5_accented() {
        let query = "café résumé naïve";
        let sanitized = sanitize_fts5_query(query);
        assert!(sanitized.contains("café"));
        assert!(sanitized.contains("résumé"));
    }

    #[test]
    fn test_sanitize_fts5_empty_result() {
        // If stripping removes everything, fall back to alphanumeric only
        let query = "!!!@@@###";
        let sanitized = sanitize_fts5_query(query);
        assert!(sanitized.trim().is_empty());
    }

    #[test]
    fn test_temporal_decay_fresh() {
        // Fresh content (today) should have minimal decay
        let now = chrono::Utc::now();
        let today = now.format("%Y-%m-%d").to_string();
        let score = apply_temporal_decay(1.0, &today, now, 30.0);
        assert!(
            score > 0.99,
            "Fresh content should have score > 0.99, got {}",
            score
        );
    }

    #[test]
    fn test_temporal_decay_half_life() {
        // Content at half_life should have 50% score
        let now = chrono::Utc::now();
        let thirty_days_ago = (now - chrono::Duration::days(30))
            .format("%Y-%m-%d")
            .to_string();
        let score = apply_temporal_decay(1.0, &thirty_days_ago, now, 30.0);
        assert!(
            (score - 0.5).abs() < 0.05,
            "Half-life content should have score ~0.5, got {}",
            score
        );
    }

    #[test]
    fn test_temporal_decay_double_half_life() {
        // Content at 2x half_life should have 25% score
        let now = chrono::Utc::now();
        let sixty_days_ago = (now - chrono::Duration::days(60))
            .format("%Y-%m-%d")
            .to_string();
        let score = apply_temporal_decay(1.0, &sixty_days_ago, now, 30.0);
        assert!(
            (score - 0.25).abs() < 0.05,
            "2x half-life content should have score ~0.25, got {}",
            score
        );
    }

    #[test]
    fn test_temporal_decay_invalid_date() {
        // Invalid date should return original score (no decay)
        let now = chrono::Utc::now();
        let score = apply_temporal_decay(1.0, "invalid-date", now, 30.0);
        assert_eq!(score, 1.0, "Invalid date should not decay");
    }

    #[test]
    fn test_temporal_decay_future_date() {
        // Future date should return original score (no decay)
        let now = chrono::Utc::now();
        let tomorrow = (now + chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let score = apply_temporal_decay(1.0, &tomorrow, now, 30.0);
        assert_eq!(score, 1.0, "Future date should not decay");
    }
}
