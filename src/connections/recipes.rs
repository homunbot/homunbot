//! Recipe loader — parse bundled + user TOML recipes.

use std::collections::HashMap;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::skills::McpEnvVar;
use crate::skills::McpServerPreset;

use super::{ConnectionRecipe, ConnectionStatus, RecipeMcpConfig};

// ── Bundled recipes (compiled into the binary) ───────────────────────

const BUNDLED_RECIPES: &[(&str, &str)] = &[
    ("github", include_str!("../../recipes/github.toml")),
    (
        "google-workspace",
        include_str!("../../recipes/google-workspace.toml"),
    ),
    ("notion", include_str!("../../recipes/notion.toml")),
    ("slack", include_str!("../../recipes/slack.toml")),
];

/// Load all connection recipes: bundled + user-created (~/.homun/recipes/).
///
/// User recipes with the same `id` as a bundled recipe override it.
pub fn load_all_recipes() -> Vec<ConnectionRecipe> {
    let mut by_id: HashMap<String, ConnectionRecipe> = HashMap::new();

    // 1. Bundled recipes (compiled in)
    for (name, toml_src) in BUNDLED_RECIPES {
        match toml::from_str::<ConnectionRecipe>(toml_src) {
            Ok(recipe) => {
                by_id.insert(recipe.id.clone(), recipe);
            }
            Err(e) => {
                tracing::warn!(name, error = %e, "Failed to parse bundled recipe");
            }
        }
    }

    // 2. User recipes (~/.homun/recipes/*.toml)
    let user_dir = Config::data_dir().join("recipes");
    if user_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&user_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "toml") {
                    match std::fs::read_to_string(&path) {
                        Ok(content) => match toml::from_str::<ConnectionRecipe>(&content) {
                            Ok(recipe) => {
                                by_id.insert(recipe.id.clone(), recipe);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    path = %path.display(),
                                    error = %e,
                                    "Failed to parse user recipe"
                                );
                            }
                        },
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                error = %e,
                                "Failed to read user recipe file"
                            );
                        }
                    }
                }
            }
        }
    }

    // Sort by display_name for consistent ordering
    let mut recipes: Vec<ConnectionRecipe> = by_id.into_values().collect();
    recipes.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    recipes
}

/// Find a single recipe by id.
pub fn find_recipe(id: &str) -> Option<ConnectionRecipe> {
    load_all_recipes().into_iter().find(|r| r.id == id)
}

/// Find all MCP server instances derived from a recipe.
///
/// Matches by `McpServerConfig::recipe_id` (explicit) or by server name
/// equalling `recipe.id` (legacy configs without `recipe_id`).
pub fn recipe_instances(
    recipe: &ConnectionRecipe,
    config: &Config,
) -> Vec<super::ConnectionInstance> {
    config
        .mcp
        .servers
        .iter()
        .filter(|(name, server)| {
            server.recipe_id.as_deref() == Some(recipe.id.as_str())
                || (server.recipe_id.is_none() && **name == recipe.id)
        })
        .map(|(name, server)| super::ConnectionInstance {
            name: name.clone(),
            tool_count: server.discovered_tool_count.unwrap_or(0),
            enabled: server.enabled,
        })
        .collect()
}

/// Aggregate connection status across all instances of a recipe.
pub fn recipe_connection_status(recipe: &ConnectionRecipe, config: &Config) -> ConnectionStatus {
    let instances = recipe_instances(recipe, config);
    let active: Vec<_> = instances.iter().filter(|i| i.enabled).collect();
    if active.is_empty() {
        ConnectionStatus::NotConnected
    } else {
        ConnectionStatus::Connected {
            tool_count: active.iter().map(|i| i.tool_count).sum(),
        }
    }
}

