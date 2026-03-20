use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::web::auth::{require_write, AuthUser};
use crate::web::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/v1/business",
            get(list_businesses_api).post(create_business_api),
        )
        .route("/v1/business/{id}", get(get_business_api))
        .route(
            "/v1/business/{id}/pause",
            axum::routing::post(pause_business_api),
        )
        .route(
            "/v1/business/{id}/resume",
            axum::routing::post(resume_business_api),
        )
        .route(
            "/v1/business/{id}/close",
            axum::routing::post(close_business_api),
        )
        .route(
            "/v1/business/{id}/strategies",
            get(list_business_strategies_api),
        )
        .route(
            "/v1/business/{id}/products",
            get(list_business_products_api),
        )
        .route(
            "/v1/business/{id}/transactions",
            get(list_business_transactions_api),
        )
        .route("/v1/business/{id}/revenue", get(get_business_revenue_api))
}

// --- Types ---

#[derive(Deserialize)]
struct BusinessListQuery {
    status: Option<String>,
}

#[derive(Deserialize)]
struct CreateBusinessRequest {
    name: String,
    description: Option<String>,
    autonomy: Option<String>,
    budget: Option<f64>,
    currency: Option<String>,
    ooda_interval: Option<String>,
    deliver_to: Option<String>,
}

// --- Handlers ---

/// GET /api/v1/business?status=active
async fn list_businesses_api(
    Query(q): Query<BusinessListQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let engine = state.business_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Business engine not available".into(),
    ))?;
    let businesses = engine
        .db()
        .list_businesses(q.status.as_deref())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total = businesses.len();
    let active = businesses
        .iter()
        .filter(|b| b.status == crate::business::BusinessStatus::Active)
        .count();
    let total_revenue: f64 = businesses.iter().map(|b| b.budget_spent).sum();

    Ok(Json(serde_json::json!({
        "businesses": businesses,
        "stats": {
            "total": total,
            "active": active,
            "total_budget_spent": total_revenue,
        }
    })))
}

/// POST /api/v1/business
async fn create_business_api(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Json(req): Json<CreateBusinessRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    require_write(&auth).map_err(|(s, j)| (s, j.0.to_string()))?;
    let engine = state.business_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Business engine not available".into(),
    ))?;

    let autonomy =
        crate::business::BusinessAutonomy::from_str(req.autonomy.as_deref().unwrap_or("semi"));
    let currency = req.currency.as_deref().unwrap_or("EUR");
    let ooda_interval = req.ooda_interval.as_deref().unwrap_or("every:86400");

    let biz = engine
        .launch(
            &req.name,
            req.description.as_deref(),
            autonomy,
            req.budget,
            currency,
            ooda_interval,
            req.deliver_to.as_deref(),
            None, // created_by
            None, // fiscal_config
        )
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "business": biz,
        "message": format!("Business '{}' launched", biz.name),
    })))
}

/// GET /api/v1/business/{id}
async fn get_business_api(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let engine = state.business_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Business engine not available".into(),
    ))?;
    let biz = engine
        .db()
        .load_business(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, format!("Business {id} not found")))?;

    let revenue = engine
        .get_revenue_summary(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "business": biz,
        "revenue": revenue,
    })))
}

/// POST /api/v1/business/{id}/pause
async fn pause_business_api(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let engine = state.business_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Business engine not available".into(),
    ))?;
    engine
        .pause(&id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(serde_json::json!({ "message": "Business paused" })))
}

/// POST /api/v1/business/{id}/resume
async fn resume_business_api(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let engine = state.business_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Business engine not available".into(),
    ))?;
    engine
        .resume(&id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(serde_json::json!({ "message": "Business resumed" })))
}

/// POST /api/v1/business/{id}/close
async fn close_business_api(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let engine = state.business_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Business engine not available".into(),
    ))?;
    engine
        .close(&id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(serde_json::json!({ "message": "Business closed" })))
}

/// GET /api/v1/business/{id}/strategies
async fn list_business_strategies_api(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let engine = state.business_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Business engine not available".into(),
    ))?;
    let strategies = engine
        .db()
        .list_strategies(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "strategies": strategies })))
}

/// GET /api/v1/business/{id}/products
async fn list_business_products_api(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let engine = state.business_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Business engine not available".into(),
    ))?;
    let products = engine
        .db()
        .list_products(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "products": products })))
}

/// GET /api/v1/business/{id}/transactions
async fn list_business_transactions_api(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let engine = state.business_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Business engine not available".into(),
    ))?;
    let transactions = engine
        .db()
        .list_transactions(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "transactions": transactions })))
}

/// GET /api/v1/business/{id}/revenue
async fn get_business_revenue_api(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let engine = state.business_engine.as_ref().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "Business engine not available".into(),
    ))?;
    let revenue = engine
        .get_revenue_summary(&id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "revenue": revenue })))
}
