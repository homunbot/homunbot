//! REST API endpoints for the Contact Book (CTB-1).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{delete, get, post, put};
use axum::Router;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::super::server::AppState;
use crate::contacts::db::ContactUpdate;
use crate::storage::Database;
use crate::web::auth::{require_write, AuthUser};

type ApiErr = (StatusCode, Json<Value>);

fn require_db(state: &AppState) -> Result<&Database, ApiErr> {
    state.db.as_ref().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Database not available"})),
        )
    })
}

fn internal(e: anyhow::Error) -> ApiErr {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": e.to_string()})),
    )
}

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/contacts", get(list_contacts).post(create_contact))
        .route(
            "/v1/contacts/{id}",
            get(get_contact).put(update_contact).delete(delete_contact),
        )
        .route(
            "/v1/contacts/{id}/identities",
            get(list_identities).post(add_identity),
        )
        .route("/v1/contacts/identities/{id}", delete(remove_identity))
        .route(
            "/v1/contacts/{id}/relationships",
            get(list_relationships).post(add_relationship),
        )
        .route(
            "/v1/contacts/relationships/{id}",
            delete(remove_relationship),
        )
        .route("/v1/contacts/{id}/events", get(list_events).post(add_event))
        .route("/v1/contacts/events/{id}", delete(remove_event))
        .route("/v1/contacts/upcoming", get(upcoming_events))
        .route("/v1/contacts/pending", get(list_pending))
        .route("/v1/contacts/pending/{id}/approve", post(approve_pending))
        .route("/v1/contacts/pending/{id}/reject", post(reject_pending))
}

// ── Request / Response types ────────────────────────────────────────

#[derive(Deserialize)]
struct ListQuery {
    q: Option<String>,
}

#[derive(Deserialize)]
struct CreateContactRequest {
    name: String,
    nickname: Option<String>,
    bio: Option<String>,
    notes: Option<String>,
    birthday: Option<String>,
    nameday: Option<String>,
    preferred_channel: Option<String>,
    response_mode: Option<String>,
    tags: Option<String>,
    tone_of_voice: Option<String>,
    persona_override: Option<String>,
    persona_instructions: Option<String>,
}

#[derive(Serialize)]
struct ContactResponse {
    #[serde(flatten)]
    contact: crate::contacts::Contact,
    #[serde(skip_serializing_if = "Option::is_none")]
    identities: Option<Vec<crate::contacts::ContactIdentity>>,
}

#[derive(Deserialize)]
struct AddIdentityRequest {
    channel: String,
    identifier: String,
    label: Option<String>,
}

#[derive(Deserialize)]
struct AddRelationshipRequest {
    to_contact_id: i64,
    relationship_type: String,
    #[serde(default)]
    bidirectional: bool,
    reverse_type: Option<String>,
    notes: Option<String>,
}

#[derive(Deserialize)]
struct AddEventRequest {
    event_type: String,
    date: String,
    recurrence: Option<String>,
    label: Option<String>,
    #[serde(default)]
    auto_greet: bool,
    notify_days_before: Option<i32>,
}

// ── Contacts ────────────────────────────────────────────────────────

async fn list_contacts(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListQuery>,
) -> Result<Json<Vec<crate::contacts::Contact>>, ApiErr> {
    let db = require_db(&state)?;
    let contacts = db
        .list_contacts(params.q.as_deref())
        .await
        .map_err(internal)?;
    Ok(Json(contacts))
}

async fn create_contact(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Json(body): Json<CreateContactRequest>,
) -> Result<Json<ContactResponse>, ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;
    let id = db
        .insert_contact(
            &body.name,
            body.nickname.as_deref(),
            body.bio.as_deref(),
            body.notes.as_deref(),
            body.birthday.as_deref(),
            body.nameday.as_deref(),
            body.preferred_channel.as_deref(),
            body.response_mode.as_deref(),
            body.tags.as_deref(),
            body.tone_of_voice.as_deref(),
        )
        .await
        .map_err(internal)?;

    let contact = db
        .load_contact(id)
        .await
        .map_err(internal)?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Contact created but not found"})),
            )
        })?;

    Ok(Json(ContactResponse {
        contact,
        identities: None,
    }))
}

async fn get_contact(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<ContactResponse>, ApiErr> {
    let db = require_db(&state)?;
    let contact = db
        .load_contact(id)
        .await
        .map_err(internal)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Contact not found"})),
            )
        })?;
    let identities = db.list_contact_identities(id).await.map_err(internal)?;

    Ok(Json(ContactResponse {
        contact,
        identities: Some(identities),
    }))
}

