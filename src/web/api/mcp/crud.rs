use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Json;
use serde::{Deserialize, Serialize};

use super::helpers::{mcp_env_preview, normalize_mcp_capabilities};
use super::{McpServerEnvView, McpServerView};
use crate::web::server::AppState;

// ── Local OkResponse ─────────────────────────────────────────────

#[derive(Serialize)]
pub(super) struct OkResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

// ── List servers ─────────────────────────────────────────────────

pub(super) async fn list_mcp_servers(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<McpServerView>> {
    let config = state.config.read().await;
    let mut servers = config
        .mcp
        .servers
        .iter()
        .map(|(name, server)| {
            let mut env = server
                .env
                .iter()
                .map(|(key, value)| {
                    let (value_preview, is_vault_ref) = mcp_env_preview(value);
                    McpServerEnvView {
                        key: key.clone(),
                        value_preview,
                        is_vault_ref,
                    }
                })
                .collect::<Vec<_>>();
            env.sort_by(|a, b| a.key.cmp(&b.key));

            McpServerView {
                name: name.clone(),
                transport: server.transport.clone(),
                command: server.command.clone(),
                args: server.args.clone(),
                url: server.url.clone(),
                capabilities: normalize_mcp_capabilities(&server.capabilities),
                enabled: server.enabled,
                env,
            }
        })
        .collect::<Vec<_>>();
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    Json(servers)
}

// ── Guided setup (preset-based) ──────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct McpSetupRequest {
    service: String,
    name: Option<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    overwrite: Option<bool>,
    skip_test: Option<bool>,
}

#[derive(Serialize)]
pub(crate) struct McpSetupResponse {
    ok: bool,
    message: String,
    name: String,
    missing_required_env: Vec<String>,
    stored_vault_keys: Vec<String>,
    tested: bool,
    connected: Option<bool>,
    tool_count: Option<usize>,
}

pub(super) async fn setup_mcp_server(
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpSetupRequest>,
) -> Result<Json<McpSetupResponse>, StatusCode> {
    let Some(preset) = crate::skills::find_mcp_preset(&req.service) else {
        return Err(StatusCode::BAD_REQUEST);
    };

    let mut config = state.config.read().await.clone();
    let server_name = req.name.clone().unwrap_or_else(|| preset.id.clone());
    let overwrite = req.overwrite.unwrap_or(false);
    let skip_test = req.skip_test.unwrap_or(false);

    let setup = match crate::mcp_setup::apply_mcp_preset_setup(
        &mut config,
        &preset,
        &server_name,
        &req.env,
        overwrite,
    ) {
        Ok(result) => result,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("already exists") {
                return Err(StatusCode::CONFLICT);
            }
            tracing::warn!(error = %e, service = %req.service, "MCP setup failed");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    state
        .save_config(config.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !setup.missing_required_env.is_empty() {
        return Ok(Json(McpSetupResponse {
            ok: true,
            message: "Configured, but required env vars are still missing.".to_string(),
            name: server_name,
            missing_required_env: setup.missing_required_env,
            stored_vault_keys: setup.stored_vault_keys,
            tested: false,
            connected: None,
            tool_count: None,
        }));
    }

    if skip_test {
        return Ok(Json(McpSetupResponse {
            ok: true,
            message: "Configured. Connection test skipped.".to_string(),
            name: server_name,
            missing_required_env: vec![],
            stored_vault_keys: setup.stored_vault_keys,
            tested: false,
            connected: None,
            tool_count: None,
        }));
    }

    let Some(server) = config.mcp.servers.get(&server_name) else {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let report = crate::mcp_setup::test_mcp_server_connection(
        &server_name,
        server,
        Some(config.security.execution_sandbox.clone()),
    )
    .await;

    Ok(Json(McpSetupResponse {
        ok: true,
        message: if report.connected {
            format!(
                "Configured and connected ({} tools discovered).",
                report.tool_count
            )
        } else {
            match report.error.as_deref() {
                Some(err) => format!("Configured, but connection test failed: {err}"),
                None => "Configured, but connection test failed.".to_string(),
            }
        },
        name: server_name,
        missing_required_env: vec![],
        stored_vault_keys: setup.stored_vault_keys,
        tested: true,
        connected: Some(report.connected),
        tool_count: Some(report.tool_count),
    }))
}

// ── Upsert (manual create/update) ────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct McpServerUpsertRequest {
    name: String,
    transport: Option<String>,
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    url: Option<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    #[serde(default)]
    capabilities: Vec<String>,
    enabled: Option<bool>,
    overwrite: Option<bool>,
}

pub(super) async fn upsert_mcp_server(
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpServerUpsertRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let mut config = state.config.read().await.clone();
    let exists = config.mcp.servers.contains_key(&req.name);
    if exists && !req.overwrite.unwrap_or(false) {
        return Err(StatusCode::CONFLICT);
    }

    let transport = req.transport.unwrap_or_else(|| "stdio".to_string());
    if transport != "stdio" && transport != "http" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let command = req.command.map(|s| s.trim().to_string());
    let url = req.url.map(|s| s.trim().to_string());
    let args = req
        .args
        .iter()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect::<Vec<_>>();
    let env = req
        .env
        .iter()
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .filter(|(k, _)| !k.is_empty())
        .collect::<HashMap<_, _>>();
    let capabilities = normalize_mcp_capabilities(&req.capabilities);

    if transport == "stdio" && command.clone().unwrap_or_default().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if transport == "http" && url.clone().unwrap_or_default().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let server = crate::config::McpServerConfig {
        transport,
        command,
        args,
        url,
        env,
        capabilities,
        enabled: req.enabled.unwrap_or(true),
        recipe_id: None,
        auth_env_key: None,
        discovered_tool_count: None,
    };

    config.mcp.servers.insert(req.name.clone(), server);
    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(OkResponse {
        ok: true,
        message: Some(format!("MCP server '{}' saved", req.name)),
    }))
}

// ── Toggle ───────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct McpToggleRequest {
    enabled: Option<bool>,
}

