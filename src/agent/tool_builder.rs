//! Tool definition assembly for the agent loop.
//!
//! Builds the list of tool definitions (built-in tools + skills) that
//! are sent to the LLM, applying blocked/allowed filters and browser
//! routing priority.

use std::collections::HashSet;

use tokio::sync::RwLock;

use crate::provider::{FunctionDefinition, ToolDefinition};
use crate::skills::loader::SkillRegistry;
use crate::tools::ToolRegistry;

use super::browser_task_plan::BrowserRoutingDecision;
use super::ToolInfo;

/// Assembled tool definitions ready for LLM consumption.
pub(crate) struct ToolDefinitionSet {
    /// Tool definitions in LLM format (for native function calling).
    pub defs: Vec<ToolDefinition>,
    /// Tool info structs (for XML dispatch mode prompt injection).
    pub tool_infos: Vec<ToolInfo>,
    /// Set of available tool names (for quick lookup).
    pub available_names: HashSet<String>,
    /// Whether any tools are available.
    pub has_tools: bool,
}

/// Build the complete set of tool definitions for an LLM request.
///
/// Merges built-in tools from the registry with installed skills,
/// then applies blocked/allowed filters and browser priority sorting.
pub(crate) async fn build_tool_definitions(
    tool_registry: &RwLock<ToolRegistry>,
    skill_registry: Option<&RwLock<SkillRegistry>>,
    blocked_tools: &HashSet<&str>,
    allowed_tools: &[String],
    allowed_skills: &[String],
    browser_routing: &BrowserRoutingDecision,
    xml_mode: bool,
) -> ToolDefinitionSet {
    let mut tool_defs = tool_registry.read().await.get_definitions();

    // Register installed skills as tool definitions so the LLM can call them.
    // Each skill becomes a callable tool with a `query` parameter.
    // Only model-invocable skills are registered (ineligible or disable-model-invocation
    // skills are hidden from the LLM).
    if let Some(registry) = skill_registry {
        let guard = registry.read().await;
        for (name, desc) in guard.list_for_model() {
            // Per-agent skill allowlist: skip skills not in the list
            if !allowed_skills.is_empty() && !allowed_skills.iter().any(|s| s == name) {
                continue;
            }
            tool_defs.push(ToolDefinition {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: name.to_string(),
                    description: format!(
                        "[Skill] {}. Call this tool to activate the skill.",
                        desc
                    ),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "query": {
                                "type": "string",
                                "description": "The user's request or query for this skill"
                            }
                        },
                        "required": ["query"]
                    }),
                },
            });
        }
    }

    if !blocked_tools.is_empty() {
        tool_defs.retain(|td| !blocked_tools.contains(td.function.name.as_str()));
    }

    // Per-agent tool allowlist (from AgentDefinition).
    if !allowed_tools.is_empty() {
        tool_defs.retain(|td| allowed_tools.iter().any(|a| a == &td.function.name));
    }

    if browser_routing.browser_required() {
        tool_defs.sort_by_key(|tool| {
            if crate::browser::is_browser_tool(&tool.function.name) {
                0
            } else if tool.function.name == "web_search" {
                1
            } else if tool.function.name == "web_fetch" {
                2
            } else {
                3
            }
        });
    }

    let has_tools = !tool_defs.is_empty();

    // Convert tool definitions to ToolInfo for the XML prompt system
    let tool_infos: Vec<ToolInfo> = if xml_mode && has_tools {
        tool_defs
            .iter()
            .map(|td| ToolInfo {
                name: td.function.name.clone(),
                description: td.function.description.clone(),
                parameters_schema: td.function.parameters.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };

    let available_names = tool_defs
        .iter()
        .map(|tool| tool.function.name.clone())
        .collect::<HashSet<_>>();

    ToolDefinitionSet {
        defs: tool_defs,
        tool_infos,
        available_names,
        has_tools,
    }
}