/// Convert a recipe into an `McpServerPreset` for use with
/// [`crate::mcp_setup::apply_mcp_preset_setup`].
///
/// `instance_name` determines the vault key namespace — each instance gets
/// isolated secrets (e.g. `mcp.gmail-work.client_id`).
pub fn recipe_to_preset(recipe: &ConnectionRecipe, instance_name: &str) -> McpServerPreset {
    let env = recipe
        .fields
        .iter()
        .filter(|f| !f.env_key.is_empty())
        .map(|f| McpEnvVar {
            key: f.env_key.clone(),
            description: f.help.clone(),
            required: f.required,
            secret: f.secret,
            vault_key: format!("mcp.{}.{}", instance_name, f.id),
        })
        .collect();

    McpServerPreset {
        id: recipe.id.clone(),
        display_name: recipe.display_name.clone(),
        description: recipe.subtitle.clone(),
        command: recipe.mcp.command.clone(),
        args: recipe.mcp.args.clone(),
        env,
        docs_url: None,
        aliases: vec![],
        keywords: vec![],
        transport: recipe.mcp.transport.clone(),
        url: recipe.mcp.url.clone(),
        auth_env_key: recipe.mcp.auth_env_key.clone(),
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_recipes_parse_successfully() {
        let recipes = load_all_recipes();
        assert!(
            recipes.len() >= 4,
            "Expected at least 4 bundled recipes, got {}",
            recipes.len()
        );

        // Verify all required ids exist
        let ids: Vec<&str> = recipes.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"github"), "Missing github recipe");
        assert!(
            ids.contains(&"google-workspace"),
            "Missing google-workspace"
        );
        assert!(ids.contains(&"notion"), "Missing notion recipe");
        assert!(ids.contains(&"slack"), "Missing slack recipe");
    }

    #[test]
    fn recipe_fields_have_env_keys() {
        let recipes = load_all_recipes();
        for recipe in &recipes {
            for field in &recipe.fields {
                assert!(
                    !field.env_key.is_empty(),
                    "Field '{}' in recipe '{}' has empty env_key",
                    field.id,
                    recipe.id
                );
            }
        }
    }

    #[test]
    fn recipe_to_preset_conversion() {
        let recipe = find_recipe("github").expect("github recipe should exist");
        let preset = recipe_to_preset(&recipe, "github");
        assert_eq!(preset.id, "github");
        assert_eq!(preset.command, "npx");
        assert!(!preset.env.is_empty());
        assert_eq!(preset.env[0].key, "GITHUB_PERSONAL_ACCESS_TOKEN");
        assert!(preset.env[0].secret);
        assert_eq!(preset.env[0].vault_key, "mcp.github.personal_access_token");
    }

    #[test]
    fn recipe_to_preset_custom_instance_name() {
        let recipe = find_recipe("github").expect("github recipe should exist");
        let preset = recipe_to_preset(&recipe, "github-work");
        // Vault keys use instance name, not recipe id
        assert_eq!(
            preset.env[0].vault_key,
            "mcp.github-work.personal_access_token"
        );
    }

    #[test]
    fn find_recipe_by_id() {
        assert!(find_recipe("github").is_some());
        assert!(find_recipe("nonexistent").is_none());
    }

    #[test]
    fn connection_status_not_connected() {
        let recipe = find_recipe("github").unwrap();
        let config = Config::default();
        let status = recipe_connection_status(&recipe, &config);
        assert!(matches!(status, ConnectionStatus::NotConnected));
    }

    #[test]
    fn recipe_instances_finds_by_recipe_id() {
        let recipe = find_recipe("google-workspace").unwrap();
        let mut config = Config::default();

        // Add two instances with explicit recipe_id
        let mut s1 = crate::config::McpServerConfig::default();
        s1.recipe_id = Some("google-workspace".to_string());
        config
            .mcp
            .servers
            .insert("google-workspace".to_string(), s1);

        let mut s2 = crate::config::McpServerConfig::default();
        s2.recipe_id = Some("google-workspace".to_string());
        config.mcp.servers.insert("google-work".to_string(), s2);

        let instances = recipe_instances(&recipe, &config);
        assert_eq!(instances.len(), 2);
    }

    #[test]
    fn recipe_instances_legacy_name_match() {
        let recipe = find_recipe("github").unwrap();
        let mut config = Config::default();

        // Legacy config: no recipe_id, server name = recipe id
        let s = crate::config::McpServerConfig::default();
        config.mcp.servers.insert("github".to_string(), s);

        let instances = recipe_instances(&recipe, &config);
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].name, "github");
    }
}
