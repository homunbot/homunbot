//! Types for the cognition phase.
//!
//! `CognitionResult` is the contract between the cognition mini-agent
//! and the main execution loop. It describes what the user wants,
//! which resources are needed, and a suggested plan of action.

use serde::{Deserialize, Serialize};

use crate::provider::{FunctionDefinition, ToolDefinition};

/// Task complexity classification — drives iteration budgets and prompt depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Complexity {
    /// Greetings, time queries, simple factual Q&A — no tools needed.
    Simple,
    /// Single-tool tasks (remember, search, send message).
    Standard,
    /// Multi-step tasks (browser automation, workflows, research).
    Complex,
}

/// Autonomy level override detected from the user's prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Autonomy {
    /// Execute without asking for confirmation.
    Automatic,
    /// Ask before taking actions with side effects.
    Assisted,
}

/// A tool discovered by the cognition phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredTool {
    /// Tool name (must match a registered tool in the ToolRegistry).
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Why the cognition selected this tool for the current request.
    pub reason: String,
}

/// A skill discovered by the cognition phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSkill {
    /// Skill name (must match an installed skill).
    pub name: String,
    /// Human-readable description.
    pub description: String,
}

/// An MCP service discovered by the cognition phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredMcp {
    /// MCP server name or recipe ID.
    pub name: String,
    /// Whether this MCP server is already connected and active.
    pub connected: bool,
    /// Tool names exposed by this MCP server (if connected).
    pub tools: Vec<String>,
}

/// Output of the cognition phase — drives context assembly and execution.
///
/// Produced by the cognition mini-agent calling the `plan_execution` tool.
/// All fields are populated by the LLM through discovery tool calls,
/// then validated programmatically before use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitionResult {
    /// Natural-language summary of what the user wants.
    pub understanding: String,

    /// Task complexity classification.
    pub complexity: Complexity,

    /// If true, the cognition can answer directly without the execution loop.
    #[serde(default)]
    pub answer_directly: bool,

    /// The direct answer (only used when `answer_directly` is true).
    #[serde(default)]
    pub direct_answer: Option<String>,

    /// Tools needed for execution (discovered via `discover_tools`).
    #[serde(default)]
    pub tools: Vec<DiscoveredTool>,

    /// Skills needed for execution (discovered via `discover_skills`).
    #[serde(default)]
    pub skills: Vec<DiscoveredSkill>,

    /// MCP services relevant to the task (discovered via `discover_mcp`).
    #[serde(default)]
    pub mcp_tools: Vec<DiscoveredMcp>,

    /// Relevant memories retrieved by the cognition (via `search_memory`).
    #[serde(default)]
    pub memory_context: Option<String>,

    /// Relevant knowledge base content (via `search_knowledge`).
    #[serde(default)]
    pub rag_context: Option<String>,

    /// Step-by-step plan for the execution phase.
    #[serde(default)]
    pub plan: Vec<String>,

    /// Constraints extracted from the user's request (time, price, quantity...).
    #[serde(default)]
    pub constraints: Vec<String>,

    /// Autonomy override detected from the user's prompt language.
    #[serde(default)]
    pub autonomy_override: Option<Autonomy>,
}

impl CognitionResult {
    /// Build a minimal direct-answer result for simple requests.
    pub fn direct(answer: &str) -> Self {
        Self {
            understanding: answer.to_string(),
            complexity: Complexity::Simple,
            answer_directly: true,
            direct_answer: Some(answer.to_string()),
            tools: Vec::new(),
            skills: Vec::new(),
            mcp_tools: Vec::new(),
            memory_context: None,
            rag_context: None,
            plan: Vec::new(),
            constraints: Vec::new(),
            autonomy_override: None,
        }
    }

