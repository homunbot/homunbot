//! Discovery tools for the cognition phase.
//!
//! Five read-only functions that the cognition mini-agent calls to find
//! relevant resources. They reuse existing registries and search infrastructure
//! — no new indexes or data structures needed.

use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::provider::{FunctionDefinition, ToolDefinition};
use crate::skills::loader::SkillRegistry;
use crate::tools::ToolRegistry;

/// Result entry from `discover_tools`.
#[derive(Debug, Serialize)]
pub(super) struct ToolEntry {
    pub name: String,
    pub description: String,
}

/// Result entry from `discover_skills`.
#[derive(Debug, Serialize)]
pub(super) struct SkillEntry {
    pub name: String,
    pub description: String,
}

/// Result entry from `discover_mcp`.
#[derive(Debug, Serialize)]
pub(super) struct McpEntry {
    pub name: String,
    pub connected: bool,
    pub tools: Vec<String>,
    pub description: String,
}

/// Result entry from `search_memory`.
#[derive(Debug, Serialize)]
pub(super) struct MemoryEntry {
    pub content: String,
    pub date: String,
    pub memory_type: String,
    pub score: f64,
}

/// Result entry from `search_knowledge`.
#[derive(Debug, Serialize)]
pub(super) struct KnowledgeEntry {
    pub content: String,
    pub source_file: String,
    pub score: f64,
}

// ── discover_tools ──────────────────────────────────────────────────

