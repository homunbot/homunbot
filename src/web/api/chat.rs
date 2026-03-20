use std::collections::HashSet;
use std::path::{Component, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{Json, Response};
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use super::super::auth::{check_write, AuthUser};
use super::super::server::AppState;
use crate::utils::reasoning_filter::strip_reasoning;
use crate::utils::text::truncate_str;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/v1/chat/conversations",
            get(list_chat_conversations).post(create_chat_conversation),
        )
        .route(
            "/v1/chat/conversations/{conversation_id}",
            axum::routing::patch(update_chat_conversation).delete(delete_chat_conversation),
        )
        .route(
            "/v1/chat/history",
            get(chat_history).delete(clear_chat_history),
        )
        .route(
            "/v1/chat/truncate",
            axum::routing::post(truncate_chat_history),
        )
        .route(
            "/v1/chat/uploads",
            axum::routing::post(upload_chat_attachment),
        )
        .route(
            "/v1/chat/uploads/{conversation_id}/{file_name}",
            get(get_chat_uploaded_file),
        )
        .route("/v1/chat/run", get(current_chat_run))
        .route("/v1/chat/compact", axum::routing::post(compact_chat))
        .route("/v1/chat/stop", axum::routing::post(stop_chat_run))
}

// Local copy of OkResponse for this module
#[derive(Serialize)]
struct OkResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

// ─── Types ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ChatHistoryQuery {
    limit: Option<u32>,
    conversation_id: Option<String>,
}

#[derive(Serialize)]
struct ChatHistoryMessage {
    id: i64,
    role: String,
    content: String,
    tools_used: Vec<String>,
    timestamp: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    attachments: Vec<super::super::chat_attachments::ChatAttachment>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    mcp_servers: Vec<super::super::chat_attachments::ChatMcpServerRef>,
}

#[derive(Deserialize)]
struct TruncateChatRequest {
    conversation_id: String,
    from_message_id: i64,
}

#[derive(Serialize)]
struct ChatConversationSummary {
    conversation_id: String,
    title: String,
    preview: String,
    created_at: String,
    updated_at: String,
    message_count: u32,
    archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_run: Option<super::super::run_state::WebChatRunSnapshot>,
}

#[derive(Deserialize)]
struct ChatConversationListQuery {
    limit: Option<u32>,
    q: Option<String>,
    include_archived: Option<bool>,
}

#[derive(Deserialize, Default)]
struct ChatConversationQuery {
    conversation_id: Option<String>,
}

#[derive(Deserialize, Serialize, Default)]
struct ChatConversationMetadata {
    title: Option<String>,
    archived: Option<bool>,
}

#[derive(Deserialize)]
struct UpdateChatConversationRequest {
    title: Option<String>,
    archived: Option<bool>,
}

#[derive(Serialize)]
struct ChatUploadResponse {
    ok: bool,
    attachment: super::super::chat_attachments::ChatAttachment,
}

struct ValidatedChatUpload {
    kind: String,
    content_type: String,
    max_bytes: usize,
}

// ─── Helpers ────────────────────────────────────────────────────

fn web_session_key(conversation_id: &str) -> String {
    format!("web:{conversation_id}")
}

fn chat_uploads_root() -> PathBuf {
    crate::config::Config::workspace_dir().join(".chat-uploads")
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ChatUploadCleanupStats {
    pub files_deleted: u64,
    pub directories_deleted: u64,
    pub bytes_deleted: u64,
}

async fn collect_upload_dir_stats(
    path: &std::path::Path,
) -> std::io::Result<ChatUploadCleanupStats> {
    let mut stats = ChatUploadCleanupStats::default();
    let mut stack = vec![path.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };

        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                stats.directories_deleted += 1;
                stack.push(entry.path());
            } else {
                stats.files_deleted += 1;
                if let Ok(metadata) = entry.metadata().await {
                    stats.bytes_deleted += metadata.len();
                }
            }
        }
    }

    Ok(stats)
}

