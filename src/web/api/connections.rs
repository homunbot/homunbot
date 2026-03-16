//! Connection Recipes REST API.
//!
//! Endpoints for discovering, connecting, testing, and listing services
//! via the Connection Recipes layer on top of MCP.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{delete, get, post};
use axum::Router;
use serde::Deserialize;

use crate::connections::recipes::{
    find_recipe, load_all_recipes, recipe_connection_status, recipe_instances,
};
use crate::connections::ConnectionCatalogItem;

use super::super::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/connections/catalog", get(catalog))
        .route("/v1/connections/recipes/{id}", get(recipe_detail))
        .route("/v1/connections/recipes/{id}/connect", post(connect))
        .route("/v1/connections/{name}/test", post(test_connection))
        .route("/v1/connections/{name}/capabilities", get(capabilities))
        .route("/v1/connections/{name}", delete(disconnect))
        .route("/v1/connections", get(list_connected))
}

// ── GET /v1/connections/catalog ──────────────────────────────────────

/// Return all recipes with their live connection status.
async fn catalog(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let config = state.config.read().await;
    let recipes = load_all_recipes();

    let items: Vec<ConnectionCatalogItem> = recipes
        .into_iter()
        .map(|recipe| {
            let status = recipe_connection_status(&recipe, &config);
            let instances = recipe_instances(&recipe, &config);
            ConnectionCatalogItem {
                recipe,
                connection_status: status,
                instances,
            }
        })
        .collect();

    Json(serde_json::json!({
        "ok": true,
        "items": items,
    }))
}

// ── GET /v1/connections/recipes/:id ──────────────────────────────────

/// Return a single recipe with connection status.
async fn recipe_detail(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let recipe = find_recipe(&id).ok_or(StatusCode::NOT_FOUND)?;
    let config = state.config.read().await;
    let status = recipe_connection_status(&recipe, &config);

    Ok(Json(serde_json::json!({
        "ok": true,
        "recipe": recipe,
        "connection_status": status,
    })))
}

// ── POST /v1/connections/recipes/:id/connect ─────────────────────────

#[derive(Deserialize)]
struct ConnectRequest {
    /// Field values keyed by field.id (e.g. { "personal_access_token": "ghp_..." })
    #[serde(default)]
    fields: HashMap<String, String>,
    /// Skip the connection test after setup (default: false).
    #[serde(default)]
    skip_test: bool,
    /// Instance name for multi-account support. Defaults to recipe id.
    #[serde(default)]
    instance_name: Option<String>,
}

