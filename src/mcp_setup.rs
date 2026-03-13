use std::collections::HashMap;

use anyhow::{Context, Result};

use crate::config::{Config, ExecutionSandboxConfig, McpServerConfig};
use crate::skills::McpServerPreset;
use crate::storage::{global_secrets, SecretKey};

/// Result of applying a guided MCP setup preset.
pub struct McpSetupResult {
    pub stored_vault_keys: Vec<String>,
    pub missing_required_env: Vec<String>,
}

/// Connection test result for an MCP server.
#[derive(Debug, Clone)]
pub struct McpConnectionTestResult {
    pub connected: bool,
    pub tool_count: usize,
    pub server_name: String,
    pub server_version: String,
    pub error: Option<String>,
}

/// Replace known templates in MCP command args.
pub fn render_mcp_arg_template(arg: &str) -> String {
    let workspace = Config::workspace_dir().display().to_string();
    let home = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .display()
        .to_string();
    arg.replace("{{workspace}}", &workspace)
        .replace("{{home}}", &home)
}

/// Parse `KEY=VALUE` strings into an environment map.
pub fn parse_env_assignments(env: &[String]) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();

    for raw in env {
        let Some((key, value)) = raw.split_once('=') else {
            anyhow::bail!("Invalid --env value '{raw}'. Expected KEY=VALUE format.");
        };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() {
            anyhow::bail!("Invalid --env value '{raw}'. KEY cannot be empty.");
        }
        out.insert(key.to_string(), value.to_string());
    }

    Ok(out)
}

/// Apply a curated MCP preset to the config, resolving secret env vars into vault refs.
pub fn apply_mcp_preset_setup(
    config: &mut Config,
    preset: &McpServerPreset,
    server_name: &str,
    env_overrides: &HashMap<String, String>,
    overwrite: bool,
) -> Result<McpSetupResult> {
    let existing = config.mcp.servers.get(server_name).cloned();
    if existing.is_some() && !overwrite {
        anyhow::bail!("MCP server '{server_name}' already exists. Use --overwrite to replace it.");
    }

    let mut merged_env = existing.as_ref().map(|s| s.env.clone()).unwrap_or_default();
    let mut stored_vault_keys = Vec::new();
    let mut missing_required_env = Vec::new();

    for required_env in &preset.env {
        let chosen = env_overrides
            .get(&required_env.key)
            .cloned()
            .or_else(|| merged_env.get(&required_env.key).cloned())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());

        let Some(value) = chosen else {
            if required_env.required {
                missing_required_env.push(required_env.key.clone());
            }
            continue;
        };

        if required_env.secret {
            if value.starts_with("vault://") {
                merged_env.insert(required_env.key.clone(), value);
            } else {
                let vault_ref = format!("vault://{}", required_env.vault_key);
                let secrets =
                    global_secrets().context("Failed to access vault while saving MCP secret")?;
                let key = SecretKey::custom(&format!("vault.{}", required_env.vault_key));
                secrets
                    .set(&key, &value)
                    .with_context(|| format!("Failed to store secret '{}'", required_env.key))?;
                merged_env.insert(required_env.key.clone(), vault_ref);
                if !stored_vault_keys.contains(&required_env.vault_key) {
                    stored_vault_keys.push(required_env.vault_key.clone());
                }
            }
        } else {
            merged_env.insert(required_env.key.clone(), value);
        }
    }

    // Extra env values passed by user that are not part of the preset schema.
    for (key, value) in env_overrides {
        if !preset.env.iter().any(|spec| spec.key == *key) {
            merged_env.insert(key.clone(), value.clone());
        }
    }

    let args = preset
        .args
        .iter()
        .map(|arg| render_mcp_arg_template(arg))
        .collect::<Vec<_>>();

    let server_config = McpServerConfig {
        transport: "stdio".to_string(),
        command: Some(preset.command.clone()),
        args,
        url: None,
        env: merged_env,
        capabilities: Vec::new(),
        enabled: true,
        recipe_id: None,
    };
    config
        .mcp
        .servers
        .insert(server_name.to_string(), server_config);

    Ok(McpSetupResult {
        stored_vault_keys,
        missing_required_env,
    })
}

/// Test a single MCP server configuration by trying to connect and list tools.
#[cfg(feature = "mcp")]
pub async fn test_mcp_server_connection(
    name: &str,
    server: &McpServerConfig,
    sandbox: Option<ExecutionSandboxConfig>,
) -> McpConnectionTestResult {
    use crate::tools::sandbox::resolve::resolve_sandbox_backend;
    use crate::tools::McpManager;

    let mut servers = HashMap::new();
    servers.insert(name.to_string(), server.clone());

    let sandbox = sandbox.unwrap_or_default();
    if let Err(e) = resolve_sandbox_backend(&sandbox) {
        return McpConnectionTestResult {
            connected: false,
            tool_count: 0,
            server_name: String::new(),
            server_version: String::new(),
            error: Some(format!("Sandbox preflight failed: {e}")),
        };
    }

    let (manager, _tools) = McpManager::start_with_sandbox(&servers, Some(sandbox), None).await;
    let info = manager
        .server_infos()
        .iter()
        .find(|info| info.name == name)
        .cloned();
    manager.shutdown().await;

    if let Some(info) = info {
        McpConnectionTestResult {
            connected: info.connected,
            tool_count: info.tool_count,
            server_name: info.server_name,
            server_version: info.server_version,
            error: None,
        }
    } else {
        McpConnectionTestResult {
            connected: false,
            tool_count: 0,
            server_name: String::new(),
            server_version: String::new(),
            error: Some("MCP connection test returned no server info".to_string()),
        }
    }
}

/// Fallback when MCP feature is not enabled in this build.
#[cfg(not(feature = "mcp"))]
pub async fn test_mcp_server_connection(
    _name: &str,
    _server: &McpServerConfig,
    _sandbox: Option<ExecutionSandboxConfig>,
) -> McpConnectionTestResult {
    McpConnectionTestResult {
        connected: false,
        tool_count: 0,
        server_name: String::new(),
        server_version: String::new(),
        error: Some("MCP feature is disabled in this build".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_server() -> McpServerConfig {
        McpServerConfig {
            transport: "stdio".to_string(),
            command: Some("echo".to_string()),
            args: vec!["hello".to_string()],
            url: None,
            env: HashMap::new(),
            capabilities: Vec::new(),
            enabled: true,
            recipe_id: None,
        }
    }

    #[cfg(feature = "mcp")]
    #[tokio::test]
    async fn strict_invalid_backend_fails_in_preflight() {
        let sandbox = ExecutionSandboxConfig {
            enabled: true,
            backend: "invalid".to_string(),
            strict: true,
            ..ExecutionSandboxConfig::default()
        };
        let report = test_mcp_server_connection("demo", &sample_server(), Some(sandbox)).await;
        assert!(!report.connected);
        assert!(report
            .error
            .unwrap_or_default()
            .contains("Sandbox preflight failed"));
    }

    #[cfg(not(feature = "mcp"))]
    #[tokio::test]
    async fn test_returns_feature_disabled_without_mcp_runtime() {
        let report = test_mcp_server_connection(
            "demo",
            &sample_server(),
            Some(ExecutionSandboxConfig::default()),
        )
        .await;
        assert!(!report.connected);
        assert_eq!(
            report.error.unwrap_or_default(),
            "MCP feature is disabled in this build"
        );
    }
}
