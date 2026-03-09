use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context as _, Result};
use sha2::{Digest, Sha256};

use crate::agent::embeddings::EmbeddingEngine;
use crate::storage::{Database, RagChunkRow, RagSourceRow};

use super::chunker::{chunk_file, detect_doc_type, is_supported, ChunkOptions};
use super::sensitive;

const CANDIDATES_PER_SOURCE: usize = 20;
const RRF_K: f64 = 60.0;

/// RAG search result with source attribution.
#[derive(Debug)]
pub struct RagSearchResult {
    pub chunk: RagChunkRow,
    pub score: f64,
    pub source_file: String,
}

/// RAG knowledge base stats.
#[derive(Debug, serde::Serialize)]
pub struct RagStats {
    pub source_count: i64,
    pub chunk_count: i64,
    pub index_vectors: usize,
}

/// Unified RAG engine — handles ingestion, search, and lifecycle.
pub struct RagEngine {
    db: Database,
    engine: EmbeddingEngine,
    chunk_opts: ChunkOptions,
}

impl RagEngine {
    pub fn new(db: Database, engine: EmbeddingEngine, chunk_opts: ChunkOptions) -> Self {
        Self {
            db,
            engine,
            chunk_opts,
        }
    }

    /// Ingest a single file. Returns source_id if successful, None if already indexed (dedup).
    pub async fn ingest_file(
        &mut self,
        path: &Path,
        source_channel: &str,
    ) -> Result<Option<i64>> {
        if !is_supported(path) {
            anyhow::bail!(
                "Unsupported file type: {}",
                path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("(none)")
            );
        }

        let content =
            std::fs::read(path).with_context(|| format!("Cannot read {}", path.display()))?;

        let hash = hex_sha256(&content);

        // Dedup: skip if already indexed
        if let Some(existing) = self.db.find_rag_source_by_hash(&hash).await? {
            tracing::debug!(source_id = existing.id, "File already indexed, skipping");
            return Ok(None);
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let doc_type = detect_doc_type(path).to_string();

        let source_id = self
            .db
            .insert_rag_source(
                &path.to_string_lossy(),
                &file_name,
                &hash,
                &doc_type,
                content.len() as i64,
                Some(source_channel),
            )
            .await?;

        match chunk_file(path, &self.chunk_opts) {
            Ok(chunks) if chunks.is_empty() => {
                self.db
                    .update_rag_source_status(source_id, "indexed", None, 0)
                    .await?;
                tracing::info!(source_id, file = %file_name, "File indexed (empty, 0 chunks)");
                Ok(Some(source_id))
            }
            Ok(chunks) => {
                let filename_sensitive = sensitive::is_sensitive_filename(&file_name);

                for chunk in &chunks {
                    // Prepend filename to heading so FTS5 can match by filename
                    let heading = if chunk.heading.is_empty() {
                        file_name.clone()
                    } else {
                        format!("{} — {}", file_name, chunk.heading)
                    };

                    let is_sensitive =
                        filename_sensitive || sensitive::is_sensitive(&chunk.content);

                    let chunk_id = self
                        .db
                        .insert_rag_chunk(
                            source_id,
                            chunk.index as i64,
                            &heading,
                            &chunk.content,
                            chunk.token_count as i64,
                            is_sensitive,
                        )
                        .await?;

                    // Embed filename + content together for better vector search
                    let embed_text = format!("{}\n{}", file_name, chunk.content);
                    self.engine.index_chunk(chunk_id, &embed_text).await?;
                }

                // Persist the HNSW index so vectors survive restarts
                if let Err(e) = self.engine.save() {
                    tracing::warn!(error = %e, "Failed to save RAG HNSW index");
                }

                self.db
                    .update_rag_source_status(source_id, "indexed", None, chunks.len() as i64)
                    .await?;

                tracing::info!(
                    source_id,
                    file = %file_name,
                    chunks = chunks.len(),
                    "File indexed in RAG"
                );
                Ok(Some(source_id))
            }
            Err(e) => {
                self.db
                    .update_rag_source_status(source_id, "error", Some(&e.to_string()), 0)
                    .await?;
                Err(e)
            }
        }
    }

    /// Ingest all supported files from a directory.
    pub async fn ingest_directory(
        &mut self,
        dir: &Path,
        recursive: bool,
        source_channel: &str,
    ) -> Result<Vec<i64>> {
        let mut indexed = Vec::new();

        let entries: Vec<_> = if recursive {
            walkdir_entries(dir)?
        } else {
            std::fs::read_dir(dir)
                .with_context(|| format!("Cannot read directory {}", dir.display()))?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .collect()
        };

        for path in entries {
            if !path.is_file() || !is_supported(&path) {
                continue;
            }
            match self.ingest_file(&path, source_channel).await {
                Ok(Some(id)) => indexed.push(id),
                Ok(None) => {} // already indexed
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Failed to ingest file");
                }
            }
        }

        Ok(indexed)
    }

