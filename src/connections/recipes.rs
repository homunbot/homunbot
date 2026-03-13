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
    ("gmail", include_str!("../../recipes/gmail.toml")),
    (
        "google-calendar",
        include_str!("../../recipes/google-calendar.toml"),
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

/// Check whether a recipe's service is already connected.
///
/// A recipe is "connected" if `config.mcp.servers` contains a server
/// with the same key as the recipe `id`.
pub fn recipe_connection_status(recipe: &ConnectionRecipe, config: &Config) -> ConnectionStatus {
    match config.mcp.servers.get(&recipe.id) {
        Some(server) if server.enabled => {
            // We can't know tool_count without starting the server,
            // so report 0 here; the API can enrich it on demand.
            ConnectionStatus::Connected {
                tool_count: server.capabilities.len(),
            }
        }
        Some(_) => ConnectionStatus::Error {
            message: "Server is disabled".to_string(),
        },
        None => ConnectionStatus::NotConnected,
    }
}

/// Convert a recipe into an `McpServerPreset` for use with
/// [`crate::mcp_setup::apply_mcp_preset_setup`].
pub fn recipe_to_preset(recipe: &ConnectionRecipe) -> McpServerPreset {
    let env = recipe
        .fields
        .iter()
        .filter(|f| !f.env_key.is_empty())
        .map(|f| McpEnvVar {
            key: f.env_key.clone(),
            description: f.help.clone(),
            required: f.required,
            secret: f.secret,
            vault_key: format!("mcp.{}.{}", recipe.id, f.id),
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
            recipes.len() >= 5,
            "Expected at least 5 bundled recipes, got {}",
            recipes.len()
        );

        // Verify all required ids exist
        let ids: Vec<&str> = recipes.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"github"), "Missing github recipe");
        assert!(ids.contains(&"gmail"), "Missing gmail recipe");
        assert!(ids.contains(&"google-calendar"), "Missing google-calendar");
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
        let preset = recipe_to_preset(&recipe);
        assert_eq!(preset.id, "github");
        assert_eq!(preset.command, "npx");
        assert!(!preset.env.is_empty());
        assert_eq!(preset.env[0].key, "GITHUB_PERSONAL_ACCESS_TOKEN");
        assert!(preset.env[0].secret);
        assert_eq!(preset.env[0].vault_key, "mcp.github.personal_access_token");
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
}
