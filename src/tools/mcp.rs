use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use rmcp::model::{CallToolRequestParams, RawContent, ReadResourceRequestParams, ResourceContents};
use rmcp::service::{RunningService, ServiceExt};
use rmcp::transport::TokioChildProcess;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::config::{Config, ExecutionSandboxConfig, McpServerConfig};
use crate::storage::{global_secrets, SecretKey};

use super::registry::{Tool, ToolContext, ToolResult};
use super::sandbox::build_process_command;

/// Info about a connected MCP server (for TUI/status display)
#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub server_name: String,
    pub server_version: String,
    pub tool_count: usize,
    pub connected: bool,
}

#[derive(Debug, Clone)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
}

/// A single MCP tool exposed as a Homun Tool.
///
/// Each tool discovered from an MCP server becomes one of these.
/// The tool name is prefixed with the server name to avoid collisions:
/// e.g., "filesystem__read_file" for tool "read_file" from server "filesystem".
pub struct McpClientTool {
    /// Full tool name as registered in ToolRegistry (server__tool)
    tool_name: String,
    /// Original tool name on the MCP server
    mcp_tool_name: String,
    /// Tool description from the MCP server
    tool_description: String,
    /// JSON Schema for tool parameters
    input_schema: Value,
    /// Shared reference to the running MCP service peer
    peer: Arc<McpPeer>,
    /// MCP server alias from config (key in [mcp.servers]).
    server_name: String,
    /// Optional shared config for runtime hot-reload.
    runtime_config: Option<Arc<RwLock<Config>>>,
}

/// Wrapper around the rmcp RunningService peer for shared access.
///
/// Used by `McpClientTool` for individual MCP tools and by `BrowserTool`
/// for the unified browser interface (calling Playwright tools through a
/// single `browser` tool).
pub struct McpPeer {
    service: RwLock<Option<RunningService<rmcp::service::RoleClient, ()>>>,
}

impl McpPeer {
    fn new(service: RunningService<rmcp::service::RoleClient, ()>) -> Self {
        Self {
            service: RwLock::new(Some(service)),
        }
    }

    pub async fn call_tool(&self, name: &str, args: Value) -> Result<String> {
        let guard = self.service.read().await;
        let service = guard.as_ref().context("MCP server connection closed")?;

        let arguments = args.as_object().cloned();

        let result = service
            .call_tool(CallToolRequestParams {
                name: name.to_string().into(),
                arguments,
                meta: None,
                task: None,
            })
            .await
            .context("MCP tool call failed")?;

        // Convert content blocks to text
        let mut output = String::new();
        for content in &result.content {
            match &content.raw {
                RawContent::Text(text) => {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str(&text.text);
                }
                RawContent::Image(img) => {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str(&format!(
                        "[image: {} ({} bytes)]",
                        img.mime_type,
                        img.data.len()
                    ));
                }
                RawContent::Resource(res) => {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    let uri = match &res.resource {
                        ResourceContents::TextResourceContents { uri, .. } => uri.as_str(),
                        ResourceContents::BlobResourceContents { uri, .. } => uri.as_str(),
                    };
                    output.push_str(&format!("[resource: {uri}]"));
                }
                _ => {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str("[unknown content type]");
                }
            }
        }

        if result.is_error.unwrap_or(false) {
            anyhow::bail!("{output}");
        }

        Ok(output)
    }

    /// List all resources available from this MCP server.
    pub async fn list_resources(&self) -> Result<Vec<rmcp::model::Resource>> {
        let guard = self.service.read().await;
        let service = guard.as_ref().context("MCP server connection closed")?;
        let result = service
            .list_resources(None)
            .await
            .context("MCP list_resources failed")?;
        Ok(result.resources)
    }

    /// Read a specific resource by URI.
    pub async fn read_resource(&self, uri: &str) -> Result<Vec<ResourceContents>> {
        let guard = self.service.read().await;
        let service = guard.as_ref().context("MCP server connection closed")?;
        let result = service
            .read_resource(ReadResourceRequestParams {
                uri: uri.to_string(),
                meta: None,
            })
            .await
            .context("MCP read_resource failed")?;
        Ok(result.contents)
    }

    async fn shutdown(&self) {
        let mut guard = self.service.write().await;
        if let Some(service) = guard.take() {
            if let Err(e) = service.cancel().await {
                tracing::warn!(error = %e, "Error shutting down MCP server");
            }
        }
    }
}

