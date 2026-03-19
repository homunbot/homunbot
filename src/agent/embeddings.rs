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
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use lru::LruCache;
use usearch::ffi::{IndexOptions, MetricKind, ScalarKind};

use crate::config::Config;

use super::index_meta::IndexMeta;

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

    /// Provider name for logging (e.g. "ollama", "openai").
    fn name(&self) -> &str;

    /// Model identifier (e.g. "nomic-embed-text", "text-embedding-3-small").
    fn model_name(&self) -> &str;
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

        let mut req = self.client.post(format!("{}/embeddings", self.api_base));

        // Only send Authorization header if an API key is set (Ollama doesn't need one)
        if !self.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let url = format!("{}/embeddings", self.api_base);
        let response = req.json(&body).send().await.with_context(|| {
            format!(
                "Embedding API request to {} failed (provider={}, model={})",
                url, self.provider_name, self.model
            )
        })?;

        // Read body before status check — error responses often contain useful messages
        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "Embedding API {} returned {}: {} (provider={}, model={})",
                url,
                status,
                error_body,
                self.provider_name,
                self.model
            );
        }

        let resp = response
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

    fn model_name(&self) -> &str {
        &self.model
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
    /// Tracking fields for IndexMeta sidecar (detect model changes).
    provider_name: String,
    model_name: String,
    dims: usize,
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

        let provider_name = provider.name().to_string();
        let model_name = provider.model_name().to_string();
        let dims = provider.dimensions();

        Ok(Self {
            provider,
            cache: LruCache::new(cache_cap),
            index,
            index_path,
            count,
            provider_name,
            model_name,
            dims,
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

        // Write sidecar metadata (best-effort — non-critical)
        let meta = IndexMeta {
            provider: self.provider_name.clone(),
            model: self.model_name.clone(),
            dimensions: self.dims,
            chunk_count: self.count,
            built_at: chrono::Utc::now().to_rfc3339(),
        };
        if let Err(e) = meta.write(&self.index_path) {
            tracing::warn!(error = %e, "Failed to write index metadata sidecar");
        }

        tracing::debug!(
            vectors = self.count,
            path = %self.index_path.display(),
            "Saved HNSW vector index"
        );
        Ok(())
    }

    /// Remove a vector from the HNSW index by its chunk ID.
    ///
    /// USearch supports lazy removal — the slot is marked as deleted
    /// and reclaimed on the next save/load cycle.
    pub fn remove(&mut self, chunk_id: i64) {
        if let Err(e) = self.index.remove(chunk_id as u64) {
            tracing::debug!(chunk_id, error = %e, "Failed to remove chunk from HNSW (may not exist)");
        } else if self.count > 0 {
            self.count -= 1;
        }
    }

    /// Number of vectors currently in the index.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Current index metadata (for status reporting and mismatch detection).
    pub fn index_meta(&self) -> IndexMeta {
        IndexMeta {
            provider: self.provider_name.clone(),
            model: self.model_name.clone(),
            dimensions: self.dims,
            chunk_count: self.count,
            built_at: String::new(), // Filled on save
        }
    }

    /// Path to the HNSW index file on disk.
    pub fn index_path(&self) -> &Path {
        &self.index_path
    }

    /// Replace the internal provider and create a fresh HNSW index.
    ///
    /// Deletes the old `.usearch` and `.meta` files, creates a new empty
    /// index with the new provider's dimensions. Used by the reindex flow
    /// when the user changes embedding model in Settings.
    pub fn reset_with_provider(&mut self, provider: Box<dyn EmbeddingProvider>) -> Result<()> {
        // Delete existing index + meta files
        if self.index_path.exists() {
            std::fs::remove_file(&self.index_path).with_context(|| {
                format!("Failed to remove old index: {}", self.index_path.display())
            })?;
        }
        IndexMeta::delete(&self.index_path);

        // Create fresh HNSW index with new dimensions
        let options = IndexOptions {
            dimensions: provider.dimensions(),
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            connectivity: 16,
            expansion_add: 128,
            expansion_search: 64,
            multi: false,
        };

        let index = usearch::new_index(&options)
            .map_err(|e| anyhow::anyhow!("Failed to create new USearch index: {e}"))?;
        index
            .reserve(1000)
            .map_err(|e| anyhow::anyhow!("Failed to reserve capacity: {e}"))?;

        // Swap all internals
        self.provider_name = provider.name().to_string();
        self.model_name = provider.model_name().to_string();
        self.dims = provider.dimensions();
        self.provider = provider;
        self.index = index;
        self.count = 0;
        self.cache = LruCache::new(
            NonZeroUsize::new(CACHE_CAPACITY).expect("CACHE_CAPACITY must be non-zero"),
        );

        tracing::info!(
            provider = %self.provider_name,
            model = %self.model_name,
            dimensions = self.dims,
            "Embedding engine reset with new provider"
        );

        Ok(())
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
/// Named providers supply sensible defaults; API key resolution handles encrypted
/// vault keys (from Web UI config) via `global_secrets()`.
pub fn create_embedding_provider(config: &Config) -> Result<Box<dyn EmbeddingProvider>> {
    let mem = &config.memory;
    let dims = if mem.embedding_dimensions > 0 {
        mem.embedding_dimensions
    } else {
        DEFAULT_EMBEDDING_DIM
    };

    // Provider-specific defaults: (name, default_model, default_api_base, needs_key)
    let (provider_name, default_model, default_base, needs_key) =
        match mem.embedding_provider.as_str() {
            "openai" => (
                "openai",
                "text-embedding-3-small",
                "https://api.openai.com/v1",
                true,
            ),
            "mistral" => (
                "mistral",
                "mistral-embed",
                "https://api.mistral.ai/v1",
                true,
            ),
            "cohere" => (
                "cohere",
                "embed-english-v3.0",
                "https://api.cohere.ai/v1",
                true,
            ),
            "together" => (
                "together",
                "togethercomputer/m2-bert-80M-8k-retrieval",
                "https://api.together.xyz/v1",
                true,
            ),
            "fireworks" => (
                "fireworks",
                "nomic-ai/nomic-embed-text-v1.5",
                "https://api.fireworks.ai/inference/v1",
                true,
            ),
            "ollama_cloud" => (
                "ollama_cloud",
                "nomic-embed-text",
                "https://ollama.com/v1",
                true,
            ),
            _ => (
                "ollama",
                "nomic-embed-text",
                "http://localhost:11434/v1",
                false,
            ),
        };

    // Resolve API key: embedding field → LLM provider (vault-aware) → empty
    let api_key = if !mem.embedding_api_key.is_empty() {
        mem.embedding_api_key.clone()
    } else {
        resolve_provider_api_key(config, provider_name)
    };

    // Resolve API base: embedding field → LLM provider (with /v1) → default
    //
    // LLM provider configs store base URLs without /v1 (e.g. "http://localhost:11434")
    // because each provider appends its own path (/api/chat, /v1/chat/completions, etc.).
    // But the embedding protocol always uses /v1/embeddings, so we must ensure /v1 is present.
    let api_base = if !mem.embedding_api_base.is_empty() {
        mem.embedding_api_base.clone()
    } else {
        let base = config
            .providers
            .get(provider_name)
            .and_then(|p| p.api_base.clone())
            .unwrap_or_else(|| default_base.to_string());

        // Append /v1 if the base doesn't already end with it
        if !base.ends_with("/v1") && !base.contains("/v1/") {
            format!("{}/v1", base.trim_end_matches('/'))
        } else {
            base
        }
    };

    let model = if !mem.embedding_model.is_empty() {
        mem.embedding_model.clone()
    } else {
        default_model.to_string()
    };

    if api_key.is_empty() && needs_key {
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

/// Resolve the API key for a provider, handling encrypted vault keys.
///
/// When configured via the Web UI, keys are stored encrypted in the vault
/// and `config.toml` contains the marker `"***ENCRYPTED***"`. This function
/// resolves the real key from `global_secrets()`.
fn resolve_provider_api_key(config: &Config, provider_name: &str) -> String {
    let prov = match config.providers.get(provider_name) {
        Some(p) => p,
        None => return String::new(),
    };

    if prov.api_key.is_empty() {
        return String::new();
    }

    // If the key is the encrypted marker, resolve from vault
    if prov.api_key == "***ENCRYPTED***" {
        if let Ok(secrets) = crate::storage::global_secrets() {
            let key = crate::storage::SecretKey::provider_api_key(provider_name);
            if let Ok(Some(real_key)) = secrets.get(&key) {
                return real_key;
            }
        }
        tracing::warn!(
            provider = provider_name,
            "Encrypted API key marker found but could not resolve from vault"
        );
        return String::new();
    }

    prov.api_key.clone()
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
