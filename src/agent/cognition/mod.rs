//! Cognition-First agent preprocessing.
//!
//! LLM-driven intent understanding — the sole path for tool/skill/MCP
//! discovery. A mini ReAct loop with read-only discovery tools analyzes
//! the user's request, finds relevant tools/skills/MCP/memory, builds
//! a plan, and passes a lean, targeted context to the main execution loop.
//!
//! When `run_cognition()` fails (provider error, timeout), the caller
//! uses `fallback_full_context()` to provide all tools to the execution
//! loop — degraded but functional.

mod discovery;
mod engine;
mod types;

use std::collections::HashSet;

use tokio::sync::RwLock;

use crate::provider::{FunctionDefinition, ToolDefinition};
use crate::skills::loader::SkillRegistry;
use crate::tools::ToolRegistry;

pub use engine::{run_cognition, CognitionParams};
pub use types::{Autonomy, Complexity, CognitionResult, DiscoveredMcp, DiscoveredSkill, DiscoveredTool};

use super::tool_builder::ToolDefinitionSet;
use super::ToolInfo;

/// Build a selective set of tool definitions based on cognition results.
///
/// Only includes tools/skills that the cognition phase identified as relevant,
/// plus a small set of always-available tools (send_message, remember, approval).
pub(crate) async fn build_selective_tool_defs(
    tool_registry: &RwLock<ToolRegistry>,
    skill_registry: Option<&RwLock<SkillRegistry>>,
    discovered_tools: &[DiscoveredTool],
    discovered_skills: &[DiscoveredSkill],
    blocked_tools: &HashSet<&str>,
    xml_mode: bool,
) -> ToolDefinitionSet {
    let always_available = [
        "send_message",
        "remember",
        "approval",
    ];

    let selected_names: HashSet<&str> = discovered_tools
        .iter()
        .map(|t| t.name.as_str())
        .chain(always_available.iter().copied())
        .collect();

    let all_defs = tool_registry.read().await.get_definitions();
    let mut tool_defs: Vec<ToolDefinition> = all_defs
        .into_iter()
        .filter(|td| {
            selected_names.contains(td.function.name.as_str())
                && !blocked_tools.contains(td.function.name.as_str())
        })
        .collect();

    // Add discovered skills
    if let Some(registry) = skill_registry {
        let guard = registry.read().await;
        let selected_skill_names: HashSet<&str> =
            discovered_skills.iter().map(|s| s.name.as_str()).collect();
        for (name, desc) in guard.list_for_model() {
            if selected_skill_names.contains(name) && !blocked_tools.contains(name) {
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
    }

    let has_tools = !tool_defs.is_empty();

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

/// Build a full-context fallback `CognitionResult` when the cognition
/// LLM call fails (provider error, timeout, etc.).
///
/// Returns a result listing ALL tools from the registry so the execution
/// loop has maximum capabilities — degraded but functional.
pub(crate) async fn fallback_full_context(
    tool_registry: &RwLock<ToolRegistry>,
) -> CognitionResult {
    let names: Vec<String> = tool_registry
        .read()
        .await
        .names()
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    CognitionResult::fallback_full(names)
}