#[async_trait]
impl Tool for McpClientTool {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.tool_description
    }

    fn parameters(&self) -> Value {
        self.input_schema.clone()
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        // Resolve vault:// references in tool arguments (e.g. secure form filling)
        let mut args = args;
        resolve_vault_args(&mut args);

        // Stateful servers (e.g. browser/playwright) must use the persistent peer
        // connection — spawning a fresh process per call would lose page state.
        // The runtime_config (hot-reload) path is only used for stateless servers.
        let use_persistent = self.server_name == crate::browser::BROWSER_MCP_SERVER_NAME;

        if !use_persistent {
            if let Some(config_handle) = &self.runtime_config {
                let (server_config, sandbox_config) = {
                    let cfg = config_handle.read().await;
                    let Some(server) = cfg.mcp.servers.get(&self.server_name) else {
                        return Ok(ToolResult::error(format!(
                            "MCP server '{}' is no longer configured",
                            self.server_name
                        )));
                    };
                    (server.clone(), cfg.security.execution_sandbox.clone())
                };

                match call_tool_once(
                    &self.server_name,
                    &server_config,
                    &sandbox_config,
                    &self.mcp_tool_name,
                    args,
                )
                .await
                {
                    Ok(output) => return Ok(ToolResult::success(output)),
                    Err(e) => {
                        return Ok(ToolResult::error(format!("MCP tool error: {e}")));
                    }
                }
            }
        }

        match self.peer.call_tool(&self.mcp_tool_name, args).await {
            Ok(output) => Ok(ToolResult::success(output)),
            Err(e) => Ok(ToolResult::error(format!("MCP tool error: {e}"))),
        }
    }
}

/// Manages all MCP server connections and their tools.
///
/// Lifecycle:
/// 1. `start()` — spawns configured MCP server processes, performs init handshake
/// 2. Tools are registered into the ToolRegistry
/// 3. `shutdown()` — gracefully closes all connections
pub struct McpManager {
    peers: Vec<(String, Arc<McpPeer>)>,
    server_infos: Vec<McpServerInfo>,
}

impl McpManager {
    /// Connect to all enabled MCP servers from config.
    /// Returns the manager and a list of Tool trait objects to register.
    pub async fn start(servers: &HashMap<String, McpServerConfig>) -> (Self, Vec<Box<dyn Tool>>) {
        Self::start_with_sandbox(servers, None, None).await
    }

    /// Connect to all enabled MCP servers using optional execution sandbox config.
    pub async fn start_with_sandbox(
        servers: &HashMap<String, McpServerConfig>,
        sandbox_config: Option<ExecutionSandboxConfig>,
        runtime_config: Option<Arc<RwLock<Config>>>,
    ) -> (Self, Vec<Box<dyn Tool>>) {
        let mut peers = Vec::new();
        let mut tools: Vec<Box<dyn Tool>> = Vec::new();
        let mut server_infos = Vec::new();
        let sandbox_config = sandbox_config.unwrap_or_default();
        let runtime_hot_reload = runtime_config.is_some();

        for (name, config) in servers {
            if !config.enabled {
                tracing::debug!(server = %name, "MCP server disabled, skipping");
                server_infos.push(McpServerInfo {
                    name: name.clone(),
                    server_name: String::new(),
                    server_version: String::new(),
                    tool_count: 0,
                    connected: false,
                });
                continue;
            }

            match connect_server(name, config, &sandbox_config).await {
                Ok((peer, discovered_tools, info)) => {
                    let peer = Arc::new(peer);
                    tracing::info!(
                        server = %name,
                        tools = discovered_tools.len(),
                        server_name = %info.server_name,
                        "MCP server connected"
                    );

                    // Create Tool wrappers for each discovered MCP tool
                    for mcp_tool in discovered_tools {
                        let tool_name = format!("{}__{}", name, mcp_tool.name);
                        let description = mcp_tool
                            .description
                            .as_deref()
                            .unwrap_or("No description")
                            .to_string();
                        // input_schema is Arc<JsonObject> (Arc<Map<String, Value>>)
                        // Convert to Value::Object for our Tool trait
                        let input_schema = Value::Object(mcp_tool.input_schema.as_ref().clone());

                        tools.push(Box::new(McpClientTool {
                            tool_name,
                            mcp_tool_name: mcp_tool.name.to_string(),
                            tool_description: description,
                            input_schema,
                            peer: peer.clone(),
                            server_name: name.clone(),
                            runtime_config: runtime_config.clone(),
                        }));
                    }

                    server_infos.push(info);
                    // Stateful servers (e.g. browser) need a persistent connection —
                    // their peer must survive across tool calls to keep state (page, DOM).
                    // Stateless servers in hot-reload mode reconnect on each call.
                    let needs_persistent = name == crate::browser::BROWSER_MCP_SERVER_NAME;
                    if runtime_hot_reload && !needs_persistent {
                        // Stateless hot-reload: reconnect on each tool call via call_tool_once(),
                        // so we can immediately release the startup discovery connection.
                        peer.shutdown().await;
                    } else {
                        peers.push((name.clone(), peer));
                    }
                }
                Err(e) => {
                    tracing::warn!(server = %name, error = %e, "Failed to connect MCP server");
                    server_infos.push(McpServerInfo {
                        name: name.clone(),
                        server_name: String::new(),
                        server_version: String::new(),
                        tool_count: 0,
                        connected: false,
                    });
                }
            }
        }

        let manager = Self {
            peers,
            server_infos,
        };
        (manager, tools)
    }