/// Search for relevant tools by natural-language query.
///
/// Uses the tool registry's names + descriptions and performs simple
/// substring matching on the query. Returns top matches.
pub(super) async fn discover_tools(
    query: &str,
    tool_registry: &RwLock<ToolRegistry>,
) -> String {
    let registry = tool_registry.read().await;
    let all_tools = registry.names_with_descriptions();
    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();

    let mut scored: Vec<(i32, ToolEntry)> = all_tools
        .into_iter()
        .filter_map(|(name, desc)| {
            let name_lower = name.to_lowercase();
            let desc_lower = desc.to_lowercase();
            let searchable = format!("{} {}", name_lower, desc_lower);

            let mut score: i32 = 0;
            // Exact name match
            if name_lower == query_lower {
                score += 10;
            }
            // Name contains query
            if name_lower.contains(&query_lower) || query_lower.contains(&name_lower) {
                score += 5;
            }
            // Word-level matching
            for word in &query_words {
                if word.len() >= 3 && searchable.contains(word) {
                    score += 2;
                }
            }

            if score > 0 {
                Some((score, ToolEntry {
                    name: name.to_string(),
                    description: desc.to_string(),
                }))
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.truncate(7);

    if scored.is_empty() {
        // No matches — return full list so the LLM can pick
        let all: Vec<ToolEntry> = tool_registry
            .read()
            .await
            .names_with_descriptions()
            .into_iter()
            .map(|(n, d)| ToolEntry {
                name: n.to_string(),
                description: d.to_string(),
            })
            .collect();
        serde_json::to_string_pretty(&all).unwrap_or_else(|_| "[]".to_string())
    } else {
        let results: Vec<&ToolEntry> = scored.iter().map(|(_, e)| e).collect();
        serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string())
    }
}

// ── discover_skills ─────────────────────────────────────────────────

/// Search for relevant skills by natural-language query.
///
/// When `active_profile_slug` is provided, per-profile skills are filtered:
/// global skills (profile_slug=None) are always included, per-profile skills
/// are only included if they belong to the active profile.
pub(super) async fn discover_skills(
    query: &str,
    skill_registry: Option<&RwLock<SkillRegistry>>,
    active_profile_slug: Option<&str>,
) -> String {
    let Some(registry) = skill_registry else {
        return "[]".to_string();
    };

    let guard = registry.read().await;
    let all_skills = guard.list_for_model();
    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();

    // Collect profile-scoped skills for visibility filtering
    let profile_map = guard.list_profile_scopes();

    let mut scored: Vec<(i32, SkillEntry)> = all_skills
        .into_iter()
        .filter(|(name, _)| {
            let Some(profile) = active_profile_slug else {
                return true; // no profile filtering
            };
            match profile_map.get(*name) {
                None => true,                  // global skill — always visible
                Some(slug) => *slug == profile, // per-profile — visible only if match
            }
        })
        .filter_map(|(name, desc)| {
            let name_lower = name.to_lowercase();
            let desc_lower = desc.to_lowercase();
            let searchable = format!("{} {}", name_lower, desc_lower);

            let mut score: i32 = 0;
            if name_lower.contains(&query_lower) || query_lower.contains(&name_lower) {
                score += 5;
            }
            for word in &query_words {
                if word.len() >= 3 && searchable.contains(word) {
                    score += 2;
                }
            }

            if score > 0 {
                Some((score, SkillEntry {
                    name: name.to_string(),
                    description: desc.to_string(),
                }))
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.truncate(5);

    let results: Vec<&SkillEntry> = scored.iter().map(|(_, e)| e).collect();
    serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string())
}

// ── discover_mcp ────────────────────────────────────────────────────

/// Search for relevant MCP services — both connected servers and available recipes.
pub(super) async fn discover_mcp(
    query: &str,
    config: &Config,
    tool_registry: &RwLock<ToolRegistry>,
) -> String {
    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();
    let mut results: Vec<McpEntry> = Vec::new();

    // 1. Check connected MCP servers from config
    for (name, server) in &config.mcp.servers {
        if !server.enabled {
            continue;
        }
        let name_lower = name.to_lowercase();
        let searchable = format!(
            "{} {} {}",
            name_lower,
            server.command.as_deref().unwrap_or(""),
            server.args.join(" ")
        )
        .to_lowercase();

        let matches = searchable.contains(&query_lower)
            || query_words
                .iter()
                .any(|w| w.len() >= 3 && searchable.contains(w));

        if matches {
            // Find MCP tool names that belong to this server
            let registry = tool_registry.read().await;
            let mcp_tools: Vec<String> = registry
                .names()
                .into_iter()
                .filter(|t| t.starts_with(&format!("mcp_{}_", name)) || t.contains(name))
                .map(|s| s.to_string())
                .collect();

            results.push(McpEntry {
                name: name.clone(),
                connected: true,
                tools: mcp_tools,
                description: format!("Connected MCP server: {}", name),
            });
        }
    }

    // 2. Check available (not yet connected) MCP recipes
    let presets = crate::skills::mcp_registry::all_mcp_presets();
    for preset in presets {
        // Skip if already connected
        if results.iter().any(|r| r.name == preset.id) {
            continue;
        }
        if config.mcp.servers.contains_key(&preset.id) {
            continue;
        }

        let searchable = format!(
            "{} {} {} {}",
            preset.id,
            preset.display_name,
            preset.description,
            preset.keywords.join(" ")
        )
        .to_lowercase();

        let matches = searchable.contains(&query_lower)
            || preset
                .aliases
                .iter()
                .any(|a| query_lower.contains(&a.to_lowercase()))
            || query_words
                .iter()
                .any(|w| w.len() >= 3 && searchable.contains(w));

        if matches {
            results.push(McpEntry {
                name: preset.id.clone(),
                connected: false,
                tools: Vec::new(),
                description: preset.description.clone(),
            });
        }
    }

    results.truncate(5);
    serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string())
}

// ── search_memory ───────────────────────────────────────────────────

/// Search long-term memory with a targeted query.
///
/// Wraps the existing hybrid vector + FTS5 search, scoped by contact and agent.
#[cfg(feature = "embeddings")]
pub(super) async fn search_memory(
    query: &str,
    searcher: &Arc<tokio::sync::Mutex<crate::agent::memory_search::MemorySearcher>>,
    contact_id: Option<i64>,
    agent_id: Option<&str>,
    profile_ids: &[i64],
) -> String {
    let mut guard = searcher.lock().await;
    match guard.search_scoped_full(query, 3, contact_id, agent_id, profile_ids).await {
        Ok(results) if !results.is_empty() => {
            let entries: Vec<MemoryEntry> = results
                .iter()
                .map(|r| MemoryEntry {
                    content: r.chunk.content.clone(),
                    date: r.chunk.date.clone(),
                    memory_type: r.chunk.memory_type.clone(),
                    score: r.score,
                })
                .collect();
            serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string())
        }
        Ok(_) => "[]".to_string(),
        Err(e) => {
            tracing::warn!(error = %e, "Cognition memory search failed");
            "[]".to_string()
        }
    }
}

/// Stub for when embeddings feature is disabled.
#[cfg(not(feature = "embeddings"))]
pub(super) async fn search_memory(
    _query: &str,
    _contact_id: Option<i64>,
    _agent_id: Option<&str>,
    _profile_ids: &[i64],
) -> String {
    "[]".to_string()
}

// ── search_knowledge ────────────────────────────────────────────────

/// Search the RAG knowledge base with a targeted query.
#[cfg(feature = "embeddings")]
pub(super) async fn search_knowledge(
    query: &str,
    rag_engine: &Arc<tokio::sync::Mutex<crate::rag::RagEngine>>,
) -> String {
    let mut guard = rag_engine.lock().await;
    match guard.search(query, 3, None).await {
        Ok(results) if !results.is_empty() => {
            let entries: Vec<KnowledgeEntry> = results
                .iter()
                .map(|r| KnowledgeEntry {
                    content: r.chunk.content.clone(),
                    source_file: r.source_file.clone(),
                    score: r.score,
                })
                .collect();
            serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string())
        }
        Ok(_) => "[]".to_string(),
        Err(e) => {
            tracing::warn!(error = %e, "Cognition RAG search failed");
            "[]".to_string()
        }
    }
}

/// Stub for when embeddings feature is disabled.
#[cfg(not(feature = "embeddings"))]
pub(super) async fn search_knowledge(_query: &str) -> String {
    "[]".to_string()
}

// ── Tool definitions for the cognition mini-loop ────────────────────

/// Build the tool definitions that the cognition agent can call.
///
/// These are the 5 discovery tools + the `plan_execution` output tool.
pub(super) fn cognition_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "discover_tools".to_string(),
                description: "Search for available tools by describing what you need to do. \
                    Returns matching tools with their names and capabilities."
                    .to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Natural-language description of the capability needed (e.g. 'search the web', 'send a message', 'automate a browser')"
                        }
                    },
                    "required": ["query"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "discover_skills".to_string(),
                description: "Search for installed skills (specialized capabilities) by description."
                    .to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Description of the skill needed (e.g. 'generate a PDF', 'code review')"
                        }
                    },
                    "required": ["query"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "discover_mcp".to_string(),
                description: "Search for external services (MCP integrations) — calendar, email, GitHub, etc. \
                    Shows both connected and available-to-install services."
                    .to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The external service or capability needed (e.g. 'calendar', 'github issues', 'email')"
                        }
                    },
                    "required": ["query"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "search_memory".to_string(),
                description: "Search the user's long-term memory for relevant past context. \
                    Use targeted queries about specific topics the user might have mentioned before."
                    .to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Specific memory query (e.g. 'train preferences', 'work schedule', 'favorite restaurants')"
                        }
                    },
                    "required": ["query"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "search_knowledge".to_string(),
                description: "Search the user's personal knowledge base (documents, notes, files). \
                    Use when the answer might be in the user's own documents."
                    .to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Knowledge base query (e.g. 'project roadmap', 'meeting notes', 'API documentation')"
                        }
                    },
                    "required": ["query"]
                }),
            },
        },
        // The output tool — forces structured JSON output
        super::types::plan_execution_tool_definition(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cognition_tool_definitions_count() {
        let defs = cognition_tool_definitions();
        // 5 discovery tools + 1 plan_execution
        assert_eq!(defs.len(), 6);
        assert_eq!(defs[0].function.name, "discover_tools");
        assert_eq!(defs[5].function.name, "plan_execution");
    }

    #[test]
    fn test_cognition_tool_definitions_have_required_query() {
        let defs = cognition_tool_definitions();
        for def in &defs[..5] {
            let required = def.function.parameters
                .get("required")
                .and_then(|r| r.as_array())
                .expect("Should have required array");
            assert!(
                required.iter().any(|v| v.as_str() == Some("query")),
                "Tool {} should require 'query' parameter",
                def.function.name
            );
        }
    }
}