    /// Build a full-context fallback when cognition LLM call fails.
    ///
    /// Returns ALL tools from the registry so the execution loop has
    /// maximum capabilities. Used only when `run_cognition()` errors.
    pub fn fallback_full(all_tool_names: Vec<String>) -> Self {
        let tools = all_tool_names
            .into_iter()
            .map(|name| DiscoveredTool {
                description: String::new(),
                reason: "Cognition unavailable — full tool set provided".to_string(),
                name,
            })
            .collect();

        Self {
            understanding: "Cognition unavailable, providing full context".to_string(),
            complexity: Complexity::Complex,
            answer_directly: false,
            direct_answer: None,
            tools,
            skills: Vec::new(),
            mcp_tools: Vec::new(),
            memory_context: None,
            rag_context: None,
            plan: Vec::new(),
            constraints: Vec::new(),
            autonomy_override: None,
        }
    }
}

/// JSON Schema for the `plan_execution` tool parameter.
///
/// This schema forces the LLM to produce a well-structured `CognitionResult`
/// via tool calling, instead of generating free-form text.
pub fn plan_execution_tool_definition() -> ToolDefinition {
    ToolDefinition {
        tool_type: "function".to_string(),
        function: FunctionDefinition {
            name: "plan_execution".to_string(),
            description: "Submit your analysis of the user's request. \
                Call this once you have understood the intent and discovered the needed resources."
                .to_string(),
            parameters: plan_execution_schema(),
        },
    }
}

/// JSON Schema for CognitionResult as a tool parameter.
fn plan_execution_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "understanding": {
                "type": "string",
                "description": "Natural-language summary of the user's intent"
            },
            "complexity": {
                "type": "string",
                "enum": ["simple", "standard", "complex"],
                "description": "Task complexity: simple (no tools), standard (1-2 tools), complex (multi-step)"
            },
            "answer_directly": {
                "type": "boolean",
                "description": "True if you can answer without any tool execution (greetings, time, simple facts)"
            },
            "direct_answer": {
                "type": "string",
                "description": "The direct answer (only when answer_directly is true)"
            },
            "tools": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Exact tool name from discover_tools results" },
                        "description": { "type": "string" },
                        "reason": { "type": "string", "description": "Why this tool is needed" }
                    },
                    "required": ["name", "description", "reason"]
                },
                "description": "Tools needed for this task (from discover_tools results)"
            },
            "skills": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "description": { "type": "string" }
                    },
                    "required": ["name", "description"]
                },
                "description": "Skills needed (from discover_skills results)"
            },
            "mcp_tools": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "connected": { "type": "boolean" },
                        "tools": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["name", "connected"]
                },
                "description": "MCP services relevant to this task"
            },
            "memory_context": {
                "type": "string",
                "description": "Relevant memories found via search_memory (null if not searched)"
            },
            "rag_context": {
                "type": "string",
                "description": "Relevant knowledge found via search_knowledge (null if not searched)"
            },
            "plan": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Step-by-step plan for the execution phase"
            },
            "constraints": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Constraints from the user's request (time, price, count...)"
            },
            "autonomy_override": {
                "type": "string",
                "enum": ["automatic", "assisted"],
                "description": "Autonomy level if the user explicitly asked for one"
            }
        },
        "required": ["understanding", "complexity", "answer_directly"]
    })
}

/// Validation errors found in a CognitionResult.
#[derive(Debug)]
pub struct ValidationIssue {
    pub field: String,
    pub message: String,
}