    /// Hybrid search: vector + FTS5 + RRF merge (no temporal decay).
    pub async fn search(
        &mut self,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<RagSearchResult>> {
        let vector_results = self
            .engine
            .search(query, CANDIDATES_PER_SOURCE)
            .await
            .unwrap_or_default();

        let sanitized_query = sanitize_fts5_query(query);
        let fts_results = if sanitized_query.trim().is_empty() {
            Vec::new()
        } else {
            self.db
                .rag_fts5_search(&sanitized_query, CANDIDATES_PER_SOURCE)
                .await
                .unwrap_or_default()
        };

        let merged = rrf_merge(&vector_results, &fts_results, top_k);
        if merged.is_empty() {
            return Ok(Vec::new());
        }

        let chunk_ids: Vec<i64> = merged.iter().map(|&(id, _)| id).collect();
        let chunks = self.db.load_rag_chunks_by_ids(&chunk_ids).await?;

        let chunk_map: HashMap<i64, RagChunkRow> =
            chunks.into_iter().map(|c| (c.id, c)).collect();

        // Load source file names for attribution
        let source_ids: Vec<i64> = chunk_map
            .values()
            .map(|c| c.source_id)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let sources = self.db.list_rag_sources().await.unwrap_or_default();
        let source_map: HashMap<i64, String> = sources
            .into_iter()
            .filter(|s| source_ids.contains(&s.id))
            .map(|s| (s.id, s.file_name))
            .collect();

        let results = merged
            .into_iter()
            .filter_map(|(id, score)| {
                chunk_map.get(&id).map(|chunk| {
                    let mut chunk = chunk.clone();
                    // Redact sensitive chunk content
                    if chunk.sensitive {
                        chunk.content = format!(
                            "[REDACTED — auth required] {} ({} tokens)",
                            chunk.heading, chunk.token_count
                        );
                    }
                    RagSearchResult {
                        source_file: source_map
                            .get(&chunk.source_id)
                            .cloned()
                            .unwrap_or_default(),
                        chunk,
                        score,
                    }
                })
            })
            .collect();

        Ok(results)
    }

    /// Re-ingest a file if its content has changed (for watcher use).
    /// Removes old source if hash changed, then ingests fresh.
    pub async fn reingest_file(
        &mut self,
        path: &Path,
        source_channel: &str,
    ) -> Result<Option<i64>> {
        let content =
            std::fs::read(path).with_context(|| format!("Cannot read {}", path.display()))?;
        let new_hash = hex_sha256(&content);

        if let Some(existing) = self
            .db
            .find_rag_source_by_path(&path.to_string_lossy())
            .await?
        {
            if existing.file_hash == new_hash {
                return Ok(None); // unchanged
            }
            // Hash changed: remove old, re-ingest
            tracing::info!(path = %path.display(), "File modified, re-indexing");
            self.remove_source(existing.id).await?;
        }

        self.ingest_file(path, source_channel).await
    }

    /// Remove a source and its chunks.
    pub async fn remove_source(&mut self, source_id: i64) -> Result<bool> {
        self.db.delete_rag_source(source_id).await
    }

    /// List all indexed sources.
    pub async fn list_sources(&self) -> Result<Vec<RagSourceRow>> {
        self.db.list_rag_sources().await
    }

    /// Get knowledge base stats.
    pub async fn stats(&self) -> Result<RagStats> {
        Ok(RagStats {
            source_count: self.db.count_rag_sources().await.unwrap_or(0),
            chunk_count: self.db.count_rag_chunks().await.unwrap_or(0),
            index_vectors: self.engine.len(),
        })
    }

    /// Rebuild the HNSW index from all chunks in the database.
    pub async fn reindex_all(&mut self) -> Result<usize> {
        let sources = self.db.list_rag_sources().await?;
        let source_map: HashMap<i64, String> = sources
            .iter()
            .map(|s| (s.id, s.file_name.clone()))
            .collect();
        let mut total = 0;

        for source in &sources {
            if source.chunk_count == 0 {
                continue;
            }

            let chunks = self.db.load_rag_chunks_by_source(source.id).await?;
            for chunk in chunks {
                let file_name = source_map.get(&chunk.source_id).cloned().unwrap_or_default();

                // Fix empty headings by prepending filename (for FTS5 matching)
                if chunk.heading.is_empty() && !file_name.is_empty() {
                    let _ = self
                        .db
                        .update_rag_chunk_heading(chunk.id, &file_name)
                        .await;
                }

                let embed_text = format!("{}\n{}", file_name, chunk.content);
                self.engine.index_chunk(chunk.id, &embed_text).await?;
                total += 1;
            }
        }

        self.engine.save()?;
        tracing::info!(vectors = total, "RAG index rebuilt");
        Ok(total)
    }

    /// Reindex if HNSW is empty but DB has chunks (e.g., after restart with missing index file).
    pub async fn reindex_if_needed(&mut self) -> Result<()> {
        let db_chunks = self.db.count_rag_chunks().await.unwrap_or(0);
        let index_vectors = self.engine.len();

        if db_chunks > 0 && index_vectors == 0 {
            tracing::info!(
                db_chunks,
                "HNSW index is empty but DB has chunks — rebuilding"
            );
            self.reindex_all().await?;
        }
        Ok(())
    }

    /// Persist the HNSW index to disk.
    pub fn save_index(&self) -> Result<()> {
        self.engine.save()
    }

    /// Reveal a sensitive chunk's full content (bypasses redaction).
    pub async fn reveal_chunk(&self, chunk_id: i64) -> Result<Option<RagChunkRow>> {
        let chunks = self.db.load_rag_chunks_by_ids(&[chunk_id]).await?;
        Ok(chunks.into_iter().next())
    }
}

fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn sanitize_fts5_query(query: &str) -> String {
    let sanitized: String = query
        .chars()
        .filter(|c| {
            c.is_alphanumeric()
                || *c == ' '
                || *c == '-'
                || *c == '_'
                || *c == '.'
                || *c == ','
                || (*c >= '\u{00e0}' && *c <= '\u{00ff}')
                || (*c >= '\u{00c0}' && *c <= '\u{00df}')
        })
        .collect();

    if sanitized.trim().is_empty() {
        query
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == ' ')
            .collect()
    } else {
        sanitized
    }
}

fn rrf_merge(
    vector_results: &[(i64, f32)],
    fts_results: &[(i64, f64)],
    top_k: usize,
) -> Vec<(i64, f64)> {
    let mut scores: HashMap<i64, f64> = HashMap::new();

    for (rank, &(id, _)) in vector_results.iter().enumerate() {
        *scores.entry(id).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
    }

    for (rank, &(id, _)) in fts_results.iter().enumerate() {
        *scores.entry(id).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
    }

    let mut sorted: Vec<(i64, f64)> = scores.into_iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    sorted.truncate(top_k);
    sorted
}

/// Recursively collect file paths from a directory.
fn walkdir_entries(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut paths = Vec::new();
    walk_recursive(dir, &mut paths)?;
    Ok(paths)
}

fn walk_recursive(dir: &Path, paths: &mut Vec<std::path::PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("Cannot read directory {}", dir.display()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip hidden dirs
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with('.'))
                .unwrap_or(false)
            {
                continue;
            }
            walk_recursive(&path, paths)?;
        } else {
            paths.push(path);
        }
    }

    Ok(())
}