    /// Get info about all MCP servers (for TUI/status)
    pub fn server_infos(&self) -> &[McpServerInfo] {
        &self.server_infos
    }

    /// Take the persistent browser peer out of the manager.
    ///
    /// Returns `Some(Arc<McpPeer>)` if a browser MCP server was connected.
    /// The peer is removed from the manager — subsequent calls return `None`.
    /// Used by `BrowserTool` to get direct access to the Playwright connection.
    pub fn take_browser_peer(&mut self) -> Option<Arc<McpPeer>> {
        let idx = self
            .peers
            .iter()
            .position(|(n, _)| n == crate::browser::BROWSER_MCP_SERVER_NAME)?;
        Some(self.peers.remove(idx).1)
    }

    /// Get a reference to a peer by server name.
    pub fn get_peer(&self, name: &str) -> Option<Arc<McpPeer>> {
        self.peers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| Arc::clone(p))
    }

    /// Shutdown all MCP server connections
    pub async fn shutdown(&self) {
        for (name, peer) in &self.peers {
            tracing::debug!(server = %name, "Shutting down MCP server");
            peer.shutdown().await;
        }
    }
}

/// Connect to a single MCP server and discover its tools
async fn connect_server(
    name: &str,
    config: &McpServerConfig,
    sandbox_config: &ExecutionSandboxConfig,
) -> Result<(McpPeer, Vec<rmcp::model::Tool>, McpServerInfo)> {
    match config.transport.as_str() {
        "stdio" => connect_stdio(name, config, sandbox_config).await,
        other => anyhow::bail!("Unsupported MCP transport: {other}. Only 'stdio' is supported."),
    }
}

pub async fn call_tool_once(
    server_name: &str,
    server_config: &McpServerConfig,
    sandbox_config: &ExecutionSandboxConfig,
    tool_name: &str,
    args: Value,
) -> Result<String> {
    let (peer, _tools, _info) = connect_server(server_name, server_config, sandbox_config).await?;
    let result = peer.call_tool(tool_name, args).await;
    peer.shutdown().await;
    result
}

pub async fn list_tools_once(
    server_name: &str,
    server_config: &McpServerConfig,
    sandbox_config: &ExecutionSandboxConfig,
) -> Result<Vec<McpToolInfo>> {
    let (peer, tools, _info) = connect_server(server_name, server_config, sandbox_config).await?;
    let out = tools
        .into_iter()
        .map(|tool| McpToolInfo {
            name: tool.name.to_string(),
            description: tool.description.unwrap_or_default().to_string(),
        })
        .collect();
    peer.shutdown().await;
    Ok(out)
}

/// Connect to an MCP server via stdio transport (child process)
async fn connect_stdio(
    name: &str,
    config: &McpServerConfig,
    sandbox_config: &ExecutionSandboxConfig,
) -> Result<(McpPeer, Vec<rmcp::model::Tool>, McpServerInfo)> {
    let cmd = config
        .command
        .as_deref()
        .context("MCP stdio server requires a 'command'")?;

    let env_vars: Vec<(String, String)> = config
        .env
        .iter()
        .map(|(k, v)| {
            resolve_env_value(name, k, v)
                .map(|resolved| (k.clone(), resolved))
                .with_context(|| format!("Failed to resolve env var '{k}' for MCP '{name}'"))
        })
        .collect::<Result<Vec<_>>>()?;
    let args = config.args.clone();
    let env_map: HashMap<String, String> = env_vars.into_iter().collect();
    let workspace_dir = crate::config::Config::workspace_dir();
    let _ = std::fs::create_dir_all(&workspace_dir);

    // Browser MCP (Playwright) must bypass the sandbox: it needs native access
    // to launch browser processes, display libs, and user data dirs.
    // Docker/bubblewrap containers lack these — Playwright would fail to start.
    let effective_sandbox = if name == crate::browser::BROWSER_MCP_SERVER_NAME {
        tracing::debug!(
            server = %name,
            "Browser MCP server bypasses execution sandbox (needs native browser access)"
        );
        ExecutionSandboxConfig::disabled()
    } else {
        sandbox_config.clone()
    };

    let process_cmd = build_process_command(
        "mcp",
        cmd,
        &args,
        &workspace_dir,
        &env_map,
        true,
        &effective_sandbox,
    )
    .with_context(|| format!("Failed to prepare MCP command for server '{name}'"))?;

    let transport = TokioChildProcess::new(process_cmd)
        .with_context(|| format!("Failed to spawn MCP server '{name}': {cmd}"))?;

    // Connect and perform MCP initialization handshake
    // () implements ClientHandler with default client info
    let service = ()
        .serve(transport)
        .await
        .with_context(|| format!("MCP initialization failed for server '{name}'"))?;

    // Get server info (peer_info returns Option<&ServerInfo>)
    let (server_name, server_version) = match service.peer_info() {
        Some(info) => (
            info.server_info.name.to_string(),
            info.server_info.version.to_string(),
        ),
        None => ("unknown".to_string(), "unknown".to_string()),
    };

    // Discover tools
    let tools = service
        .list_all_tools()
        .await
        .with_context(|| format!("Failed to list tools from MCP server '{name}'"))?;

    let info = McpServerInfo {
        name: name.to_string(),
        server_name,
        server_version,
        tool_count: tools.len(),
        connected: true,
    };

    Ok((McpPeer::new(service), tools, info))
}