/// Validate a CognitionResult against the actual registries.
///
/// Returns a list of issues (empty = valid). Checks:
/// - All tool names exist in the registry
/// - All skill names exist
/// - Logical consistency (no tools but not answer_directly)
pub fn validate_cognition_result(
    result: &CognitionResult,
    known_tools: &[String],
    known_skills: &[String],
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    for tool in &result.tools {
        if !known_tools.iter().any(|t| t == &tool.name) {
            issues.push(ValidationIssue {
                field: "tools".to_string(),
                message: format!("Tool '{}' does not exist. Available: {}", tool.name, known_tools.join(", ")),
            });
        }
    }

    for skill in &result.skills {
        if !known_skills.iter().any(|s| s == &skill.name) {
            issues.push(ValidationIssue {
                field: "skills".to_string(),
                message: format!("Skill '{}' not found", skill.name),
            });
        }
    }

    if !result.answer_directly && result.tools.is_empty() && result.skills.is_empty() {
        // Not answering directly but no tools/skills selected — might be okay
        // (e.g. the LLM can answer from context), but worth flagging.
        // Don't add as hard error — the execution loop can still work with zero tools.
    }

    if result.answer_directly && result.direct_answer.is_none() {
        issues.push(ValidationIssue {
            field: "direct_answer".to_string(),
            message: "answer_directly is true but no direct_answer provided".to_string(),
        });
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cognition_result_direct() {
        let result = CognitionResult::direct("Sono le 14:32");
        assert!(result.answer_directly);
        assert_eq!(result.direct_answer.as_deref(), Some("Sono le 14:32"));
        assert_eq!(result.complexity, Complexity::Simple);
        assert!(result.tools.is_empty());
    }

    #[test]
    fn test_cognition_result_serialization() {
        let result = CognitionResult {
            understanding: "User wants to search for trains".to_string(),
            complexity: Complexity::Complex,
            answer_directly: false,
            direct_answer: None,
            tools: vec![DiscoveredTool {
                name: "web_search".to_string(),
                description: "Search the web".to_string(),
                reason: "Need to find train schedules".to_string(),
            }],
            skills: Vec::new(),
            mcp_tools: Vec::new(),
            memory_context: Some("User prefers Frecciarossa".to_string()),
            rag_context: None,
            plan: vec!["Search Trenitalia".to_string(), "Compare prices".to_string()],
            constraints: vec!["Tomorrow morning".to_string()],
            autonomy_override: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: CognitionResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.understanding, result.understanding);
        assert_eq!(parsed.complexity, Complexity::Complex);
        assert_eq!(parsed.tools.len(), 1);
        assert_eq!(parsed.plan.len(), 2);
    }

    #[test]
    fn test_validation_unknown_tool() {
        let result = CognitionResult {
            understanding: "test".to_string(),
            complexity: Complexity::Standard,
            answer_directly: false,
            direct_answer: None,
            tools: vec![DiscoveredTool {
                name: "nonexistent_tool".to_string(),
                description: "...".to_string(),
                reason: "...".to_string(),
            }],
            skills: Vec::new(),
            mcp_tools: Vec::new(),
            memory_context: None,
            rag_context: None,
            plan: Vec::new(),
            constraints: Vec::new(),
            autonomy_override: None,
        };

        let issues = validate_cognition_result(
            &result,
            &["web_search".to_string(), "browser".to_string()],
            &[],
        );
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("nonexistent_tool"));
    }

    #[test]
    fn test_validation_direct_without_answer() {
        let mut result = CognitionResult::direct("test");
        result.direct_answer = None;

        let issues = validate_cognition_result(&result, &[], &[]);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("direct_answer"));
    }

    #[test]
    fn test_validation_valid_result() {
        let result = CognitionResult {
            understanding: "Send a message".to_string(),
            complexity: Complexity::Standard,
            answer_directly: false,
            direct_answer: None,
            tools: vec![DiscoveredTool {
                name: "send_message".to_string(),
                description: "Send message".to_string(),
                reason: "User wants to send a message".to_string(),
            }],
            skills: Vec::new(),
            mcp_tools: Vec::new(),
            memory_context: None,
            rag_context: None,
            plan: vec!["Send the message via WhatsApp".to_string()],
            constraints: Vec::new(),
            autonomy_override: None,
        };

        let issues = validate_cognition_result(
            &result,
            &["send_message".to_string(), "web_search".to_string()],
            &[],
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_plan_execution_schema() {
        let def = plan_execution_tool_definition();
        assert_eq!(def.function.name, "plan_execution");
        let props = def.function.parameters.get("properties").unwrap();
        assert!(props.get("understanding").is_some());
        assert!(props.get("complexity").is_some());
        assert!(props.get("tools").is_some());
        assert!(props.get("plan").is_some());
    }
}
