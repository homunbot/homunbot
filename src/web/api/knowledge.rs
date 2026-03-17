#[cfg(feature = "embeddings")]
mod inner {
    use std::collections::HashMap;
    use std::sync::Arc;

    use axum::extract::{Multipart, Query, State};
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Json};
    use axum::routing::get;
    use axum::Router;

    use crate::web::server::AppState;

    pub(crate) fn routes() -> Router<Arc<AppState>> {
        Router::new()
            .route("/v1/knowledge/stats", get(knowledge_stats))
            .route(
                "/v1/knowledge/sources",
                get(list_knowledge_sources).delete(delete_knowledge_source),
            )
            .route("/v1/knowledge/search", get(search_knowledge))
            .route(
                "/v1/knowledge/ingest",
                axum::routing::post(ingest_knowledge),
            )
            .route(
                "/v1/knowledge/ingest-directory",
                axum::routing::post(ingest_knowledge_directory),
            )
            .route(
                "/v1/knowledge/reveal",
                axum::routing::post(reveal_knowledge_chunk),
            )
    }

    /// GET /api/v1/knowledge/stats
    async fn knowledge_stats(State(state): State<Arc<AppState>>) -> impl IntoResponse {
        let Some(ref rag) = state.rag_engine else {
            return Json(serde_json::json!({"error": "Knowledge base not initialized"}))
                .into_response();
        };
        let engine = rag.lock().await;
        match engine.stats().await {
            Ok(stats) => Json(serde_json::json!(stats)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response(),
        }
    }

    /// GET /api/v1/knowledge/sources
    async fn list_knowledge_sources(State(state): State<Arc<AppState>>) -> impl IntoResponse {
        let Some(ref rag) = state.rag_engine else {
            return Json(serde_json::json!({"error": "Knowledge base not initialized"}))
                .into_response();
        };
        let engine = rag.lock().await;
        match engine.list_sources().await {
            Ok(sources) => Json(serde_json::json!({"sources": sources})).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response(),
        }
    }

    /// DELETE /api/v1/knowledge/sources?id=N
    async fn delete_knowledge_source(
        State(state): State<Arc<AppState>>,
        Query(params): Query<HashMap<String, String>>,
    ) -> impl IntoResponse {
        let Some(ref rag) = state.rag_engine else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "Knowledge base not initialized"})),
            )
                .into_response();
        };

        let Some(id_str) = params.get("id") else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Missing 'id' parameter"})),
            )
                .into_response();
        };

        let Ok(source_id) = id_str.parse::<i64>() else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid 'id' parameter"})),
            )
                .into_response();
        };

        let mut engine = rag.lock().await;
        match engine.remove_source(source_id).await {
            Ok(_) => Json(serde_json::json!({"ok": true})).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response(),
        }
    }

    /// GET /api/v1/knowledge/search?q=...&limit=5
    async fn search_knowledge(
        State(state): State<Arc<AppState>>,
        Query(params): Query<HashMap<String, String>>,
    ) -> impl IntoResponse {
        let Some(ref rag) = state.rag_engine else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "Knowledge base not initialized"})),
            )
                .into_response();
        };

        let Some(query) = params.get("q") else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Missing 'q' parameter"})),
            )
                .into_response();
        };

        let limit = params
            .get("limit")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(5);

        let mut engine = rag.lock().await;
        match engine.search(query, limit).await {
            Ok(results) => {
                let items: Vec<serde_json::Value> = results
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "source_file": r.source_file,
                            "chunk_index": r.chunk.chunk_index,
                            "heading": r.chunk.heading,
                            "content": r.chunk.content,
                            "score": r.score,
                            "sensitive": r.chunk.sensitive,
                            "chunk_id": r.chunk.id,
                        })
                    })
                    .collect();
                Json(serde_json::json!({"results": items})).into_response()
            }
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response(),
        }
    }

    /// POST /api/v1/knowledge/ingest -- multipart file upload
    async fn ingest_knowledge(
        State(state): State<Arc<AppState>>,
        mut multipart: Multipart,
    ) -> impl IntoResponse {
        let Some(ref rag) = state.rag_engine else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "Knowledge base not initialized"})),
            )
                .into_response();
        };

        let mut ingested = Vec::new();
        let mut errors = Vec::new();

        while let Ok(Some(field)) = multipart.next_field().await {
            let file_name = field.file_name().unwrap_or("upload.txt").to_string();

            let Ok(bytes) = field.bytes().await else {
                errors.push(format!("{file_name}: failed to read upload"));
                continue;
            };

            // Write to a temp file so RagEngine can process it
            let tmp_dir = std::env::temp_dir().join("homun_uploads");
            if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
                errors.push(format!("{file_name}: {e}"));
                continue;
            }
            let tmp_path = tmp_dir.join(&file_name);
            if let Err(e) = std::fs::write(&tmp_path, &bytes) {
                errors.push(format!("{file_name}: {e}"));
                continue;
            }

            let mut engine = rag.lock().await;
            match engine.ingest_file(&tmp_path, "web").await {
                Ok(Some(id)) => {
                    ingested.push(serde_json::json!({"file": file_name, "source_id": id}))
                }
                Ok(None) => {
                    ingested.push(serde_json::json!({"file": file_name, "status": "duplicate"}))
                }
                Err(e) => errors.push(format!("{file_name}: {e}")),
            }

            // Clean up temp file
            let _ = std::fs::remove_file(&tmp_path);
        }

        Json(serde_json::json!({
            "ingested": ingested,
            "errors": errors,
        }))
        .into_response()
    }

    /// POST /api/v1/knowledge/ingest-directory -- index a server-side folder
    async fn ingest_knowledge_directory(
        State(state): State<Arc<AppState>>,
        Json(req): Json<serde_json::Value>,
    ) -> impl IntoResponse {
        let Some(ref rag) = state.rag_engine else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "Knowledge base not initialized"})),
            )
                .into_response();
        };

        let path_str = req["path"].as_str().unwrap_or("");
        if path_str.is_empty() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Missing 'path' field"})),
            )
                .into_response();
        }
        let recursive = req["recursive"].as_bool().unwrap_or(false);

        // Expand tilde
        let path = if let Some(rest) = path_str.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(rest)
            } else {
                std::path::PathBuf::from(path_str)
            }
        } else {
            std::path::PathBuf::from(path_str)
        };

        if !path.is_dir() {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Not a directory: {}", path.display())})),
            )
                .into_response();
        }

        let mut engine = rag.lock().await;
        match engine.ingest_directory(&path, recursive, "web").await {
            Ok(ids) => Json(serde_json::json!({
                "indexed": ids.len(),
                "source_ids": ids,
            }))
            .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response(),
        }
    }

    /// POST /api/v1/knowledge/reveal -- reveal a sensitive chunk (optionally with TOTP)
    async fn reveal_knowledge_chunk(
        State(state): State<Arc<AppState>>,
        Json(req): Json<serde_json::Value>,
    ) -> impl IntoResponse {
        let Some(ref rag) = state.rag_engine else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "Knowledge base not initialized"})),
            )
                .into_response();
        };

        let Some(chunk_id) = req["chunk_id"].as_i64() else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Missing 'chunk_id'"})),
            )
                .into_response();
        };

        // VLT-2: If 2FA is enabled, verify TOTP code or session_id
        #[cfg(feature = "vault-2fa")]
        {
            use crate::security::{global_session_manager, TotpManager, TwoFactorStorage};

            if let Ok(storage) = TwoFactorStorage::new() {
                if let Ok(config) = storage.load() {
                    if config.enabled {
                        let session_id = req["session_id"].as_str().unwrap_or("");
                        let code = req["code"].as_str().unwrap_or("");

                        let authenticated = if !session_id.is_empty() {
                            // Verify via session
                            let sm = global_session_manager();
                            sm.verify_session(session_id).await
                        } else if !code.is_empty() {
                            // Verify TOTP code directly
                            match TotpManager::new(&config.totp_secret, &config.account) {
                                Ok(manager) => manager.verify(code),
                                Err(_) => {
                                    return (
                                        StatusCode::INTERNAL_SERVER_ERROR,
                                        Json(
                                            serde_json::json!({"error": "2FA configuration error"}),
                                        ),
                                    )
                                        .into_response();
                                }
                            }
                        } else {
                            false
                        };

                        if !authenticated {
                            return (
                                StatusCode::UNAUTHORIZED,
                                Json(serde_json::json!({
                                    "error": "2FA required. Provide 'code' or 'session_id'.",
                                    "requires_2fa": true
                                })),
                            )
                                .into_response();
                        }
                    }
                }
            }
        }

        let engine = rag.lock().await;
        match engine.reveal_chunk(chunk_id).await {
            Ok(Some(chunk)) => Json(serde_json::json!({
                "chunk_id": chunk.id,
                "content": chunk.content,
                "heading": chunk.heading,
            }))
            .into_response(),
            Ok(None) => (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Chunk not found"})),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
                .into_response(),
        }
    }
}

#[cfg(feature = "embeddings")]
pub(super) use inner::routes;
