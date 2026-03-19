//! Multi-agent CRUD API.
//!
//! Manages `[agents.*]` definitions and `[routing]` config via the Web UI.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::agent::AgentDefinition;
use crate::config::AgentDefinitionConfig;

use super::super::server::AppState;
use super::OkResponse;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/agents", axum::routing::get(list_agents))
        .route("/v1/agents", axum::routing::post(create_agent))
        .route(
            "/v1/agents/{id}",
            axum::routing::put(update_agent).delete(delete_agent),
        )
        .route(
            "/v1/agents/routing",
            axum::routing::get(get_routing).put(update_routing),
        )
}

// ── Response types ──────────────────────────────────────────────────

#[derive(Serialize)]
struct AgentView {
    id: String,
    model: String,
    instructions: String,
    tools: Vec<String>,
    skills: Vec<String>,
    max_concurrency: usize,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    is_default: bool,
    /// True if this agent was synthesized from [agent] global config.
    is_implicit: bool,
}

#[derive(Serialize)]
struct RoutingView {
    classifier_model: String,
    agent_count: usize,
}

// ── Request types ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateAgentRequest {
    id: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    instructions: String,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    skills: Vec<String>,
    #[serde(default)]
    max_concurrency: usize,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    max_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct UpdateAgentRequest {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    instructions: Option<String>,
    #[serde(default)]
    tools: Option<Vec<String>>,
    #[serde(default)]
    skills: Option<Vec<String>>,
    #[serde(default)]
    max_concurrency: Option<usize>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    max_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct UpdateRoutingRequest {
    classifier_model: String,
}

// ── Handlers ────────────────────────────────────────────────────────

/// List all agent definitions (resolved, always includes "default").
async fn list_agents(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AgentView>>, StatusCode> {
    let config = state.config.read().await;
    let definitions = AgentDefinition::resolve_all(&config);
    let explicit_ids: std::collections::HashSet<&str> =
        config.agents.keys().map(|s| s.as_str()).collect();

    let mut views: Vec<AgentView> = definitions
        .into_iter()
        .map(|(id, def)| {
            let is_implicit = !explicit_ids.contains(id.as_str());
            AgentView {
                is_default: id == "default",
                is_implicit,
                id,
                model: def.model,
                instructions: def.instructions,
                tools: def.allowed_tools,
                skills: def.allowed_skills,
                max_concurrency: def.max_concurrency,
                temperature: def.temperature,
                max_tokens: def.max_tokens,
            }
        })
        .collect();

    // Sort: default first, then alphabetical
    views.sort_by(|a, b| b.is_default.cmp(&a.is_default).then(a.id.cmp(&b.id)));

    Ok(Json(views))
}

/// Create a new agent definition.
async fn create_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let id = req.id.to_lowercase().trim().to_string();
    if id.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut config = state.config.read().await.clone();
    if config.agents.contains_key(&id) {
        return Err(StatusCode::CONFLICT);
    }

    config.agents.insert(
        id.clone(),
        AgentDefinitionConfig {
            model: req.model,
            instructions: req.instructions,
            tools: req.tools,
            skills: req.skills,
            max_concurrency: req.max_concurrency,
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            fallback_models: Vec::new(),
        },
    );

    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!(agent_id = %id, "Agent created via Web UI");

    Ok(Json(OkResponse {
        ok: true,
        message: Some(format!("Agent '{id}' created")),
    }))
}

/// Update an existing agent definition (partial update).
async fn update_agent(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateAgentRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let mut config = state.config.read().await.clone();

    let entry = config
        .agents
        .entry(id.clone())
        .or_insert_with(AgentDefinitionConfig::default);

    if let Some(model) = req.model {
        entry.model = model;
    }
    if let Some(instructions) = req.instructions {
        entry.instructions = instructions;
    }
    if let Some(tools) = req.tools {
        entry.tools = tools;
    }
    if let Some(skills) = req.skills {
        entry.skills = skills;
    }
    if let Some(max_concurrency) = req.max_concurrency {
        entry.max_concurrency = max_concurrency;
    }
    if req.temperature.is_some() {
        entry.temperature = req.temperature;
    }
    if req.max_tokens.is_some() {
        entry.max_tokens = req.max_tokens;
    }

    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!(agent_id = %id, "Agent updated via Web UI");

    Ok(Json(OkResponse {
        ok: true,
        message: Some(format!("Agent '{id}' updated")),
    }))
}

/// Delete an agent definition. Cannot delete "default".
async fn delete_agent(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<OkResponse>, StatusCode> {
    if id == "default" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut config = state.config.read().await.clone();
    if config.agents.remove(&id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!(agent_id = %id, "Agent deleted via Web UI");

    Ok(Json(OkResponse {
        ok: true,
        message: Some(format!("Agent '{id}' deleted")),
    }))
}

/// Get routing configuration.
async fn get_routing(
    State(state): State<Arc<AppState>>,
) -> Result<Json<RoutingView>, StatusCode> {
    let config = state.config.read().await;
    let definitions = AgentDefinition::resolve_all(&config);

    Ok(Json(RoutingView {
        classifier_model: config.routing.classifier_model.clone(),
        agent_count: definitions.len(),
    }))
}

/// Update routing configuration.
async fn update_routing(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateRoutingRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let mut config = state.config.read().await.clone();
    config.routing.classifier_model = req.classifier_model;

    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!("Routing config updated via Web UI");

    Ok(Json(OkResponse {
        ok: true,
        message: Some("Routing config updated".to_string()),
    }))
}
