use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use super::super::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/memory/stats", get(memory_stats))
        .route(
            "/v1/memory/content",
            get(get_memory_file).put(put_memory_file),
        )
        .route("/v1/memory/search", get(search_memory))
        .route("/v1/memory/history", get(get_memory_history))
        .route(
            "/v1/memory/instructions",
            get(get_instructions).put(put_instructions),
        )
        .route("/v1/memory/daily", get(list_daily_files))
        .route("/v1/memory/daily/{date}", get(get_daily_file))
        .route(
            "/v1/memory/cleanup",
            axum::routing::post(run_memory_cleanup),
        )
}

// Local copy of OkResponse for this module
#[derive(Serialize)]
struct OkResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Serialize)]
struct MemoryStatsResponse {
    chunk_count: i64,
    daily_count: usize,
    has_memory_md: bool,
    has_history_md: bool,
    has_instructions_md: bool,
}

async fn memory_stats(State(state): State<Arc<AppState>>) -> Json<MemoryStatsResponse> {
    let data_dir = crate::config::Config::data_dir();

    let chunk_count = match state.db.as_ref() {
        Some(db) => db.count_memory_chunks().await.unwrap_or(0),
        None => 0,
    };

    let daily_count = std::fs::read_dir(data_dir.join("memory"))
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
                .count()
        })
        .unwrap_or(0);

    Json(MemoryStatsResponse {
        chunk_count,
        daily_count,
        has_memory_md: data_dir.join("MEMORY.md").exists(),
        has_history_md: data_dir.join("HISTORY.md").exists(),
        has_instructions_md: data_dir.join("brain").join("INSTRUCTIONS.md").exists()
            || data_dir.join("INSTRUCTIONS.md").exists(),
    })
}

/// Run memory cleanup based on retention policies.
/// POST /api/v1/memory/cleanup
#[derive(Deserialize)]
struct MemoryCleanupRequest {
    /// Override conversation retention days (optional)
    conversation_retention_days: Option<u32>,
    /// Override history retention days (optional)
    history_retention_days: Option<u32>,
}

#[derive(Serialize)]
struct MemoryCleanupResponse {
    ok: bool,
    messages_deleted: u64,
    chunks_deleted: u64,
    uploads_deleted: u64,
    upload_dirs_deleted: u64,
    message: String,
}

