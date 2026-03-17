//! Embedding engine — pluggable provider + HNSW vector index.
//!
//! The provider is selected via `config.memory.embedding_provider`:
//! - `"local"` (default): fastembed AllMiniLML6V2Q, 384-dim, ~30MB ONNX model.
//! - `"openai"`: OpenAI text-embedding-3-small via API.
//! - `"ollama"`: Ollama-served model via OpenAI-compatible `/v1/embeddings`.
//!
//! All providers produce vectors of `config.memory.embedding_dimensions` (default 384).
//! An LRU cache (512 entries) prevents redundant embedding calls.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use lru::LruCache;
use usearch::ffi::{IndexOptions, MetricKind, ScalarKind};

use crate::config::Config;

/// Default dimensionality — used when config doesn't specify.
const DEFAULT_EMBEDDING_DIM: usize = 384;

/// LRU cache capacity — balances memory vs. hit rate.
const CACHE_CAPACITY: usize = 512;

// ─── Provider Trait ────────────────────────────────────────────

/// Abstraction over embedding backends (local ONNX vs. API).
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embeddings for one or more texts.
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Embedding dimensionality (must be EMBEDDING_DIM for HNSW compat).
    fn dimensions(&self) -> usize;

    /// Provider name for logging.
    fn name(&self) -> &str;
}

// ─── Local Provider (fastembed ONNX) ───────────────────────────

/// Local embedding via fastembed — runs the AllMiniLML6V2Q ONNX model on CPU.
/// Model is downloaded (~30MB) on first use, then cached.
pub struct LocalEmbeddingProvider {
    /// Mutex because `TextEmbedding::embed()` takes `&mut self`.
    /// Only locked inside `spawn_blocking` — no async contention.
    model: Arc<std::sync::Mutex<TextEmbedding>>,
}

impl LocalEmbeddingProvider {
    pub fn new() -> Result<Self> {
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::AllMiniLML6V2Q).with_show_download_progress(true),
        )
        .context("Failed to initialize local embedding model")?;

        Ok(Self {
            model: Arc::new(std::sync::Mutex::new(model)),
        })
    }
}

#[async_trait]
impl EmbeddingProvider for LocalEmbeddingProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let model = self.model.clone();
        let texts = texts.to_vec();

        tokio::task::spawn_blocking(move || {
            let str_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
            let mut model = model
                .lock()
                .map_err(|e| anyhow::anyhow!("Model lock poisoned: {e}"))?;
            model
                .embed(str_refs, None)
                .context("fastembed embedding failed")
        })
        .await
        .context("Blocking embedding task panicked")?
    }

    fn dimensions(&self) -> usize {
        DEFAULT_EMBEDDING_DIM
    }

    fn name(&self) -> &str {
        "local"
    }
}

// ─── API Provider (OpenAI-compatible) ─────────────────────────

/// Embedding via any OpenAI-compatible `/v1/embeddings` endpoint.
/// Works with OpenAI, Ollama, HuggingFace TEI, and other compatible APIs.
pub struct ApiEmbeddingProvider {
    client: reqwest::Client,
    api_key: String,
    api_base: String,
    model: String,
    dimensions: usize,
    provider_name: String,
}

impl ApiEmbeddingProvider {
    pub fn new(
        api_key: String,
        api_base: String,
        model: String,
        dimensions: usize,
        provider_name: String,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            api_base,
            model,
            dimensions,
            provider_name,
        }
    }
}

/// OpenAI /v1/embeddings response format.
#[derive(serde::Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(serde::Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for ApiEmbeddingProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
            "dimensions": self.dimensions,
        });

        let mut req = self
            .client
            .post(format!("{}/embeddings", self.api_base));

        // Only send Authorization header if an API key is set (Ollama doesn't need one)
        if !self.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let resp = req
            .json(&body)
            .send()
            .await
            .context("Embedding API request failed")?
            .error_for_status()
            .context("Embedding API error")?
            .json::<EmbeddingResponse>()
            .await
            .context("Failed to parse embedding API response")?;

        Ok(resp.data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn name(&self) -> &str {
        &self.provider_name
    }
}

// ─── Embedding Engine ──────────────────────────────────────────

/// Embedding engine wrapping a pluggable provider + HNSW vector index + LRU cache.
///
/// The provider generates 384-dim vectors; USearch stores and searches them.
/// The LRU cache prevents redundant embedding calls for identical texts.
pub struct EmbeddingEngine {
    provider: Box<dyn EmbeddingProvider>,
    cache: LruCache<String, Vec<f32>>,
    index: usearch::Index,
    index_path: PathBuf,
    count: usize,
}

impl EmbeddingEngine {
    /// Create a new embedding engine with the configured provider.
    ///
    /// Selects local (fastembed) or OpenAI based on `config.memory.embedding_provider`.
    /// Falls back to local if OpenAI API key is missing.
    pub fn new(config: &Config) -> Result<Self> {
        let provider = create_embedding_provider(config)?;

        tracing::info!(
            provider = provider.name(),
            dimensions = provider.dimensions(),
            "Embedding provider initialized"
        );

        Self::with_provider(provider)
    }

