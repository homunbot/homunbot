use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use rmcp::model::{CallToolRequestParams, RawContent, ResourceContents};
use rmcp::service::{RunningService, ServiceExt};
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use serde_json::Value;
use tokio::process::Command;
use tokio::sync::RwLock;

use crate::config::McpServerConfig;

use super::registry::{Tool, ToolContext, ToolResult};

/// Info about a connected MCP server (for TUI/status display)
#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub server_name: String,
    pub server_version: String,
    pub tool_count: usize,
    pub connected: bool,
}

/// A single MCP tool exposed as a HomunBot Tool.
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
}

/// Wrapper around the rmcp RunningService peer for shared access
struct McpPeer {
    service: RwLock<Option<RunningService<rmcp::service::RoleClient, ()>>>,
}

impl McpPeer {
    fn new(service: RunningService<rmcp::service::RoleClient, ()>) -> Self {
        Self {
            service: RwLock::new(Some(service)),
        }
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<String> {
        let guard = self.service.read().await;
        let service = guard
            .as_ref()
            .context("MCP server connection closed")?;

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
    pub async fn start(
        servers: &HashMap<String, McpServerConfig>,
    ) -> (Self, Vec<Box<dyn Tool>>) {
        let mut peers = Vec::new();
        let mut tools: Vec<Box<dyn Tool>> = Vec::new();
        let mut server_infos = Vec::new();

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

            match connect_server(name, config).await {
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
                        let input_schema =
                            Value::Object(mcp_tool.input_schema.as_ref().clone());

                        tools.push(Box::new(McpClientTool {
                            tool_name,
                            mcp_tool_name: mcp_tool.name.to_string(),
                            tool_description: description,
                            input_schema,
                            peer: peer.clone(),
                        }));
                    }

                    server_infos.push(info);
                    peers.push((name.clone(), peer));
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

        let manager = Self { peers, server_infos };
        (manager, tools)
    }

    /// Get info about all MCP servers (for TUI/status)
    pub fn server_infos(&self) -> &[McpServerInfo] {
        &self.server_infos
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
) -> Result<(McpPeer, Vec<rmcp::model::Tool>, McpServerInfo)> {
    match config.transport.as_str() {
        "stdio" => connect_stdio(name, config).await,
        other => anyhow::bail!("Unsupported MCP transport: {other}. Only 'stdio' is supported."),
    }
}

/// Connect to an MCP server via stdio transport (child process)
async fn connect_stdio(
    name: &str,
    config: &McpServerConfig,
) -> Result<(McpPeer, Vec<rmcp::model::Tool>, McpServerInfo)> {
    let cmd = config
        .command
        .as_deref()
        .context("MCP stdio server requires a 'command'")?;

    let env_vars: Vec<(String, String)> = config
        .env
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let args = config.args.clone();

    let transport = TokioChildProcess::new(
        Command::new(cmd).configure(move |c| {
            for arg in &args {
                c.arg(arg);
            }
            for (k, v) in &env_vars {
                c.env(k, v);
            }
        }),
    )
    .with_context(|| format!("Failed to spawn MCP server '{name}': {cmd}"))?;

    // Connect and perform MCP initialization handshake
    // () implements ClientHandler with default client info
    let service = ().serve(transport)
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
                enabled: false,
            },
        );
        let (manager, tools) = McpManager::start(&servers).await;
        assert!(tools.is_empty());
        assert_eq!(manager.server_infos().len(), 1);
        assert!(!manager.server_infos()[0].connected);
    }
}
