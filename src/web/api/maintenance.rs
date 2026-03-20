use std::sync::Arc;

use axum::extract::State;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};

use super::super::auth::{require_admin, AuthUser};
use super::super::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/maintenance/db-stats", get(db_stats))
        .route("/v1/maintenance/purge", post(purge_tables))
}

// ─── Domain groups ──────────────────────────────────────────────

/// Each domain maps to one or more SQLite tables that get purged together.
const DOMAIN_GROUPS: &[(&str, &str, &[&str])] = &[
    (
        "conversations",
        "Chat sessions, messages, and memory",
        &[
            "sessions",
            "messages",
            "web_chat_runs",
            "memories",
            "memory_chunks",
        ],
    ),
    (
        "automations",
        "Automations and their run history",
        &["automations", "automation_runs"],
    ),
    (
        "workflows",
        "Workflows and their steps",
        &["workflows", "workflow_steps"],
    ),
    (
        "knowledge",
        "RAG knowledge base documents and chunks",
        &["rag_sources", "rag_chunks"],
    ),
    (
        "business",
        "Business strategies, products, transactions, and orders",
        &[
            "businesses",
            "business_strategies",
            "products",
            "transactions",
            "orders",
            "market_insights",
        ],
    ),
    (
        "usage",
        "Token usage statistics and skill audit log",
        &["token_usage", "skill_audit"],
    ),
    ("cron", "Scheduled cron jobs", &["cron_jobs"]),
    ("email", "Pending email queue", &["email_pending"]),
];

// ─── Stats ──────────────────────────────────────────────────────

#[derive(Serialize)]
struct DomainStats {
    id: String,
    label: String,
    tables: Vec<TableStats>,
    total_rows: i64,
}

#[derive(Serialize)]
struct TableStats {
    name: String,
    rows: i64,
}

async fn db_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<DomainStats>>, (axum::http::StatusCode, String)> {
    let db = state.db.as_ref().ok_or((
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;
    let pool = db.pool();
    let mut domains = Vec::new();

    for &(id, label, tables) in DOMAIN_GROUPS {
        let mut table_stats = Vec::new();
        let mut total: i64 = 0;

        for &table in tables {
            // Use format! for table name (safe — these are hardcoded constants, not user input)
            let query = format!("SELECT COUNT(*) as cnt FROM {table}");
            let count: i64 = sqlx::query_scalar(&query)
                .fetch_one(pool)
                .await
                .unwrap_or(0);
            total += count;
            table_stats.push(TableStats {
                name: table.to_string(),
                rows: count,
            });
        }

        domains.push(DomainStats {
            id: id.to_string(),
            label: label.to_string(),
            tables: table_stats,
            total_rows: total,
        });
    }

    Ok(Json(domains))
}

// ─── Purge ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct PurgeRequest {
    /// Domain ID to purge (e.g. "conversations", "automations")
    domain: String,
}

#[derive(Serialize)]
struct PurgeResponse {
    ok: bool,
    domain: String,
    deleted_rows: i64,
}

async fn purge_tables(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Json(req): Json<PurgeRequest>,
) -> Result<Json<PurgeResponse>, (axum::http::StatusCode, String)> {
    require_admin(&auth).map_err(|(s, j)| (s, j.0.to_string()))?;
    // Find the domain group
    let group = DOMAIN_GROUPS
        .iter()
        .find(|&&(id, _, _)| id == req.domain)
        .ok_or_else(|| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                format!("Unknown domain: {}", req.domain),
            )
        })?;

    let db = state.db.as_ref().ok_or((
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;
    let pool = db.pool();
    let mut total_deleted: i64 = 0;

    // Delete from each table in the group (order matters for foreign keys)
    // Delete child tables first (they're listed after parents in our groups)
    for &table in group.2.iter().rev() {
        let query = format!("DELETE FROM {table}");
        let result = sqlx::query(&query).execute(pool).await.map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to purge {table}: {e}"),
            )
        })?;
        total_deleted += result.rows_affected() as i64;
    }

    // Also clear related FTS indexes if applicable
    if req.domain == "conversations" {
        let _ = sqlx::query("DELETE FROM memory_fts").execute(pool).await;
    }
    if req.domain == "knowledge" {
        let _ = sqlx::query("DELETE FROM rag_fts").execute(pool).await;
    }

    tracing::info!(
        domain = %req.domain,
        deleted = total_deleted,
        "Database domain purged"
    );

    Ok(Json(PurgeResponse {
        ok: true,
        domain: req.domain,
        deleted_rows: total_deleted,
    }))
}
