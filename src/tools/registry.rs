use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::bus::OutboundMessage;
use crate::provider::{FunctionDefinition, ToolDefinition};
use crate::tools::approval::ApprovalManager;

/// Result of executing a tool — always a string for the LLM
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub output: String,
    pub is_error: bool,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            is_error: false,
        }
    }

    pub fn error(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            is_error: true,
        }
    }
}

/// Context passed to tools during execution — provides channel info, workspace, etc.
#[derive(Debug, Clone)]
pub struct ToolContext {
    pub workspace: String,
    pub channel: String,
    pub chat_id: String,
    /// Optional sender for proactive messaging (MessageTool).
    /// Set in Gateway mode where channels are available; None in CLI mode.
    pub message_tx: Option<mpsc::Sender<OutboundMessage>>,
    /// Optional approval manager for interactive approval workflow.
    pub approval_manager: Option<Arc<ApprovalManager>>,
}

/// Tool trait — every built-in tool and skill implements this.
///
/// Follows nanobot's Tool base class pattern:
/// - name, description, parameters (JSON Schema), execute
/// - Execution is async and returns ToolResult (never panics)
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique tool name (used in LLM function calling)
    fn name(&self) -> &str;

    /// Human-readable description for the LLM
    fn description(&self) -> &str;

    /// JSON Schema defining the tool's parameters
    fn parameters(&self) -> Value;

    /// Execute the tool with the given arguments
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult>;
}

/// Registry of available tools — maps names to tool implementations.
///
/// Used by the agent loop to:
/// 1. Get all tool definitions for the LLM (`get_definitions()`)
/// 2. Dispatch tool calls by name (`execute()`)
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Overwrites if name already exists.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        tracing::debug!(tool = %name, "Registered tool");
        self.tools.insert(name, tool);
    }

    /// Unregister a tool by name
    pub fn unregister(&mut self, name: &str) -> bool {
        self.tools.remove(name).is_some()
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// Get all tool definitions in OpenAI function-calling format
    pub fn get_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|tool| ToolDefinition {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: tool.name().to_string(),
                    description: tool.description().to_string(),
                    parameters: tool.parameters(),
                },
            })
            .collect()
    }

    /// Execute a tool by name with the given arguments.
    /// Returns error as ToolResult (not Err) for graceful LLM handling.
    pub async fn execute(&self, name: &str, args: Value, ctx: &ToolContext) -> ToolResult {
        let tool = match self.tools.get(name) {
            Some(t) => t,
            None => {
                return ToolResult::error(format!("Unknown tool: {name}"));
            }
        };

        tracing::debug!(tool = %name, args = %args, "Executing tool");

        match tool.execute(args, ctx).await {
            Ok(result) => {
                tracing::debug!(
                    tool = %name,
                    is_error = result.is_error,
                    output_len = result.output.len(),
                    "Tool execution complete"
                );
                result
            }
            Err(e) => {
                tracing::warn!(tool = %name, error = %e, "Tool execution failed");
                ToolResult::error(format!("Tool error: {e}"))
            }
        }
    }

    /// Number of registered tools
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether the registry is empty
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// List all tool names
    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to extract a required string parameter from args
pub fn get_string_param(args: &Value, name: &str) -> Result<String> {
    args.get(name)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .with_context(|| format!("Missing required parameter: {name}"))
}

/// Helper to extract an optional string parameter from args
pub fn get_optional_string(args: &Value, name: &str) -> Option<String> {
    args.get(name)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Helper to extract an optional bool parameter from args
pub fn get_optional_bool(args: &Value, name: &str) -> Option<bool> {
    args.get(name).and_then(|v| v.as_bool())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyTool;

    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            "dummy"
        }
        fn description(&self) -> &str {
            "A dummy tool for testing"
        }
        fn parameters(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "Test input" }
                },
                "required": ["input"]
            })
        }
        async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolResult> {
            let input = get_string_param(&args, "input")?;
            Ok(ToolResult::success(format!("echo: {input}")))
        }
    }

    fn test_ctx() -> ToolContext {
        ToolContext {
            workspace: "/tmp/test".to_string(),
            channel: "cli".to_string(),
            chat_id: "test".to_string(),
            message_tx: None,
            approval_manager: None,
        }
    }

    #[test]
    fn test_register_and_get() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool));
        assert_eq!(registry.len(), 1);
        assert!(registry.get("dummy").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_get_definitions() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool));
        let defs = registry.get_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].function.name, "dummy");
        assert_eq!(defs[0].tool_type, "function");
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool));

        let args = serde_json::json!({"input": "hello"});
        let result = registry.execute("dummy", args, &test_ctx()).await;
        assert!(!result.is_error);
        assert_eq!(result.output, "echo: hello");
    }

    #[tokio::test]
    async fn test_execute_unknown_tool() {
        let registry = ToolRegistry::new();
        let result = registry
            .execute("nope", serde_json::json!({}), &test_ctx())
            .await;
        assert!(result.is_error);
        assert!(result.output.contains("Unknown tool"));
    }

    #[tokio::test]
    async fn test_execute_missing_param() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool));

        let result = registry
            .execute("dummy", serde_json::json!({}), &test_ctx())
            .await;
        assert!(result.is_error);
        assert!(result.output.contains("Missing required parameter"));
    }

    #[test]
    fn test_unregister() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(DummyTool));
        assert!(registry.unregister("dummy"));
        assert!(!registry.unregister("dummy"));
        assert_eq!(registry.len(), 0);
    }
}