pub(super) async fn toggle_mcp_server(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpToggleRequest>,
) -> Result<Json<OkResponse>, StatusCode> {
    let mut config = state.config.read().await.clone();
    let Some(server) = config.mcp.servers.get_mut(&name) else {
        return Err(StatusCode::NOT_FOUND);
    };

    let new_enabled = req.enabled.unwrap_or(!server.enabled);
    server.enabled = new_enabled;

    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !new_enabled {
        if let Some(db) = &state.db {
            let reason = format!("Missing or disabled MCP dependency: {name}");
            if let Err(e) = db
                .invalidate_automations_by_dependency("mcp", &name, &reason)
                .await
            {
                tracing::warn!(
                    error = %e,
                    server = %name,
                    "Failed to invalidate dependent automations after MCP disable"
                );
            }
        }
    }

    Ok(Json(OkResponse {
        ok: true,
        message: Some(format!(
            "MCP server '{}' {}",
            name,
            if new_enabled { "enabled" } else { "disabled" }
        )),
    }))
}

// ── Test ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub(crate) struct McpTestResponse {
    ok: bool,
    connected: bool,
    message: String,
    tool_count: usize,
    server_name: String,
    server_version: String,
    error: Option<String>,
}

pub(super) async fn test_mcp_server(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<McpTestResponse>, StatusCode> {
    let config = state.config.read().await;
    let Some(server) = config.mcp.servers.get(&name) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let server = server.clone();
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

    Ok(Json(McpTestResponse {
        ok: true,
        connected: report.connected,
        message: if report.connected {
            format!("Connected: {} tool(s) discovered.", report.tool_count)
        } else {
            match report.error.as_deref() {
                Some(err) => format!("Connection failed: {err}"),
                None => {
                    "Connection failed. Check command, args, and environment variables.".to_string()
                }
            }
        },
        tool_count: report.tool_count,
        server_name: report.server_name,
        server_version: report.server_version,
        error: report.error,
    }))
}

// ── Delete ───────────────────────────────────────────────────────

pub(super) async fn delete_mcp_server(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<OkResponse>, StatusCode> {
    let mut config = state.config.read().await.clone();
    if config.mcp.servers.remove(&name).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    state
        .save_config(config)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if let Some(db) = &state.db {
        let reason = format!("Missing or disabled MCP dependency: {name}");
        if let Err(e) = db
            .invalidate_automations_by_dependency("mcp", &name, &reason)
            .await
        {
            tracing::warn!(
                error = %e,
                server = %name,
                "Failed to invalidate dependent automations after MCP removal"
            );
        }
    }

    Ok(Json(OkResponse {
        ok: true,
        message: Some(format!("MCP server '{}' removed", name)),
    }))
}

// ── Discover tools (on-demand connection) ────────────────────────

/// Connect to an MCP server and return its discovered tools.
/// Used by the automations builder to populate tool dropdowns
/// without requiring the server to be connected at gateway startup.
pub(super) async fn list_mcp_server_tools(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let config = state.config.read().await;
    let Some(server) = config.mcp.servers.get(&name) else {
        return Err(StatusCode::NOT_FOUND);
    };
    let server = server.clone();
    let sandbox = config.security.execution_sandbox.clone();
    drop(config);

    match crate::tools::mcp::list_tools_once(&name, &server, &sandbox).await {
        Ok(tools) => Ok(Json(serde_json::json!({
            "ok": true,
            "server": name,
            "tools": tools,
        }))),
        Err(e) => Ok(Json(serde_json::json!({
            "ok": false,
            "server": name,
            "tools": [],
            "error": format!("{e:#}"),
        }))),
    }
}