/// Recursively resolve `vault://` references in MCP tool arguments.
///
/// Walks the JSON argument tree and replaces any string value starting
/// with `vault://` with the corresponding secret from the vault.
/// This enables secure form filling via browser MCP tools and any
/// other MCP tool that accepts sensitive data.
fn resolve_vault_args(args: &mut Value) {
    match args {
        Value::String(s) if s.starts_with("vault://") => {
            let key = s.strip_prefix("vault://").unwrap_or_default().trim();
            if !key.is_empty() {
                if let Ok(secrets) = global_secrets() {
                    let secret_key = SecretKey::custom(&format!("vault.{key}"));
                    if let Ok(Some(value)) = secrets.get(&secret_key) {
                        *s = value;
                    } else {
                        tracing::warn!(vault_key = %key, "Vault secret not found in MCP tool argument");
                    }
                }
            }
        }
        Value::Object(map) => {
            for value in map.values_mut() {
                resolve_vault_args(value);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                resolve_vault_args(item);
            }
        }
        _ => {}
    }
}

/// Resolve MCP env value, supporting vault references (`vault://key_name`).
fn resolve_env_value(server_name: &str, env_key: &str, raw_value: &str) -> Result<String> {
    if !raw_value.starts_with("vault://") {
        return Ok(raw_value.to_string());
    }

    let Some(vault_key) = raw_value.strip_prefix("vault://") else {
        anyhow::bail!("Invalid vault reference in env var '{env_key}'");
    };
    if vault_key.trim().is_empty() {
        anyhow::bail!("Empty vault key in env var '{env_key}'");
    }

    let secrets = global_secrets().context("Failed to access vault")?;
    let key = SecretKey::custom(&format!("vault.{}", vault_key.trim()));
    match secrets.get(&key)? {
        Some(value) => Ok(value),
        None => anyhow::bail!(
            "Vault secret '{vault_key}' not found (required by MCP '{server_name}', env '{env_key}')"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name_format() {
        let name = format!("{}__{}", "filesystem", "read_file");
        assert_eq!(name, "filesystem__read_file");
    }

    #[test]
    fn test_server_info_default() {
        let info = McpServerInfo {
            name: "test".to_string(),
            server_name: "TestServer".to_string(),
            server_version: "1.0".to_string(),
            tool_count: 3,
            connected: true,
        };
        assert!(info.connected);
        assert_eq!(info.tool_count, 3);
    }

    #[tokio::test]
    async fn test_empty_manager() {
        let servers = HashMap::new();
        let (manager, tools) = McpManager::start(&servers).await;
        assert!(tools.is_empty());
        assert!(manager.server_infos().is_empty());
        manager.shutdown().await;
    }

    #[tokio::test]
    async fn test_disabled_server_skipped() {
        let mut servers = HashMap::new();
        servers.insert(
            "test".to_string(),
            McpServerConfig {
                transport: "stdio".to_string(),
                command: Some("echo".to_string()),
                args: vec![],
                url: None,
                env: HashMap::new(),
                capabilities: Vec::new(),
                enabled: false,
            },
        );
        let (manager, tools) = McpManager::start(&servers).await;
        assert!(tools.is_empty());
        assert_eq!(manager.server_infos().len(), 1);
        assert!(!manager.server_infos()[0].connected);
    }
}
