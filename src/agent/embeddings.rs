//! Embedding engine — pluggable provider + HNSW vector index.
//!
//! The provider is selected via `config.memory.embedding_provider`:
//! - `"ollama"` (default): local Ollama via OpenAI-compatible `/v1/embeddings`.
//! - `"openai"`: OpenAI text-embedding-3-small.
//! - `"mistral"`: Mistral mistral-embed.
//! - Any other value defaults to Ollama.
//!
//! All providers use the OpenAI-compatible `/v1/embeddings` protocol.
//! API key resolution: `embedding_api_key` → matching LLM provider key → empty.
//! Vectors are `config.memory.embedding_dimensions` (default 384).
//! An LRU cache (512 entries) prevents redundant embedding calls.

use std::num::NonZeroUsize;
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use lru::LruCache;
use usearch::ffi::{IndexOptions, MetricKind, ScalarKind};

use crate::config::Config;

/// Default dimensionality — used when config doesn't specify.
const DEFAULT_EMBEDDING_DIM: usize = 384;

/// LRU cache capacity — balances memory vs. hit rate.
const CACHE_CAPACITY: usize = 512;

// ─── Provider Trait ────────────────────────────────────────────

/// Abstraction over embedding backends.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embeddings for one or more texts.
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Embedding dimensionality (must be EMBEDDING_DIM for HNSW compat).
    fn dimensions(&self) -> usize;

    /// Provider name for logging.
    fn name(&self) -> &str;
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
    /// Selects Ollama or OpenAI based on `config.memory.embedding_provider`.
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
/// All providers use `ApiEmbeddingProvider` (OpenAI-compatible `/v1/embeddings`).
/// Named providers (ollama, openai, mistral) supply sensible defaults;
/// API key resolution: `embedding_api_key` → matching LLM provider key → empty.
pub fn create_embedding_provider(config: &Config) -> Result<Box<dyn EmbeddingProvider>> {
    let mem = &config.memory;
    let dims = if mem.embedding_dimensions > 0 {
        mem.embedding_dimensions
    } else {
        DEFAULT_EMBEDDING_DIM
    };

    // Provider-specific defaults: (provider_name, default_model, default_api_base, llm_provider)
    let (provider_name, default_model, default_base, llm_provider) =
        match mem.embedding_provider.as_str() {
            "openai" => (
                "openai",
                "text-embedding-3-small",
                "https://api.openai.com/v1",
                Some(&config.providers.openai),
            ),
            "mistral" => (
                "mistral",
                "mistral-embed",
                "https://api.mistral.ai/v1",
                Some(&config.providers.mistral),
            ),
            // Default: Ollama (free, local, no API key needed)
            _ => (
                "ollama",
                "nomic-embed-text",
                "http://localhost:11434/v1",
                None, // Ollama doesn't need an API key
            ),
        };

    // 3-level fallback: embedding field → LLM provider field → default
    let api_key = if !mem.embedding_api_key.is_empty() {
        mem.embedding_api_key.clone()
    } else if let Some(prov) = llm_provider {
        prov.api_key.clone()
    } else {
        String::new()
    };

    let api_base = if !mem.embedding_api_base.is_empty() {
        mem.embedding_api_base.clone()
    } else if let Some(prov) = llm_provider {
        prov.api_base.clone().unwrap_or_else(|| default_base.to_string())
    } else {
        default_base.to_string()
    };

    let model = if !mem.embedding_model.is_empty() {
        mem.embedding_model.clone()
    } else {
        default_model.to_string()
    };

    // Require API key for providers that need one
    if api_key.is_empty() && llm_provider.is_some() {
        anyhow::bail!(
            "{provider_name} embedding requires an API key. \
             Set memory.embedding_api_key or providers.{provider_name}.api_key, \
             or switch to embedding_provider = \"ollama\"."
        );
    }

    tracing::info!(
        provider = provider_name, %api_base, %model, dims,
        "Embedding provider initialized"
    );

    Ok(Box::new(ApiEmbeddingProvider::new(
        api_key,
        api_base,
        model,
        dims,
        provider_name.into(),
    )))
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
