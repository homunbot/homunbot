use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::Query;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use super::super::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/logs/stream", get(stream_logs))
        .route("/v1/logs/recent", get(recent_logs))
}

/// GET /api/v1/logs/stream
async fn stream_logs() -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let rx = crate::logs::subscribe();

    let stream = futures::stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Ok(record) => {
                let payload = serde_json::to_string(&record).unwrap_or_else(|_| "{}".to_string());
                let event = Event::default().event("log").data(payload);
                Some((Ok(event), rx))
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                let lag_record = crate::logs::LogRecord {
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    level: "warn".to_string(),
                    target: "homun.logs".to_string(),
                    message: format!(
                        "Log stream dropped {skipped} events because the client was too slow"
                    ),
                    module_path: None,
                    file: None,
                    line: None,
                    fields: Vec::new(),
                };
                let payload =
                    serde_json::to_string(&lag_record).unwrap_or_else(|_| "{}".to_string());
                let event = Event::default().event("log").data(payload);
                Some((Ok(event), rx))
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => None,
        }
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    )
}

#[derive(Deserialize)]
struct RecentLogsQuery {
    limit: Option<usize>,
}

/// GET /api/v1/logs/recent
async fn recent_logs(Query(query): Query<RecentLogsQuery>) -> Json<Vec<crate::logs::LogRecord>> {
    let limit = query.limit.unwrap_or(250).clamp(1, 1000);
    Json(crate::logs::recent(limit))
}
