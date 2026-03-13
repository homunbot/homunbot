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
/// This orchestrates:
/// 1. Building env overrides from field values (field.env_key → value)
/// 2. Converting the recipe to an `McpServerPreset`
/// 3. Calling `apply_mcp_preset_setup` to store secrets and configure the server
/// 4. Optionally testing the connection
pub async fn connect_recipe(
    config: &mut Config,
    recipe: &ConnectionRecipe,
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

    // 2. Convert recipe → preset
    let preset = recipe_to_preset(recipe);

    // 3. Apply setup (stores secrets in vault, writes config)
    let setup_result = mcp_setup::apply_mcp_preset_setup(
        config,
        &preset,
        &recipe.id,
        &env_overrides,
        true, // overwrite existing
    )?;

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
    let (connected, tool_count) = if skip_test {
        (None, 0)
    } else {
        let server = config
            .mcp
            .servers
            .get(&recipe.id)
            .cloned()
            .expect("server should exist after setup");

        let test = mcp_setup::test_mcp_server_connection(&recipe.id, &server, sandbox).await;
        (Some(test.connected), test.tool_count)
    };

    let ok = connected.unwrap_or(true);
    let message = if ok {
        recipe.success.title.clone()
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
