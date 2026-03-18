//! Modular prompt system inspired by ZeroClaw/OpenClaw.
//!
//! This module provides a scalable architecture for building system prompts:
//! - `PromptSection` trait for modular sections
//! - `SystemPromptBuilder` for composing sections
//! - `PromptMode` for different contexts (main, subagent, minimal)

mod builder;
mod sections;

pub use builder::SystemPromptBuilder;
pub use sections::{
    BusinessSection, ContactsSection, IdentitySection, MemorySection, PromptSection,
    RuntimeSection, SafetySection, SkillsSection, ToolsSection, WorkspaceSection,
};

use std::path::Path;

/// Shared context passed to all prompt sections.
pub struct PromptContext<'a> {
    /// Workspace directory path
    pub workspace_dir: &'a Path,
    /// Current model name
    pub model_name: &'a str,
    /// Tool definitions for the tools section (only populated in XML mode)
    pub tools: &'a [ToolInfo],
    /// Names of all registered tools (always populated, for routing rules)
    pub registered_tool_names: &'a [String],
    /// Skills summary for the skills section
    pub skills_summary: &'a str,
    /// Bootstrap files: (filename, content)
    pub bootstrap_files: &'a [(String, String)],
    /// Long-term memory content (MEMORY.md)
    pub memory_content: &'a str,
    /// Relevant memories from vector search
    pub relevant_memories: &'a str,
    /// Relevant knowledge from RAG knowledge base search
    pub rag_knowledge: &'a str,
    /// Contextual MCP setup suggestions inferred from the current request.
    pub mcp_suggestions: &'a str,
    /// Originating channel (web, telegram, etc.)
    pub channel: &'a str,
    /// Prompt mode (full, minimal, none)
    pub prompt_mode: PromptMode,
    /// Available channels for cross-channel messaging
    pub channels_info: &'a str,
    /// Contact profile for the current message sender (CTB-5)
    pub contact_context: &'a str,
}

/// Tool information for prompt generation.
#[derive(Clone, Debug)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
}

/// Prompt mode for different contexts.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptMode {
    /// Full prompt with all sections (main agent)
    #[default]
    Full,
    /// Reduced sections (subagent) - only Tooling, Workspace, Runtime
    Minimal,
    /// Just basic identity line
    None,
}

impl PromptMode {
    pub fn is_minimal(&self) -> bool {
        matches!(self, PromptMode::Minimal)
    }

    pub fn is_none(&self) -> bool {
        matches!(self, PromptMode::None)
    }

    pub fn is_full(&self) -> bool {
        matches!(self, PromptMode::Full)
    }
}
