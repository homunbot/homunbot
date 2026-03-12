use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::web::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/v1/usage", get(get_usage))
}

// --- Types ---

#[derive(Deserialize)]
struct UsageQuery {
    session: Option<String>,
    since: Option<String>,
    until: Option<String>,
}

#[derive(Serialize)]
struct UsageResponse {
    models: Vec<crate::storage::TokenUsageAggRow>,
    days: Vec<crate::storage::TokenUsageDailyRow>,
    totals: UsageTotals,
}

#[derive(Serialize)]
struct UsageTotals {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
    call_count: i64,
}

// --- Handlers ---

async fn get_usage(
    State(state): State<Arc<AppState>>,
    Query(q): Query<UsageQuery>,
) -> Result<Json<UsageResponse>, (StatusCode, String)> {
    let db = state.db.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Database not available".to_string(),
    ))?;

    let rows = db
        .query_token_usage(q.session.as_deref(), q.since.as_deref(), q.until.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query usage: {}", e),
            )
        })?;

    let days = db
        .query_token_usage_daily(q.session.as_deref(), q.since.as_deref(), q.until.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query daily usage: {}", e),
            )
        })?;

    let totals = UsageTotals {
        prompt_tokens: rows.iter().map(|r| r.prompt_tokens).sum(),
        completion_tokens: rows.iter().map(|r| r.completion_tokens).sum(),
        total_tokens: rows.iter().map(|r| r.total_tokens).sum(),
        call_count: rows.iter().map(|r| r.call_count).sum(),
    };

    Ok(Json(UsageResponse {
        models: rows,
        days,
        totals,
    }))
}
