//! Embedding index management API — status check and reindex.

#[cfg(feature = "embeddings")]
mod inner {
    use std::sync::Arc;

    use axum::extract::State;
    use axum::response::{IntoResponse, Json};
    use axum::routing::get;
    use axum::Router;
    use serde::Serialize;

    use crate::agent::index_meta::IndexMeta;
    use crate::config::Config;
    use crate::web::server::AppState;

    pub(crate) fn routes() -> Router<Arc<AppState>> {
        Router::new()
            .route("/v1/embeddings/status", get(embedding_status))
            .route(
                "/v1/embeddings/reindex",
                axum::routing::post(reindex_embeddings),
            )
    }

    // ─── Response Types ──────────────────────────────────────────

    #[derive(Serialize)]
    struct StatusResponse {
        ok: bool,
        config: EmbeddingConfigInfo,
        memory_index: Option<IndexMeta>,
        rag_index: Option<IndexMeta>,
        memory_chunks_in_db: i64,
        rag_chunks_in_db: i64,
        mismatch: bool,
    }

    #[derive(Serialize)]
    struct EmbeddingConfigInfo {
        provider: String,
        model: String,
        dimensions: usize,
    }

    #[derive(Serialize)]
    struct ReindexResponse {
        ok: bool,
        memory_count: usize,
        rag_count: usize,
        message: String,
    }

    // ─── GET /api/v1/embeddings/status ───────────────────────────

    async fn embedding_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
        let Some(ref db) = state.db else {
            return Json(serde_json::json!({"ok": false, "message": "Database not available"}))
                .into_response();
        };

        // Read current config
        let config = Config::load().unwrap_or_default();
        let mem = &config.memory;

        let config_info = EmbeddingConfigInfo {
            provider: effective_provider(mem),
            model: effective_model(mem),
            dimensions: effective_dimensions(mem),
        };

        // Read index metadata from sidecar files
        let data_dir = Config::data_dir();
        let memory_meta = IndexMeta::read(&data_dir.join("memory.usearch"));
        let rag_meta = IndexMeta::read(&data_dir.join("rag.usearch"));

        // Count chunks in DB
        let memory_chunks = db.count_memory_chunks().await.unwrap_or(0);
        let rag_chunks = db.count_rag_chunks().await.unwrap_or(0);

        // Detect mismatch: compare config vs stored meta
        let mismatch = detect_mismatch(&config_info, &memory_meta, &rag_meta);

        Json(StatusResponse {
            ok: true,
            config: config_info,
            memory_index: memory_meta,
            rag_index: rag_meta,
            memory_chunks_in_db: memory_chunks,
            rag_chunks_in_db: rag_chunks,
            mismatch,
        })
        .into_response()
    }

    // ─── POST /api/v1/embeddings/reindex ─────────────────────────

    async fn reindex_embeddings(State(state): State<Arc<AppState>>) -> impl IntoResponse {
        let config = Config::load().unwrap_or_default();

        let mut memory_count = 0usize;
        let mut rag_count = 0usize;

        // 1. Rebuild memory index
        if let Some(ref searcher_arc) = state.memory_searcher {
            match crate::agent::create_embedding_provider(&config) {
                Ok(provider) => {
                    let mut searcher = searcher_arc.lock().await;
                    if let Err(e) = searcher.reset_engine(provider) {
                        return Json(ReindexResponse {
                            ok: false,
                            memory_count: 0,
                            rag_count: 0,
                            message: format!("Failed to reset memory engine: {e}"),
                        })
                        .into_response();
                    }
                    match searcher.reindex_all().await {
                        Ok(count) => memory_count = count,
                        Err(e) => {
                            return Json(ReindexResponse {
                                ok: false,
                                memory_count: 0,
                                rag_count: 0,
                                message: format!("Failed to reindex memory: {e}"),
                            })
                            .into_response();
                        }
                    }
                }
                Err(e) => {
                    return Json(ReindexResponse {
                        ok: false,
                        memory_count: 0,
                        rag_count: 0,
                        message: format!("Failed to create embedding provider: {e}"),
                    })
                    .into_response();
                }
            }
        }

        // 2. Rebuild RAG index
        if let Some(ref rag_arc) = state.rag_engine {
            match crate::agent::create_embedding_provider(&config) {
                Ok(provider) => {
                    let mut rag = rag_arc.lock().await;
                    if let Err(e) = rag.reset_engine(provider) {
                        return Json(ReindexResponse {
                            ok: false,
                            memory_count,
                            rag_count: 0,
                            message: format!("Failed to reset RAG engine: {e}"),
                        })
                        .into_response();
                    }
                    match rag.reindex_all().await {
                        Ok(count) => rag_count = count,
                        Err(e) => {
                            return Json(ReindexResponse {
                                ok: false,
                                memory_count,
                                rag_count: 0,
                                message: format!("Failed to reindex RAG: {e}"),
                            })
                            .into_response();
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to create RAG embedding provider");
                    // Non-fatal — memory was already rebuilt
                }
            }
        }

        tracing::info!(memory_count, rag_count, "Embedding indices rebuilt");

        Json(ReindexResponse {
            ok: true,
            memory_count,
            rag_count,
            message: format!(
                "Rebuilt: {memory_count} memory + {rag_count} RAG chunks re-embedded."
            ),
        })
        .into_response()
    }

    // ─── Helpers ─────────────────────────────────────────────────

    /// Resolve effective provider name (config field or default).
    fn effective_provider(mem: &crate::config::MemoryConfig) -> String {
        if mem.embedding_provider.is_empty() {
            "ollama".into()
        } else {
            mem.embedding_provider.clone()
        }
    }

    /// Resolve effective model name (config field or provider default).
    fn effective_model(mem: &crate::config::MemoryConfig) -> String {
        if !mem.embedding_model.is_empty() {
            return mem.embedding_model.clone();
        }
        // Match the same defaults as create_embedding_provider()
        match mem.embedding_provider.as_str() {
            "openai" => "text-embedding-3-small".into(),
            "mistral" => "mistral-embed".into(),
            "cohere" => "embed-english-v3.0".into(),
            "together" => "togethercomputer/m2-bert-80M-8k-retrieval".into(),
            "fireworks" => "nomic-ai/nomic-embed-text-v1.5".into(),
            "ollama_cloud" => "nomic-embed-text".into(),
            _ => "nomic-embed-text".into(),
        }
    }

    /// Resolve effective dimensions (config field or default 384).
    fn effective_dimensions(mem: &crate::config::MemoryConfig) -> usize {
        if mem.embedding_dimensions > 0 {
            mem.embedding_dimensions
        } else {
            384
        }
    }

    /// Detect mismatch between current config and stored index metadata.
    fn detect_mismatch(
        config: &EmbeddingConfigInfo,
        memory_meta: &Option<IndexMeta>,
        rag_meta: &Option<IndexMeta>,
    ) -> bool {
        let check = |meta: &Option<IndexMeta>| -> bool {
            if let Some(m) = meta {
                m.provider != config.provider
                    || m.model != config.model
                    || m.dimensions != config.dimensions
            } else {
                false // No meta = unknown, not a "mismatch" per se
            }
        };
        check(memory_meta) || check(rag_meta)
    }
}

#[cfg(feature = "embeddings")]
pub(super) use inner::routes;
