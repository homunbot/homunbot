//! Embedding engine — local ONNX embedding via fastembed + HNSW vector index via USearch.
//!
//! Lazy-initialized: the ONNX model (~30MB) is downloaded on first use.
//! The HNSW index is persisted to `~/.homun/memory.usearch`.

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use usearch::ffi::{IndexOptions, MetricKind, ScalarKind};

use crate::config::Config;

/// Dimensionality of the embedding model (AllMiniLML6V2Q = 384)
const EMBEDDING_DIM: usize = 384;

/// Embedding engine wrapping fastembed (ONNX) + USearch (HNSW).
///
/// Thread-safe: fastembed and usearch are both Send + Sync.
pub struct EmbeddingEngine {
    model: TextEmbedding,
    index: usearch::Index,
    index_path: PathBuf,
    count: usize,
}

impl EmbeddingEngine {
    /// Create a new embedding engine, loading or creating the HNSW index.
    ///
    /// On first run, downloads the ONNX model (~30MB) to the cache dir.
    /// If a persisted index exists at `~/.homun/memory.usearch`, it's loaded.
    pub fn new() -> Result<Self> {
        let data_dir = Config::data_dir();
        let index_path = data_dir.join("memory.usearch");

        // Initialize fastembed with a small multilingual model
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::AllMiniLML6V2Q).with_show_download_progress(true),
        )
        .context("Failed to initialize embedding model")?;

        // Create or load the USearch HNSW index
        let options = IndexOptions {
            dimensions: EMBEDDING_DIM,
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            connectivity: 16,       // HNSW connectivity parameter (M)
            expansion_add: 128,     // ef_construction
            expansion_search: 64,   // ef_search
            multi: false,
        };

        let index = usearch::new_index(&options)
            .map_err(|e| anyhow::anyhow!("Failed to create USearch index: {e}"))?;

        let mut count = 0;

        // Load existing index if present
        if index_path.exists() {
            index
                .load(index_path.to_str().unwrap_or("memory.usearch"))
                .map_err(|e| anyhow::anyhow!("Failed to load USearch index: {e}"))?;
            count = index.size();

            // Ensure there's capacity for new additions — loaded indexes
            // may have zero capacity left, which causes segfaults on add().
            let capacity = index.capacity();
            if capacity < count + 100 {
                index
                    .reserve(count + 1000)
                    .map_err(|e| anyhow::anyhow!("Failed to reserve USearch capacity: {e}"))?;
            }

            tracing::info!(
                vectors = count,
                capacity = index.capacity(),
                path = %index_path.display(),
                "Loaded HNSW vector index"
            );
        } else {
            // Reserve initial capacity
            index
                .reserve(1000)
                .map_err(|e| anyhow::anyhow!("Failed to reserve USearch capacity: {e}"))?;
            tracing::info!("Created new HNSW vector index");
        }

        Ok(Self {
            model,
            index,
            index_path,
            count,
        })
    }

    /// Generate embeddings for a single text.
    pub fn embed_text(&mut self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self
            .model
            .embed(vec![text], None)
            .context("Failed to generate embedding")?;

        embeddings
            .into_iter()
            .next()
            .context("No embedding returned")
    }

    /// Add a memory chunk to the HNSW index.
    /// `chunk_id` is the SQLite row ID from `memory_chunks`.
    pub fn index_chunk(&mut self, chunk_id: i64, text: &str) -> Result<()> {
        let embedding = self.embed_text(text)?;

        // USearch uses u64 keys
        let key = chunk_id as u64;

        self.index
            .add(key, &embedding)
            .map_err(|e| anyhow::anyhow!("Failed to add vector to index: {e}"))?;

        self.count += 1;

        // Auto-save every 50 additions (cheap for HNSW)
        if self.count % 50 == 0 {
            self.save()?;
        }

        Ok(())
    }

    /// Search for the nearest neighbors of a query text.
    /// Returns `(chunk_id, distance)` pairs, sorted by relevance (lowest distance = best).
    pub fn search(&mut self, query: &str, top_k: usize) -> Result<Vec<(i64, f32)>> {
        if self.count == 0 {
            return Ok(Vec::new());
        }

        let embedding = self.embed_text(query)?;

        let results = self
            .index
            .search(&embedding, top_k)
            .map_err(|e| anyhow::anyhow!("Vector search failed: {e}"))?;

        Ok(results
            .keys
            .iter()
            .zip(results.distances.iter())
            .map(|(&key, &dist)| (key as i64, dist))
            .collect())
    }

    /// Check if a similar chunk already exists in the index.
    /// Returns `Some((chunk_id, distance))` if found within threshold, `None` otherwise.
    ///
    /// Distance threshold: 0.0 = identical, 0.15 ≈ 85% similarity for cosine.
    /// Use 0.15 for strict dedup, 0.25 for looser matching.
    pub fn find_similar(&mut self, text: &str, distance_threshold: f32) -> Result<Option<(i64, f32)>> {
        if self.count == 0 {
            return Ok(None);
        }

        let results = self.search(text, 1)?;
        if let Some((chunk_id, distance)) = results.first() {
            if *distance <= distance_threshold {
                return Ok(Some((*chunk_id, *distance)));
            }
        }
        Ok(None)
    }

    /// Persist the HNSW index to disk.
    pub fn save(&self) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.index_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create data directory for index")?;
        }

        self.index
            .save(self.index_path.to_str().unwrap_or("memory.usearch"))
            .map_err(|e| anyhow::anyhow!("Failed to save USearch index: {e}"))?;

        tracing::debug!(
            vectors = self.count,
            path = %self.index_path.display(),
            "Saved HNSW vector index"
        );
        Ok(())
    }

    /// Number of vectors currently in the index.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

impl Drop for EmbeddingEngine {
    fn drop(&mut self) {
        // Best-effort save on shutdown
        if let Err(e) = self.save() {
            tracing::warn!(error = %e, "Failed to save vector index on shutdown");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_dim() {
        assert_eq!(EMBEDDING_DIM, 384);
    }
}