async fn run_memory_cleanup(
    State(state): State<Arc<AppState>>,
    Json(req): Json<MemoryCleanupRequest>,
) -> Result<Json<MemoryCleanupResponse>, StatusCode> {
    let db = match state.db.as_ref() {
        Some(db) => db,
        None => return Err(StatusCode::SERVICE_UNAVAILABLE),
    };

    // Use request overrides or config defaults
    let config = state.config.read().await;
    let mem_config = &config.memory;
    let conv_days = req
        .conversation_retention_days
        .unwrap_or(mem_config.conversation_retention_days);
    let hist_days = req
        .history_retention_days
        .unwrap_or(mem_config.history_retention_days);
    drop(config); // Release lock before DB operation

    match db.run_memory_cleanup(conv_days, hist_days).await {
        Ok(result) => {
            let upload_cleanup = super::cleanup_chat_upload_dirs(db, conv_days)
                .await
                .unwrap_or_default();
            Ok(Json(MemoryCleanupResponse {
                ok: true,
                messages_deleted: result.messages_deleted,
                chunks_deleted: result.chunks_deleted,
                uploads_deleted: upload_cleanup.files_deleted,
                upload_dirs_deleted: upload_cleanup.directories_deleted,
                message: format!(
                    "Cleaned up {} old messages, {} old history chunks, and {} uploaded chat files",
                    result.messages_deleted, result.chunks_deleted, upload_cleanup.files_deleted
                ),
            }))
        }
        Err(e) => {
            tracing::error!(error = %e, "Memory cleanup failed");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Deserialize)]
struct MemoryFileQuery {
    file: String,
}

#[derive(Serialize)]
struct MemoryFileResponse {
    ok: bool,
    content: String,
}

async fn get_memory_file(
    Query(q): Query<MemoryFileQuery>,
) -> Result<Json<MemoryFileResponse>, StatusCode> {
    let data_dir = crate::config::Config::data_dir();
    let brain_dir = data_dir.join("brain");
    let path = match q.file.as_str() {
        "memory" => data_dir.join("MEMORY.md"),
        "history" => data_dir.join("HISTORY.md"),
        "instructions" => {
            // Prefer brain/ location, fall back to legacy data_dir
            let new_path = brain_dir.join("INSTRUCTIONS.md");
            if new_path.exists() {
                new_path
            } else {
                data_dir.join("INSTRUCTIONS.md")
            }
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    Ok(Json(MemoryFileResponse { ok: true, content }))
}

#[derive(Deserialize)]
struct PutMemoryFileRequest {
    file: String,
    content: String,
}

async fn put_memory_file(
    Json(req): Json<PutMemoryFileRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let data_dir = crate::config::Config::data_dir();
    let brain_dir = data_dir.join("brain");
    let path = match req.file.as_str() {
        "memory" => data_dir.join("MEMORY.md"),
        "instructions" => brain_dir.join("INSTRUCTIONS.md"),
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    tokio::fs::write(&path, &req.content)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(OkResponse {
        ok: true,
        message: None,
    }))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
}

fn default_search_limit() -> usize {
    20
}

#[derive(Serialize)]
struct SearchResponse {
    chunks: Vec<ChunkView>,
}

#[derive(Serialize)]
struct ChunkView {
    id: i64,
    date: String,
    source: String,
    heading: String,
    content: String,
    memory_type: String,
    created_at: String,
    /// Relevance score from hybrid search (0.0-1.0). None for FTS5-only results.
    #[serde(skip_serializing_if = "Option::is_none")]
    score: Option<f64>,
}

impl From<crate::storage::MemoryChunkRow> for ChunkView {
    fn from(row: crate::storage::MemoryChunkRow) -> Self {
        Self {
            id: row.id,
            date: row.date,
            source: row.source,
            heading: row.heading,
            content: row.content,
            memory_type: row.memory_type,
            created_at: row.created_at,
            score: None,
        }
    }
}

async fn search_memory(
    State(state): State<Arc<AppState>>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    if q.q.trim().is_empty() {
        return Ok(Json(SearchResponse { chunks: vec![] }));
    }

    // Try hybrid search (vector + FTS5) if memory searcher is available
    #[cfg(feature = "embeddings")]
    if let Some(ref searcher_mutex) = state.memory_searcher {
        let mut searcher = searcher_mutex.lock().await;
        match searcher.search(&q.q, q.limit).await {
            Ok(results) => {
                let chunks: Vec<ChunkView> = results
                    .into_iter()
                    .map(|r| ChunkView {
                        id: r.chunk.id,
                        date: r.chunk.date,
                        source: r.chunk.source,
                        heading: r.chunk.heading,
                        content: r.chunk.content,
                        memory_type: r.chunk.memory_type,
                        created_at: r.chunk.created_at,
                        score: Some(r.score),
                    })
                    .collect();
                return Ok(Json(SearchResponse { chunks }));
            }
            Err(e) => {
                tracing::warn!(error = %e, "Hybrid search failed, falling back to FTS5-only");
                // Fall through to FTS5-only search below
            }
        }
    }

    // Fallback: FTS5-only search (no vector similarity)
    let fts_results = db
        .fts5_search(&q.q, q.limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if fts_results.is_empty() {
        return Ok(Json(SearchResponse { chunks: vec![] }));
    }

    let ids: Vec<i64> = fts_results.iter().map(|&(id, _)| id).collect();
    let rows = db
        .load_chunks_by_ids(&ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Preserve FTS5 ranking order
    let mut id_order: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for (i, &(id, _)) in fts_results.iter().enumerate() {
        id_order.insert(id, i);
    }
    let mut chunks: Vec<ChunkView> = rows.into_iter().map(ChunkView::from).collect();
    chunks.sort_by_key(|c| id_order.get(&c.id).copied().unwrap_or(usize::MAX));

    Ok(Json(SearchResponse { chunks }))
}

#[derive(Deserialize)]
struct HistoryQuery {
    #[serde(default = "default_history_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_history_limit() -> i64 {
    20
}

async fn get_memory_history(
    State(state): State<Arc<AppState>>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<SearchResponse>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;

    let rows: Vec<crate::storage::MemoryChunkRow> = sqlx::query_as(
        "SELECT id, date, source, heading, content, memory_type, created_at \
         FROM memory_chunks WHERE memory_type = 'history' \
         ORDER BY created_at DESC LIMIT ? OFFSET ?",
    )
    .bind(q.limit)
    .bind(q.offset)
    .fetch_all(db.pool())
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let chunks = rows.into_iter().map(ChunkView::from).collect();
    Ok(Json(SearchResponse { chunks }))
}

#[derive(Serialize)]
struct InstructionsResponse {
    ok: bool,
    instructions: Vec<String>,
}

async fn get_instructions() -> Json<InstructionsResponse> {
    let data_dir = crate::config::Config::data_dir();
    let brain_path = data_dir.join("brain").join("INSTRUCTIONS.md");
    let legacy_path = data_dir.join("INSTRUCTIONS.md");
    let path = if brain_path.exists() {
        brain_path
    } else {
        legacy_path
    };

    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let instructions: Vec<String> = content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .map(|s| s.to_string())
        })
        .filter(|s| !s.is_empty())
        .collect();

    Json(InstructionsResponse {
        ok: true,
        instructions,
    })
}

#[derive(Deserialize)]
struct PutInstructionsRequest {
    instructions: Vec<String>,
}

async fn put_instructions(
    Json(req): Json<PutInstructionsRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let data_dir = crate::config::Config::data_dir();
    let brain_dir = data_dir.join("brain");
    let path = brain_dir.join("INSTRUCTIONS.md");

    tokio::fs::create_dir_all(&brain_dir)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let content = req
        .instructions
        .iter()
        .map(|i| format!("- {i}"))
        .collect::<Vec<_>>()
        .join("\n");

    tokio::fs::write(
        &path,
        if content.is_empty() {
            String::new()
        } else {
            format!("{content}\n")
        },
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(OkResponse {
        ok: true,
        message: None,
    }))
}

#[derive(Serialize)]
struct DailyListResponse {
    dates: Vec<String>,
}

async fn list_daily_files() -> Json<DailyListResponse> {
    let memory_dir = crate::config::Config::data_dir().join("memory");

    let mut dates: Vec<String> = std::fs::read_dir(&memory_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    name.strip_suffix(".md").map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    dates.sort_unstable_by(|a, b| b.cmp(a)); // newest first
    Json(DailyListResponse { dates })
}

#[derive(Serialize)]
struct DailyFileResponse {
    ok: bool,
    date: String,
    content: String,
}

async fn get_daily_file(Path(date): Path<String>) -> Result<Json<DailyFileResponse>, StatusCode> {
    // Validate date format to prevent path traversal
    if !date.chars().all(|c| c.is_ascii_digit() || c == '-') || date.len() != 10 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let path = crate::config::Config::data_dir()
        .join("memory")
        .join(format!("{date}.md"));

    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    Ok(Json(DailyFileResponse {
        ok: true,
        date,
        content,
    }))
}