async fn remove_chat_upload_dir(conversation_id: &str) -> std::io::Result<ChatUploadCleanupStats> {
    let conversation = sanitize_chat_segment(conversation_id, "default");
    let path = chat_uploads_root().join(conversation);
    let mut stats = collect_upload_dir_stats(&path).await?;
    match tokio::fs::remove_dir_all(&path).await {
        Ok(()) => {
            stats.directories_deleted += 1;
            Ok(stats)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ChatUploadCleanupStats::default()),
        Err(e) => Err(e),
    }
}

pub(crate) async fn cleanup_chat_upload_dirs(
    db: &crate::storage::Database,
    retention_days: u32,
) -> anyhow::Result<ChatUploadCleanupStats> {
    let root = chat_uploads_root();
    if !root.exists() {
        return Ok(ChatUploadCleanupStats::default());
    }

    let rows = db.list_sessions_by_prefix("web:%", 5000).await?;
    let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
    let cutoff_str = cutoff.format("%Y-%m-%d %H:%M:%S").to_string();
    let keep = rows
        .into_iter()
        .filter_map(|row| {
            row.key
                .strip_prefix("web:")
                .map(|conversation_id| (conversation_id.to_string(), row.updated_at))
        })
        .filter(|(_, updated_at)| updated_at >= &cutoff_str)
        .map(|(conversation_id, _)| sanitize_chat_segment(&conversation_id, "default"))
        .collect::<HashSet<_>>();

    let mut stats = ChatUploadCleanupStats::default();
    let mut entries = tokio::fs::read_dir(&root).await?;
    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        if !file_type.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();
        if keep.contains(&dir_name) {
            continue;
        }
        let dir_stats = remove_chat_upload_dir(&dir_name).await?;
        stats.files_deleted += dir_stats.files_deleted;
        stats.directories_deleted += dir_stats.directories_deleted;
        stats.bytes_deleted += dir_stats.bytes_deleted;
    }

    Ok(stats)
}

fn sanitize_chat_segment(value: &str, fallback: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized
    }
}

fn chat_upload_path(conversation_id: &str, file_name: &str) -> Option<PathBuf> {
    let conversation = sanitize_chat_segment(conversation_id, "default");
    let file_name = sanitize_chat_segment(file_name, "upload.bin");
    let path = chat_uploads_root().join(conversation).join(file_name);
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return None;
    }
    Some(path)
}

