//! SystemPromptBuilder - composes modular prompt sections.

use anyhow::Result;

use super::{PromptContext, PromptSection};

/// Builder for composing system prompts from modular sections.
///
/// Inspired by ZeroClaw's approach:
/// - Each section implements `PromptSection` trait
/// - Sections can be added/removed dynamically
/// - Sections respect `PromptMode` (full/minimal/none)
pub struct SystemPromptBuilder {
    sections: Vec<Box<dyn PromptSection>>,
}

impl Default for SystemPromptBuilder {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl SystemPromptBuilder {
    /// Create a builder with default sections in standard order.
    pub fn with_defaults() -> Self {
        use super::{
            BusinessSection, ContactsSection, IdentitySection, MemorySection, RuntimeSection,
            SafetySection, SkillsSection, ToolsSection, WorkspaceSection,
        };

        Self {
            sections: vec![
                Box::new(IdentitySection),
                Box::new(ToolsSection),
                Box::new(SafetySection),
                Box::new(SkillsSection),
                Box::new(BusinessSection),
                Box::new(MemorySection),
                Box::new(ContactsSection),
                Box::new(WorkspaceSection),
                Box::new(RuntimeSection),
            ],
        }
    }

    /// Create an empty builder (add sections manually).
    pub fn empty() -> Self {
        Self {
            sections: Vec::new(),
        }
    }

    /// Add a custom section.
    pub fn add_section(mut self, section: Box<dyn PromptSection>) -> Self {
        self.sections.push(section);
        self
    }

    /// Add a section at a specific position.
    pub fn add_section_at(mut self, index: usize, section: Box<dyn PromptSection>) -> Self {
        self.sections.insert(index, section);
        self
    }

    /// Remove a section by name.
    pub fn remove_section(mut self, name: &str) -> Self {
        self.sections.retain(|s| s.name() != name);
        self
    }

    /// Build the complete system prompt.
    pub fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let mut output = String::new();

        for section in &self.sections {
            // Skip based on mode
            if ctx.prompt_mode.is_none() && section.skip_in_none() {
                continue;
            }
            if ctx.prompt_mode.is_minimal() && section.skip_in_minimal() {
                continue;
            }

            let part = section.build(ctx)?;
            if !part.trim().is_empty() {
                output.push_str(&part);
                output.push_str("\n\n");
            }
        }

        Ok(output)
    }

    /// Build a minimal prompt for subagents.
    pub fn build_minimal(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let ctx = ctx.with_mode(super::PromptMode::Minimal);
        self.build(&ctx)
    }
}

/// Helper to clone PromptContext with a different mode.
impl<'a> PromptContext<'a> {
    pub fn with_mode(&self, mode: super::PromptMode) -> PromptContext<'a> {
        PromptContext {
            workspace_dir: self.workspace_dir,
            model_name: self.model_name,
            tools: self.tools,
            registered_tool_names: self.registered_tool_names,
            skills_summary: self.skills_summary,
            bootstrap_files: self.bootstrap_files,
            memory_content: self.memory_content,
            relevant_memories: self.relevant_memories,
            rag_knowledge: self.rag_knowledge,
            mcp_suggestions: self.mcp_suggestions,
            channel: self.channel,
            prompt_mode: mode,
            channels_info: self.channels_info,
            contact_context: self.contact_context,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::PromptMode;
    use std::path::Path;

    fn make_ctx() -> PromptContext<'static> {
        PromptContext {
            workspace_dir: Path::new("/tmp/workspace"),
            model_name: "test-model",
            tools: &[],
            registered_tool_names: &[],
            skills_summary: "",
            bootstrap_files: &[],
            memory_content: "",
            relevant_memories: "",
            rag_knowledge: "",
            mcp_suggestions: "",
            channel: "test",
            prompt_mode: PromptMode::Full,
            channels_info: "",
            contact_context: "",
        }
    }

    #[test]
    fn test_builder_produces_non_empty() {
        let builder = SystemPromptBuilder::with_defaults();
        let ctx = make_ctx();
        let prompt = builder.build(&ctx).unwrap();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("Homun"));
    }

    #[test]
    fn test_empty_builder_produces_empty() {
        let builder = SystemPromptBuilder::empty();
        let ctx = make_ctx();
        let prompt = builder.build(&ctx).unwrap();
        assert!(prompt.is_empty());
    }

    #[test]
    fn test_minimal_mode_skips_sections() {
        let builder = SystemPromptBuilder::with_defaults();
        let ctx = make_ctx().with_mode(PromptMode::Minimal);
        let prompt = builder.build(&ctx).unwrap();
        // Minimal should have fewer sections but still some content
        assert!(prompt.len() < builder.build(&make_ctx()).unwrap().len() || prompt.is_empty());
    }
}
