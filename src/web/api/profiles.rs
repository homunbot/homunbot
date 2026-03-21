//! REST API endpoints for the Profile System.
//!
//! CRUD for profiles + SOUL.md read/write per profile.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post, put};
use axum::Router;
use serde::Deserialize;
use serde_json::{json, Value};

use super::super::server::AppState;
use crate::config::Config;
use crate::profiles;
use crate::provider::one_shot::{llm_one_shot, OneShotRequest};
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

fn not_found(msg: &str) -> ApiErr {
    (StatusCode::NOT_FOUND, Json(json!({"error": msg})))
}

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/profiles", get(list_profiles).post(create_profile))
        .route(
            "/v1/profiles/{id}",
            get(get_profile).put(update_profile).delete(delete_profile),
        )
        .route(
            "/v1/profiles/{id}/soul",
            get(read_soul).put(write_soul),
        )
        .route(
            "/v1/profiles/{id}/instructions",
            get(read_instructions),
        )
        .route(
            "/v1/profiles/{id}/generate",
            post(generate_profile_json),
        )
}

// ── Request types ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateProfileRequest {
    slug: String,
    display_name: String,
    avatar_emoji: Option<String>,
    profile_json: Option<String>,
}

#[derive(Deserialize)]
struct UpdateProfileRequest {
    display_name: Option<String>,
    avatar_emoji: Option<String>,
    profile_json: Option<String>,
}

#[derive(Deserialize)]
struct SoulBody {
    content: String,
}

#[derive(Deserialize)]
struct GenerateRequest {
    description: String,
}

// ── Handlers ────────────────────────────────────────────────────────

/// List all profiles (default first).
async fn list_profiles(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<profiles::Profile>>, ApiErr> {
    let db = require_db(&state)?;
    let list = profiles::db::load_all_profiles(db.pool())
        .await
        .map_err(internal)?;
    Ok(Json(list))
}

/// Create a new profile.
async fn create_profile(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Json(body): Json<CreateProfileRequest>,
) -> Result<(StatusCode, Json<profiles::Profile>), ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;

    let emoji = body.avatar_emoji.as_deref().unwrap_or("👤");
    let pj = body.profile_json.as_deref().unwrap_or("{}");

    let id = profiles::db::insert_profile(db.pool(), &body.slug, &body.display_name, emoji, pj)
        .await
        .map_err(internal)?;

    // Create brain directory for the new profile
    let data_dir = Config::data_dir();
    let dir = data_dir
        .join("brain")
        .join("profiles")
        .join(&body.slug);
    std::fs::create_dir_all(&dir).ok();

    let profile = profiles::db::load_profile_by_id(db.pool(), id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("Profile created but not found"))?;

    Ok((StatusCode::CREATED, Json(profile)))
}