    /// Create an engine with a specific provider (for testing or custom backends).
    /// Uses the default index path (`~/.homun/memory.usearch`).
    pub fn with_provider(provider: Box<dyn EmbeddingProvider>) -> Result<Self> {
        let data_dir = Config::data_dir();
        let index_path = data_dir.join("memory.usearch");
        Self::with_provider_and_path(provider, index_path)
    }

    /// Create an engine with a specific provider and custom HNSW index path.
    /// Used by RAG to maintain a separate index (`rag.usearch`).
    pub fn with_provider_and_path(
        provider: Box<dyn EmbeddingProvider>,
        index_path: PathBuf,
    ) -> Result<Self> {
        // Create or load the USearch HNSW index
        let options = IndexOptions {
            dimensions: provider.dimensions(),
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            connectivity: 16,     // HNSW connectivity parameter (M)
            expansion_add: 128,   // ef_construction
            expansion_search: 64, // ef_search
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

        let cache_cap = NonZeroUsize::new(CACHE_CAPACITY).expect("CACHE_CAPACITY must be non-zero");

        Ok(Self {
            provider,
            cache: LruCache::new(cache_cap),
            index,
            index_path,
            count,
        })
    }

    /// Generate embeddings for a single text (cached).
    pub async fn embed_text(&mut self, text: &str) -> Result<Vec<f32>> {
        // Check cache first
        if let Some(cached) = self.cache.get(text) {
            return Ok(cached.clone());
        }

        // Generate via provider
        let texts = vec![text.to_string()];
        let mut results = self.provider.embed(&texts).await?;
        let embedding = results
            .pop()
            .context("No embedding returned from provider")?;

        // Cache the result
        self.cache.put(text.to_string(), embedding.clone());

        Ok(embedding)
    }

    /// Add a memory chunk to the HNSW index.
    /// `chunk_id` is the SQLite row ID from `memory_chunks`.
    pub async fn index_chunk(&mut self, chunk_id: i64, text: &str) -> Result<()> {
        let embedding = self.embed_text(text).await?;

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
    pub async fn search(&mut self, query: &str, top_k: usize) -> Result<Vec<(i64, f32)>> {
        if self.count == 0 {
            return Ok(Vec::new());
        }

        let embedding = self.embed_text(query).await?;

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
    pub async fn find_similar(
        &mut self,
        text: &str,
        distance_threshold: f32,
    ) -> Result<Option<(i64, f32)>> {
        if self.count == 0 {
            return Ok(None);
        }

        let results = self.search(text, 1).await?;
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
            std::fs::create_dir_all(parent).context("Failed to create data directory for index")?;
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

// ─── Factory ───────────────────────────────────────────────────

/// Create the appropriate embedding provider based on config.
///
/// - `"local"` (default) → fastembed ONNX model (384-dim).
/// - `"openai"` → OpenAI API (text-embedding-3-small).
/// - `"ollama"` → Ollama via OpenAI-compatible endpoint.
pub fn create_embedding_provider(config: &Config) -> Result<Box<dyn EmbeddingProvider>> {
    let mem = &config.memory;
    let dims = if mem.embedding_dimensions > 0 {
        mem.embedding_dimensions
    } else {
        DEFAULT_EMBEDDING_DIM
    };

    match mem.embedding_provider.as_str() {
        "ollama" => {
            let api_base = if mem.embedding_api_base.is_empty() {
                "http://localhost:11434/v1".to_string()
            } else {
                mem.embedding_api_base.clone()
            };
            let model = if mem.embedding_model.is_empty() {
                "nomic-embed-text".to_string()
            } else {
                mem.embedding_model.clone()
            };
            tracing::info!(
                %api_base, %model, dims,
                "Using Ollama embedding provider"
            );
            Ok(Box::new(ApiEmbeddingProvider::new(
                String::new(), // Ollama doesn't need an API key
                api_base, model, dims, "ollama".into(),
            )))
        }
        "openai" => {
            let api_key = config.providers.openai.api_key.clone();
            if api_key.is_empty() {
                tracing::warn!(
                    "OpenAI embedding requested but no API key configured, falling back to local"
                );
                return Ok(Box::new(LocalEmbeddingProvider::new()?));
            }
            let api_base = if mem.embedding_api_base.is_empty() {
                config.providers.openai.api_base.clone()
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
            } else {
                mem.embedding_api_base.clone()
            };
            let model = if mem.embedding_model.is_empty() {
                "text-embedding-3-small".to_string()
            } else {
                mem.embedding_model.clone()
            };
            tracing::info!(
                %api_base, %model, dims,
                "Using OpenAI embedding provider"
            );
            Ok(Box::new(ApiEmbeddingProvider::new(
                api_key, api_base, model, dims, "openai".into(),
            )))
        }
        _ => {
            tracing::info!("Using local embedding provider (fastembed AllMiniLML6V2Q)");
            Ok(Box::new(LocalEmbeddingProvider::new()?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_dim() {
        assert_eq!(DEFAULT_EMBEDDING_DIM, 384);
    }

    #[test]
    fn test_cache_capacity() {
        assert!(CACHE_CAPACITY > 0);
    }
}