async fn update_contact(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(id): Path<i64>,
    Json(body): Json<ContactUpdate>,
) -> Result<Json<crate::contacts::Contact>, ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;
    let updated = db.update_contact(id, &body).await.map_err(internal)?;
    if !updated {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Contact not found or no fields to update"})),
        ));
    }
    let contact = db
        .load_contact(id)
        .await
        .map_err(internal)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Contact not found"})),
            )
        })?;
    Ok(Json(contact))
}

async fn delete_contact(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;
    let deleted = db.delete_contact(id).await.map_err(internal)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Contact not found"})),
        ))
    }
}

// ── Identities ──────────────────────────────────────────────────────

async fn list_identities(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<crate::contacts::ContactIdentity>>, ApiErr> {
    let db = require_db(&state)?;
    let ids = db.list_contact_identities(id).await.map_err(internal)?;
    Ok(Json(ids))
}

async fn add_identity(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(contact_id): Path<i64>,
    Json(body): Json<AddIdentityRequest>,
) -> Result<Json<Value>, ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;
    let id = db
        .insert_contact_identity(
            contact_id,
            &body.channel,
            &body.identifier,
            body.label.as_deref(),
        )
        .await
        .map_err(internal)?;
    Ok(Json(json!({"id": id})))
}

async fn remove_identity(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;
    let deleted = db.delete_contact_identity(id).await.map_err(internal)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Identity not found"})),
        ))
    }
}

// ── Relationships ───────────────────────────────────────────────────

async fn list_relationships(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<crate::contacts::ContactRelationship>>, ApiErr> {
    let db = require_db(&state)?;
    let rels = db.list_contact_relationships(id).await.map_err(internal)?;
    Ok(Json(rels))
}

async fn add_relationship(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(from_id): Path<i64>,
    Json(body): Json<AddRelationshipRequest>,
) -> Result<Json<Value>, ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;
    let id = db
        .insert_contact_relationship(
            from_id,
            body.to_contact_id,
            &body.relationship_type,
            body.bidirectional,
            body.reverse_type.as_deref(),
            body.notes.as_deref(),
        )
        .await
        .map_err(internal)?;
    Ok(Json(json!({"id": id})))
}

async fn remove_relationship(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;
    let deleted = db.delete_contact_relationship(id).await.map_err(internal)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Relationship not found"})),
        ))
    }
}

// ── Events ──────────────────────────────────────────────────────────

async fn list_events(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<crate::contacts::ContactEvent>>, ApiErr> {
    let db = require_db(&state)?;
    let events = db.list_contact_events(id).await.map_err(internal)?;
    Ok(Json(events))
}

async fn add_event(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(contact_id): Path<i64>,
    Json(body): Json<AddEventRequest>,
) -> Result<Json<Value>, ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;
    let id = db
        .insert_contact_event(
            contact_id,
            &body.event_type,
            &body.date,
            body.recurrence.as_deref(),
            body.label.as_deref(),
            body.auto_greet,
            body.notify_days_before,
        )
        .await
        .map_err(internal)?;
    Ok(Json(json!({"id": id})))
}

async fn remove_event(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;
    let deleted = db.delete_contact_event(id).await.map_err(internal)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Event not found"})),
        ))
    }
}

// ── Upcoming events ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct UpcomingQuery {
    #[serde(default = "default_days")]
    days: i32,
}

fn default_days() -> i32 {
    7
}

async fn upcoming_events(
    State(state): State<Arc<AppState>>,
    Query(params): Query<UpcomingQuery>,
) -> Result<Json<Vec<crate::contacts::UpcomingEvent>>, ApiErr> {
    let db = require_db(&state)?;
    let events = db
        .load_upcoming_contact_events(params.days)
        .await
        .map_err(internal)?;
    Ok(Json(events))
}

// ── Pending responses ───────────────────────────────────────────────

async fn list_pending(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<crate::contacts::PendingResponse>>, ApiErr> {
    let db = require_db(&state)?;
    let pending = db
        .list_pending_responses(Some("pending"))
        .await
        .map_err(internal)?;
    Ok(Json(pending))
}

async fn approve_pending(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;
    let updated = db
        .update_pending_response_status(id, "approved")
        .await
        .map_err(internal)?;
    if updated {
        Ok(Json(json!({"ok": true, "message": "Response approved"})))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Pending response not found"})),
        ))
    }
}

async fn reject_pending(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;
    let updated = db
        .update_pending_response_status(id, "rejected")
        .await
        .map_err(internal)?;
    if updated {
        Ok(Json(json!({"ok": true, "message": "Response rejected"})))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "Pending response not found"})),
        ))
    }
}
