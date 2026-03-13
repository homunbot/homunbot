//! Connection orchestrator — execute the connect flow for a recipe.
//!
//! Flow: UI fields → env overrides → `apply_mcp_preset_setup` → optional test → result.

use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::{Config, ExecutionSandboxConfig};
use crate::mcp_setup::{self, McpConnectionTestResult};

use super::recipes::recipe_to_preset;
use super::{ConnectionRecipe, SuccessCopy};

/// Result of a connect operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectResult {
    pub ok: bool,
    pub message: String,
    /// Whether the connection test succeeded (None if test was skipped).
    pub connected: Option<bool>,
    /// Number of tools exposed by the MCP server (0 if test was skipped).
    pub tool_count: usize,
    /// Vault keys that were stored during setup.
    pub stored_vault_keys: Vec<String>,
    /// Success copy from the recipe (only set when ok=true).
    pub success: Option<SuccessCopy>,
}

/// Connect a service using a recipe and user-provided field values.
///
/// `instance_name` is the MCP server key in config (e.g. "gmail", "gmail-work").
/// For a single-instance recipe this equals `recipe.id`; for multi-instance it
/// can be any user-chosen name. Vault keys and config entries are namespaced
/// by `instance_name` to keep credentials isolated.
pub async fn connect_recipe(
    config: &mut Config,
    recipe: &ConnectionRecipe,
    instance_name: &str,
    field_values: &HashMap<String, String>,
    sandbox: Option<ExecutionSandboxConfig>,
    skip_test: bool,
) -> Result<ConnectResult> {
    // 1. Map field IDs → env keys
    let mut env_overrides = HashMap::new();
    for field in &recipe.fields {
        if let Some(value) = field_values.get(&field.id) {
            if !value.trim().is_empty() {
                env_overrides.insert(field.env_key.clone(), value.clone());
            }
        }
    }

    // 2. Convert recipe → preset (vault keys scoped to instance_name)
    let preset = recipe_to_preset(recipe, instance_name);

    // 3. Apply setup (stores secrets in vault, writes config)
    let setup_result = mcp_setup::apply_mcp_preset_setup(
        config,
        &preset,
        instance_name,
        &env_overrides,
        true, // overwrite existing
    )?;

    // Tag the server with its source recipe for multi-instance discovery
    if let Some(server) = config.mcp.servers.get_mut(instance_name) {
        server.recipe_id = Some(recipe.id.clone());
    }

    if !setup_result.missing_required_env.is_empty() {
        return Ok(ConnectResult {
            ok: false,
            message: format!(
                "Missing required fields: {}",
                setup_result.missing_required_env.join(", ")
            ),
            connected: None,
            tool_count: 0,
            stored_vault_keys: setup_result.stored_vault_keys,
            success: None,
        });
    }

    // 4. Optionally test the connection
    let (connected, tool_count, test_error) = if skip_test {
        (None, 0, None)
    } else {
        let server = config
            .mcp
            .servers
            .get(instance_name)
            .cloned()
            .expect("server should exist after setup");

        let test = mcp_setup::test_mcp_server_connection(instance_name, &server, sandbox).await;
        (Some(test.connected), test.tool_count, test.error)
    };

    let ok = connected.unwrap_or(true);
    let message = if ok {
        recipe.success.title.clone()
    } else if let Some(err) = &test_error {
        format!("Connection test failed: {err}")
    } else {
        "Connection test failed. The server was configured but could not connect.".to_string()
    };

    Ok(ConnectResult {
        ok,
        message,
        connected,
        tool_count,
        stored_vault_keys: setup_result.stored_vault_keys,
        success: if ok {
            Some(recipe.success.clone())
        } else {
            None
        },
    })
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connections::recipes::find_recipe;

    #[test]
    fn env_mapping_from_fields() {
        let recipe = find_recipe("github").unwrap();
        let mut values = HashMap::new();
        values.insert(
            "personal_access_token".to_string(),
            "ghp_test123".to_string(),
        );

        // Build env overrides the same way connect_recipe does
        let mut env_overrides = HashMap::new();
        for field in &recipe.fields {
            if let Some(value) = values.get(&field.id) {
                if !value.trim().is_empty() {
                    env_overrides.insert(field.env_key.clone(), value.clone());
                }
            }
        }

        assert_eq!(
            env_overrides.get("GITHUB_PERSONAL_ACCESS_TOKEN"),
            Some(&"ghp_test123".to_string())
        );
    }
}