fn validate_chat_upload_kind(
    kind: &str,
    file_name: &str,
    content_type: Option<&str>,
) -> Option<ValidatedChatUpload> {
    let normalized_kind = kind.trim().to_lowercase();
    let extension = std::path::Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_lowercase())
        .unwrap_or_default();
    let guessed = content_type
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| {
            mime_guess::from_path(file_name)
                .first_or_octet_stream()
                .essence_str()
                .to_string()
        });

    match normalized_kind.as_str() {
        "image" if guessed.starts_with("image/") => Some(ValidatedChatUpload {
            kind: "image".to_string(),
            content_type: guessed,
            max_bytes: 15 * 1024 * 1024,
        }),
        "document" => {
            let allowed = matches!(extension.as_str(), "pdf" | "md" | "txt" | "doc" | "docx")
                || matches!(
                    guessed.as_str(),
                    "application/pdf"
                        | "text/markdown"
                        | "text/plain"
                        | "application/msword"
                        | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                );
            if allowed {
                Some(ValidatedChatUpload {
                    kind: "document".to_string(),
                    content_type: guessed,
                    max_bytes: 25 * 1024 * 1024,
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

fn chat_conversation_id(query: &ChatConversationQuery) -> String {
    query
        .conversation_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("default")
        .to_string()
}

fn chat_conversation_title(metadata: &str, first_user_message: Option<&str>) -> String {
    let metadata_title = parse_chat_conversation_metadata(metadata)
        .title
        .filter(|value| !value.trim().is_empty());

    metadata_title
        .or_else(|| {
            first_user_message
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(truncate_conversation_label)
        })
        .unwrap_or_else(|| "New conversation".to_string())
}

fn truncate_conversation_label(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return "New conversation".to_string();
    }
    truncate_str(&compact, 48, "\u{2026}")
}

fn parse_chat_conversation_metadata(metadata: &str) -> ChatConversationMetadata {
    serde_json::from_str(metadata).unwrap_or_default()
}

fn chat_message_label(raw: &str) -> String {
    let parsed = super::super::chat_attachments::parse_message_content(raw);
    let text = parsed.text.trim().to_string();
    if !text.is_empty() {
        return text;
    }
    if let Some(attachment) = parsed.attachments.first() {
        return attachment.name.clone();
    }
    parsed
        .mcp_servers
        .first()
        .map(|server| server.name.clone())
        .unwrap_or_default()
}

fn build_chat_conversation_summary(
    state: &Arc<AppState>,
    row: crate::storage::SessionListRow,
) -> Option<ChatConversationSummary> {
    let conversation_id = row.key.strip_prefix("web:")?.to_string();
    let session_key = web_session_key(&conversation_id);
    let metadata = parse_chat_conversation_metadata(&row.metadata);
    let first_user_message = row.first_user_message.as_deref().map(chat_message_label);
    let title = metadata
        .title
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| chat_conversation_title(&row.metadata, first_user_message.as_deref()));
    let preview = row
        .last_message_preview
        .as_deref()
        .map(chat_message_label)
        .map(|value| truncate_conversation_label(&value))
        .unwrap_or_default();
    let updated_at = row.last_message_at.unwrap_or(row.updated_at);
    Some(ChatConversationSummary {
        conversation_id,
        title,
        preview,
        created_at: row.created_at,
        updated_at,
        message_count: row.message_count.max(0) as u32,
        archived: metadata.archived.unwrap_or(false),
        active_run: state.web_runs.active_snapshot(&session_key),
    })
}

// ─── Handlers ───────────────────────────────────────────────────

async fn list_chat_conversations(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ChatConversationListQuery>,
) -> Result<Json<Vec<ChatConversationSummary>>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let rows = db
        .list_sessions_by_prefix("web:%", q.limit.unwrap_or(50))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let search = q.q.as_deref().map(|value| value.trim().to_lowercase());
    let include_archived = q.include_archived.unwrap_or(false);

    let conversations = rows
        .into_iter()
        .filter_map(|row| build_chat_conversation_summary(&state, row))
        .filter(|conversation| {
            if !include_archived && conversation.archived {
                return false;
            }
            if let Some(search) = search.as_deref() {
                let haystack = format!(
                    "{} {}",
                    conversation.title.to_lowercase(),
                    conversation.preview.to_lowercase()
                );
                haystack.contains(search)
            } else {
                true
            }
        })
        .collect();

    Ok(Json(conversations))
}

async fn create_chat_conversation(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
) -> Result<Json<ChatConversationSummary>, StatusCode> {
    check_write(&auth)?;
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let conversation_id = uuid::Uuid::new_v4().to_string();
    let session_key = web_session_key(&conversation_id);
    let metadata = serde_json::json!({});

    db.upsert_session(&session_key, 0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    db.set_session_metadata(&session_key, &metadata.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let session = db
        .load_session(&session_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ChatConversationSummary {
        conversation_id,
        title: String::new(),
        preview: String::new(),
        created_at: session.created_at,
        updated_at: session.updated_at,
        message_count: 0,
        archived: false,
        active_run: None,
    }))
}

async fn update_chat_conversation(
    State(state): State<Arc<AppState>>,
    Path(conversation_id): Path<String>,
    Json(req): Json<UpdateChatConversationRequest>,
) -> Result<Json<ChatConversationSummary>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let session_key = web_session_key(&conversation_id);
    let existing = db
        .load_session(&session_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut metadata = parse_chat_conversation_metadata(&existing.metadata);
    if let Some(title) = req.title {
        let title = title.trim();
        metadata.title = if title.is_empty() {
            None
        } else {
            Some(truncate_conversation_label(title))
        };
    }
    if let Some(archived) = req.archived {
        metadata.archived = Some(archived);
    }

    let metadata_json =
        serde_json::to_string(&metadata).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    db.set_session_metadata(&session_key, &metadata_json)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = db
        .list_sessions_by_prefix(&session_key, 1)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let row = rows.into_iter().next().ok_or(StatusCode::NOT_FOUND)?;
    let summary = build_chat_conversation_summary(&state, row).ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(summary))
}

async fn delete_chat_conversation(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(conversation_id): Path<String>,
) -> Result<Json<OkResponse>, StatusCode> {
    check_write(&auth)?;
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let session_key = web_session_key(&conversation_id);

    state.web_runs.clear_session(&session_key);
    let _ = db.delete_web_chat_runs(&session_key).await;
    let deleted = db
        .delete_session(&session_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deleted {
        if let Err(e) = remove_chat_upload_dir(&conversation_id).await {
            tracing::warn!(conversation_id = %conversation_id, error = %e, "Failed to remove chat upload directory");
        }
    }

    Ok(Json(OkResponse {
        ok: deleted,
        message: Some(if deleted {
            "Conversation deleted".to_string()
        } else {
            "Conversation not found".to_string()
        }),
    }))
}

async fn chat_history(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ChatHistoryQuery>,
) -> Result<Json<Vec<ChatHistoryMessage>>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let limit = q.limit.unwrap_or(50);
    let conversation_id = q
        .conversation_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("default");
    let session_key = web_session_key(conversation_id);

    let rows = db
        .load_messages(&session_key, limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let messages: Vec<ChatHistoryMessage> = rows
        .into_iter()
        .filter(|r| r.role == "user" || r.role == "assistant")
        .map(|r| {
            let tools: Vec<String> = serde_json::from_str(&r.tools_used).unwrap_or_default();
            let parsed = super::super::chat_attachments::parse_message_content(&r.content);
            // Strip reasoning/thinking blocks from stored assistant messages —
            // these are only meaningful during live streaming, not on history reload.
            let text = if r.role == "assistant" {
                strip_reasoning(&parsed.text)
            } else {
                parsed.text
            };
            ChatHistoryMessage {
                id: r.id,
                role: r.role,
                content: text,
                tools_used: tools,
                timestamp: r.timestamp,
                attachments: parsed.attachments,
                mcp_servers: parsed.mcp_servers,
            }
        })
        .collect();

    Ok(Json(messages))
}

async fn upload_chat_attachment(
    mut multipart: Multipart,
) -> Result<Json<ChatUploadResponse>, StatusCode> {
    let mut conversation_id = "default".to_string();
    let mut kind = "image".to_string();
    let mut file_name = None;
    let mut content_type = None;
    let mut bytes = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        match field.name() {
            Some("conversation_id") => {
                conversation_id = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            }
            Some("kind") => {
                kind = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            }
            Some("file") => {
                file_name = Some(field.file_name().unwrap_or("upload.bin").to_string());
                content_type = field.content_type().map(ToString::to_string);
                bytes = Some(field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?);
            }
            _ => {}
        }
    }

    let file_name = sanitize_chat_segment(file_name.as_deref().unwrap_or("upload.bin"), "image");
    let bytes = bytes.ok_or(StatusCode::BAD_REQUEST)?;
    let validated = validate_chat_upload_kind(&kind, &file_name, content_type.as_deref())
        .ok_or(StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
    if bytes.is_empty() || bytes.len() > validated.max_bytes {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }

    let conversation_id = sanitize_chat_segment(&conversation_id, "default");
    let extension = std::path::Path::new(&file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| sanitize_chat_segment(value, "bin"))
        .filter(|value| !value.is_empty());
    let stored_name = match extension {
        Some(ext) => format!("{}.{}", uuid::Uuid::new_v4(), ext),
        None => uuid::Uuid::new_v4().to_string(),
    };
    let path = chat_upload_path(&conversation_id, &stored_name).ok_or(StatusCode::BAD_REQUEST)?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    tokio::fs::write(&path, &bytes)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ChatUploadResponse {
        ok: true,
        attachment: super::super::chat_attachments::ChatAttachment {
            kind: validated.kind,
            name: file_name,
            stored_path: path.to_string_lossy().to_string(),
            preview_url: format!("/api/v1/chat/uploads/{conversation_id}/{stored_name}"),
            content_type: validated.content_type,
            size_bytes: bytes.len() as u64,
        },
    }))
}

async fn get_chat_uploaded_file(
    Path((conversation_id, file_name)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let path = chat_upload_path(&conversation_id, &file_name).ok_or(StatusCode::BAD_REQUEST)?;
    let data = tokio::fs::read(&path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let content_type = mime_guess::from_path(&path).first_or_octet_stream();

    let mut response = Response::new(Body::from(data));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(content_type.as_ref())
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );
    Ok(response)
}

/// Clear chat history for the web session
async fn clear_chat_history(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Query(q): Query<ChatConversationQuery>,
) -> Result<Json<OkResponse>, StatusCode> {
    check_write(&auth)?;
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let conversation_id = chat_conversation_id(&q);
    let session_key = web_session_key(&conversation_id);

    db.clear_messages(&session_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = db.delete_web_chat_runs(&session_key).await;

    state.web_runs.clear_session(&session_key);

    Ok(Json(OkResponse {
        ok: true,
        message: Some("Chat history cleared".to_string()),
    }))
}

/// Truncate chat history from a specific message ID (for edit/resend).
async fn truncate_chat_history(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TruncateChatRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let conversation_id = if req.conversation_id.trim().is_empty() {
        "default".to_string()
    } else {
        req.conversation_id
    };
    let session_key = web_session_key(&conversation_id);

    let deleted = db
        .delete_messages_from(&session_key, req.from_message_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(OkResponse {
        ok: true,
        message: Some(format!("Deleted {deleted} messages")),
    }))
}

async fn current_chat_run(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ChatConversationQuery>,
) -> Result<Json<Option<super::super::run_state::WebChatRunSnapshot>>, StatusCode> {
    let conversation_id = chat_conversation_id(&q);
    let session_key = web_session_key(&conversation_id);

    if let Some(run) = state.web_runs.active_snapshot(&session_key) {
        return Ok(Json(Some(run)));
    }

    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let run = db
        .load_restorable_web_chat_run(&session_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(run))
}

/// Compact chat conversation (trigger memory consolidation)
async fn compact_chat(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ChatConversationQuery>,
) -> Result<Json<OkResponse>, StatusCode> {
    let db = state.db.as_ref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let conversation_id = chat_conversation_id(&q);
    let session_key = web_session_key(&conversation_id);

    // Check if there are enough messages to consolidate
    let count = db
        .count_messages(&session_key)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if count < 10 {
        return Ok(Json(OkResponse {
            ok: false,
            message: Some("Not enough messages to compact (need at least 10)".to_string()),
        }));
    }

    // Trigger consolidation by resetting the last_consolidated counter
    // The agent loop will handle the actual consolidation on next message
    db.upsert_session(&session_key, 0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(OkResponse {
        ok: true,
        message: Some("Conversation will be compacted on next message".to_string()),
    }))
}

/// Request cancellation of the current web chat run.
async fn stop_chat_run(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ChatConversationQuery>,
) -> Result<Json<OkResponse>, StatusCode> {
    let conversation_id = chat_conversation_id(&q);
    let session_key = web_session_key(&conversation_id);
    let active = state.web_runs.request_stop(&session_key);
    if let Some(run) = active.as_ref() {
        if let Some(db) = state.db.as_ref() {
            if let Err(error) = db.upsert_web_chat_run(run).await {
                tracing::error!(run_id = %run.run_id, %error, "Failed to persist stopping web chat run");
            }
        }
    }
    crate::agent::stop::request_stop();
    Ok(Json(OkResponse {
        ok: true,
        message: Some(if active.is_some() {
            "Stop requested".to_string()
        } else {
            "No active chat run".to_string()
        }),
    }))
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod chat_upload_tests {
    use super::validate_chat_upload_kind;

    #[test]
    fn accepts_supported_image_uploads() {
        let upload = validate_chat_upload_kind("image", "photo.png", Some("image/png"))
            .expect("expected image upload to validate");
        assert_eq!(upload.kind, "image");
        assert_eq!(upload.content_type, "image/png");
    }

    #[test]
    fn accepts_supported_document_uploads() {
        let upload = validate_chat_upload_kind("document", "report.pdf", Some("application/pdf"))
            .expect("expected document upload to validate");
        assert_eq!(upload.kind, "document");
        assert_eq!(upload.content_type, "application/pdf");
    }

    #[test]
    fn rejects_unsupported_document_uploads() {
        assert!(
            validate_chat_upload_kind("document", "archive.zip", Some("application/zip")).is_none()
        );
    }
}