/// Connect a service: store credentials, configure MCP server, optionally test.
async fn connect(
    Path(id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<ConnectRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let recipe = find_recipe(&id).ok_or(StatusCode::NOT_FOUND)?;

    let instance_name = req
        .instance_name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| recipe.id.clone());

    let mut config = state.config.read().await.clone();

    let result = crate::connections::connect::connect_recipe(
        &mut config,
        &recipe,
        &instance_name,
        &req.fields,
        req.skip_test,
    )
    .await
    .map_err(|e| {
        tracing::error!(recipe_id = %id, instance = %instance_name, error = %e, "Connection setup failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Persist config if setup succeeded (even if test failed — server is configured)
    state
        .save_config(config.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Hot-reload: register new MCP tools into the running agent's registry
    // so they're available immediately without restarting the gateway.
    #[cfg(feature = "mcp")]
    if result.ok {
        if let (Some(registry), Some(server_cfg)) =
            (&state.tool_registry, config.mcp.servers.get(&instance_name))
        {
            let name = instance_name.clone();
            let server = server_cfg.clone();
            let sandbox = config.security.execution_sandbox.clone();
            let runtime_cfg = Arc::clone(&state.config);
            let registry = Arc::clone(registry);
            tokio::spawn(async move {
                match crate::tools::McpManager::connect_single(
                    &name,
                    &server,
                    Some(sandbox),
                    Some(runtime_cfg),
                )
                .await
                {
                    Ok(tools) => {
                        let count = tools.len();
                        let mut reg = registry.write().await;
                        for tool in tools {
                            reg.register(tool);
                        }
                        tracing::info!(server = %name, tools = count, "Hot-reloaded MCP tools into registry");
                    }
                    Err(e) => {
                        tracing::warn!(server = %name, error = %e, "Hot-reload MCP connect failed (tools available on restart)");
                    }
                }
            });
        }
    }

    Ok(Json(serde_json::to_value(&result).unwrap_or_default()))
}

// ── POST /v1/connections/:name/test ──────────────────────────────────

/// Re-test an already-configured connection.
async fn test_connection(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let config = state.config.read().await;
    let server = config
        .mcp
        .servers
        .get(&name)
        .ok_or(StatusCode::NOT_FOUND)?
        .clone();
    let sandbox = config.security.execution_sandbox.clone();
    drop(config);

    let report = crate::mcp_setup::test_mcp_server_connection(&name, &server, Some(sandbox)).await;

    // Cache discovered tool count in config for catalog display
    if report.connected && report.tool_count > 0 {
        let mut config = state.config.read().await.clone();
        if let Some(srv) = config.mcp.servers.get_mut(&name) {
            srv.discovered_tool_count = Some(report.tool_count);
        }
        let _ = state.save_config(config).await;
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "connected": report.connected,
        "tool_count": report.tool_count,
        "server_name": report.server_name,
        "server_version": report.server_version,
        "error": report.error,
    })))
}

// ── GET /v1/connections/:name/capabilities ───────────────────────────

/// List tool names exposed by a connected MCP server.
///
/// This starts the server temporarily to discover its tools, then shuts it down.
async fn capabilities(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let config = state.config.read().await;
    let server = config
        .mcp
        .servers
        .get(&name)
        .ok_or(StatusCode::NOT_FOUND)?
        .clone();
    let sandbox = config.security.execution_sandbox.clone();
    drop(config);

    let tools = discover_tools(&name, &server, sandbox).await;

    Ok(Json(serde_json::json!({
        "ok": true,
        "name": name,
        "tools": tools,
    })))
}

/// Start an MCP server temporarily to list its tools, then shut it down.
#[cfg(feature = "mcp")]
async fn discover_tools(
    name: &str,
    server: &crate::config::McpServerConfig,
    sandbox: crate::config::ExecutionSandboxConfig,
) -> Vec<ToolInfo> {
    use crate::tools::McpManager;

    let mut servers = HashMap::new();
    servers.insert(name.to_string(), server.clone());

    let (manager, tools) = McpManager::start_with_sandbox(&servers, Some(sandbox), None).await;
    let result: Vec<ToolInfo> = tools
        .iter()
        .map(|t| ToolInfo {
            name: t.name().to_string(),
            description: t.description().to_string(),
        })
        .collect();
    manager.shutdown().await;
    result
}

#[cfg(not(feature = "mcp"))]
async fn discover_tools(
    _name: &str,
    _server: &crate::config::McpServerConfig,
    _sandbox: crate::config::ExecutionSandboxConfig,
) -> Vec<ToolInfo> {
    vec![]
}

#[derive(serde::Serialize)]
struct ToolInfo {
    name: String,
    description: String,
}

// ── DELETE /v1/connections/:name ──────────────────────────────────────

/// Disconnect a single server instance by name.
async fn disconnect(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut config = state.config.read().await.clone();
    if config.mcp.servers.remove(&name).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!(name = %name, "Connection instance disconnected");
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── GET /v1/connections ──────────────────────────────────────────────

/// List all services that are connected (recipe ↔ config.mcp.servers cross-reference).
async fn list_connected(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let config = state.config.read().await;
    let recipes = load_all_recipes();

    let connected: Vec<serde_json::Value> = recipes
        .into_iter()
        .flat_map(|recipe| {
            let instances = recipe_instances(&recipe, &config);
            instances
                .into_iter()
                .filter(|i| i.enabled)
                .map(move |inst| {
                    serde_json::json!({
                        "id": recipe.id,
                        "instance_name": inst.name,
                        "display_name": recipe.display_name,
                        "icon": recipe.icon,
                        "category": recipe.category,
                        "capability_intro": recipe.capability_intro,
                    })
                })
        })
        .collect();

    Json(serde_json::json!({
        "ok": true,
        "connections": connected,
    }))
}
