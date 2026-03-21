//! Tool definition assembly for the agent loop.
//!
//! Builds the list of tool definitions (built-in tools + skills) that
//! are sent to the LLM, applying blocked/allowed filters.

use std::collections::HashSet;

use tokio::sync::RwLock;

use crate::provider::{FunctionDefinition, ToolDefinition};
use crate::skills::loader::SkillRegistry;
use crate::tools::ToolRegistry;

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