/// Get a single profile by ID.
async fn get_profile(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<profiles::Profile>, ApiErr> {
    let db = require_db(&state)?;
    let profile = profiles::db::load_profile_by_id(db.pool(), id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("Profile not found"))?;
    Ok(Json(profile))
}

/// Update a profile's mutable fields.
async fn update_profile(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<profiles::Profile>, ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;

    let existing = profiles::db::load_profile_by_id(db.pool(), id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("Profile not found"))?;

    let display_name = body.display_name.as_deref().unwrap_or(&existing.display_name);
    let emoji = body.avatar_emoji.as_deref().unwrap_or(&existing.avatar_emoji);
    let pj = body.profile_json.as_deref().unwrap_or(&existing.profile_json);

    profiles::db::update_profile(db.pool(), id, display_name, emoji, pj)
        .await
        .map_err(internal)?;

    let updated = profiles::db::load_profile_by_id(db.pool(), id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("Profile not found after update"))?;

    Ok(Json(updated))
}

/// Delete a profile (refuses default).
async fn delete_profile(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;

    profiles::db::delete_profile(db.pool(), id)
        .await
        .map_err(|e| {
            if e.to_string().contains("default") {
                (
                    StatusCode::FORBIDDEN,
                    Json(json!({"error": "Cannot delete the default profile"})),
                )
            } else {
                internal(e)
            }
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Read SOUL.md from a profile's brain directory.
async fn read_soul(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiErr> {
    let db = require_db(&state)?;
    let profile = profiles::db::load_profile_by_id(db.pool(), id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("Profile not found"))?;

    let data_dir = Config::data_dir();
    let soul_path = profile.brain_dir(&data_dir).join("SOUL.md");

    let content = if soul_path.exists() {
        std::fs::read_to_string(&soul_path).unwrap_or_default()
    } else {
        String::new()
    };

    Ok(Json(json!({"content": content})))
}

/// Write SOUL.md to a profile's brain directory.
async fn write_soul(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(id): Path<i64>,
    Json(body): Json<SoulBody>,
) -> Result<Json<Value>, ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;
    let profile = profiles::db::load_profile_by_id(db.pool(), id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("Profile not found"))?;

    let data_dir = Config::data_dir();
    let brain_dir = profile.brain_dir(&data_dir);
    std::fs::create_dir_all(&brain_dir).map_err(|e| internal(e.into()))?;

    let soul_path = brain_dir.join("SOUL.md");
    std::fs::write(&soul_path, &body.content).map_err(|e| internal(e.into()))?;

    Ok(Json(json!({"ok": true})))
}

/// Read INSTRUCTIONS.md from a profile's brain directory.
async fn read_instructions(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, ApiErr> {
    let db = require_db(&state)?;
    let profile = profiles::db::load_profile_by_id(db.pool(), id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("Profile not found"))?;

    let data_dir = Config::data_dir();
    let path = profile.brain_dir(&data_dir).join("INSTRUCTIONS.md");

    let content = if path.exists() {
        std::fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    };

    Ok(Json(json!({"content": content})))
}

/// Generate PROFILE.json via LLM from a text description.
async fn generate_profile_json(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Path(id): Path<i64>,
    Json(body): Json<GenerateRequest>,
) -> Result<Json<profiles::Profile>, ApiErr> {
    require_write(&auth)?;
    let db = require_db(&state)?;
    let profile = profiles::db::load_profile_by_id(db.pool(), id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("Profile not found"))?;

    let config = state.config.read().await;

    let system_prompt = r#"You generate structured JSON profiles. Output ONLY valid JSON matching this schema:
{
  "version": "1.0",
  "identity": { "name": "", "display_name": "", "bio": "", "role": "", "avatar_emoji": "" },
  "linguistics": { "language": "", "formality": "", "style": "", "forbidden_words": [], "catchphrases": [] },
  "personality": { "traits": [], "tone": "", "humor": false },
  "capabilities": { "tools_emphasis": [], "domains": [] },
  "visibility": { "readable_from": ["default"] }
}
Fill all fields based on the user's description. Use the appropriate language for the description provided.
Output raw JSON only — no markdown fences, no explanation."#;

    let req = OneShotRequest {
        system_prompt: system_prompt.to_string(),
        user_message: body.description.clone(),
        max_tokens: 1024,
        temperature: 0.4,
        ..Default::default()
    };

    let response = llm_one_shot(&config, req)
        .await
        .map_err(|e| internal(e.context("LLM generation failed")))?;

    // Validate the generated JSON parses correctly
    let generated: serde_json::Value = serde_json::from_str(response.content.trim())
        .map_err(|e| internal(anyhow::anyhow!("LLM returned invalid JSON: {e}")))?;

    let profile_json = serde_json::to_string(&generated)
        .map_err(|e| internal(e.into()))?;

    profiles::db::update_profile(
        db.pool(),
        id,
        &profile.display_name,
        &profile.avatar_emoji,
        &profile_json,
    )
    .await
    .map_err(internal)?;

    let updated = profiles::db::load_profile_by_id(db.pool(), id)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("Profile not found after update"))?;

    Ok(Json(updated))
}
