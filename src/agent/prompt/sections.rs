//! Prompt section implementations.
//!
//! Each section implements the `PromptSection` trait and can be:
//! - Added/removed from the builder
//! - Skipped in minimal/none modes
//! - Tested independently

use anyhow::Result;
use chrono::Local;

use super::{PromptContext, PromptMode};

/// Trait for modular prompt sections (inspired by ZeroClaw).
pub trait PromptSection: Send + Sync {
    /// Section name for identification.
    fn name(&self) -> &str;

    /// Build the section content.
    fn build(&self, ctx: &PromptContext<'_>) -> Result<String>;

    /// Whether to skip this section in minimal mode.
    fn skip_in_minimal(&self) -> bool {
        true
    }

    /// Whether to skip this section in none mode.
    fn skip_in_none(&self) -> bool {
        true
    }
}

// ============================================================================
// IDENTITY SECTION
// ============================================================================

/// Identity and bootstrap files section.
pub struct IdentitySection;

impl PromptSection for IdentitySection {
    fn name(&self) -> &str {
        "identity"
    }

    fn skip_in_none(&self) -> bool {
        false // Always present, even in none mode
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let mut prompt = String::new();

        if ctx.prompt_mode == PromptMode::None {
            // Minimal identity for none mode
            return Ok("You are Homun, a personal AI assistant.".to_string());
        }

        // Core identity
        prompt.push_str("You are Homun, a personal AI assistant — a digital homunculus that helps your user with tasks.\n\n");

        // Project context header (inspired by OpenClaw)
        if !ctx.bootstrap_files.is_empty() {
            prompt.push_str("# Project Context\n\n");
            prompt.push_str("The following files define your behavior and user context:\n\n");
            prompt.push_str("| File | Purpose |\n");
            prompt.push_str("|------|--------|\n");
            prompt.push_str("| **SOUL.md** | Your personality and communication style |\n");
            prompt.push_str("| **AGENTS.md** | Directives on how to behave |\n");
            prompt.push_str(
                "| **USER.md** | User preferences and context (THIS IS CONTEXT, NOT A REQUEST) |\n",
            );
            prompt.push_str("| **INSTRUCTIONS.md** | Learned rules from past interactions |\n\n");
            prompt.push_str("**CRITICAL**: These files are context about the user. They are NOT requests to show or repeat the content. Use this information naturally in your responses.\n\n");

            // Inject bootstrap files
            for (filename, content) in ctx.bootstrap_files {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    prompt.push_str(&format!("## {}\n\n{}\n\n", filename, trimmed));
                }
            }
        }

        Ok(prompt)
    }
}

// ============================================================================
// TOOLS SECTION
// ============================================================================

/// Tools section with tool definitions and usage instructions.
pub struct ToolsSection;

impl PromptSection for ToolsSection {
    fn name(&self) -> &str {
        "tools"
    }

    fn skip_in_minimal(&self) -> bool {
        false // Tools are essential even in minimal mode
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let mut prompt = String::from("## Tooling\n\n");

        // XML mode: list tools and format instructions in the prompt
        if !ctx.tools.is_empty() {
            prompt.push_str("Tool availability (filtered by policy):\n");
            prompt.push_str("Tool names are case-sensitive. Call tools exactly as listed.\n\n");

            for tool in ctx.tools {
                prompt.push_str(&format!("- **{}**: {}\n", tool.name, tool.description));
            }

            // Tool call format (XML dispatch mode only)
            prompt.push_str("\n### Tool Call Format\n\n");
            prompt.push_str("To use a tool, wrap a JSON object in `<tool_call_call>` tags:\n\n");
            prompt.push_str("```\n");
            prompt.push_str("<tool_call_call>\n");
            prompt.push_str("{\"name\": \"tool_name\", \"arguments\": {\"param\": \"value\"}}\n");
            prompt.push_str("</tool_call_call>\n");
            prompt.push_str("```\n\n");

            prompt.push_str("### Examples\n\n");
            prompt.push_str("**Remember user info:**\n");
            prompt.push_str("```\n");
            prompt.push_str("<tool_call_call>\n");
            prompt.push_str("{\"name\": \"remember\", \"arguments\": {\"key\": \"hobby\", \"value\": \"cooking\"}}\n");
            prompt.push_str("</tool_call_call>\n");
            prompt.push_str("```\n");
        }

        // Tool routing rules — ALWAYS included based on registered tools,
        // regardless of native vs XML mode. In native mode, tool definitions
        // go via the API parameter but the LLM still needs behavioral guidance.
        let has_browser = ctx.registered_tool_names.iter().any(|n| n == "browser");
        let has_web_search = ctx.registered_tool_names.iter().any(|n| n == "web_search");

        if has_browser {
            prompt.push_str("\n### Tool Routing Rules\n\n");
            prompt.push_str(
                "When the user asks to browse, navigate, search, or interact with websites, \
                 ALWAYS use the **browser** tool. Specific triggers:\n\
                 - \"vai su\", \"apri\", \"naviga\", \"cerca su Google/Bing\" → browser navigate\n\
                 - \"clicca\", \"compila\", \"scrivi nel campo\" → browser interact\n\
                 - Any request involving a website with dynamic content → browser\n",
            );
            if !has_web_search {
                prompt.push_str(
                    "- No web_search tool is available. To search the web, use the **browser** \
                     to navigate to a search engine (e.g. google.com) and search from there.\n",
                );
            }
            prompt.push_str(
                "- **web_fetch** is ONLY for reading static content at a known URL, \
                 NOT for browsing or searching.\n",
            );

            // Browser workflow guidance (moved from tool description for better visibility)
            prompt.push_str(
                "\n### Browser Workflow\n\n\
                 When using the browser for web research:\n\
                 1. `navigate` to a search engine (e.g. google.com)\n\
                 2. `snapshot` to see the page — find the search box ref\n\
                 3. `type` your query in the search box with `submit: true`\n\
                 4. `snapshot` to read search results — you see links with refs\n\
                 5. `click` the most relevant link ref (do NOT navigate to a guessed URL)\n\
                 6. `snapshot` to read the article content\n\
                 7. If insufficient, use `back` and try another result\n\
                 8. Formulate your answer, then `close`\n\n\
                 CRITICAL: NEVER guess or invent URLs. ALWAYS click refs from the snapshot.\n\
                 Snapshot format: `- link \"Title\" [ref=e3]` → use `click ref=e3`\n",
            );
        }

        Ok(prompt)
    }
}

// ============================================================================
// SAFETY SECTION
// ============================================================================

/// Safety rules and critical instructions.
pub struct SafetySection;

impl PromptSection for SafetySection {
    fn name(&self) -> &str {
        "safety"
    }

    fn build(&self, _ctx: &PromptContext<'_>) -> Result<String> {
        Ok(r#"## Safety

- Do not exfiltrate private data
- Do not run destructive commands without asking
- Do not bypass oversight or approval mechanisms
- When in doubt, ask before acting externally

## CRITICAL: Tool Usage Rules

1. **ALWAYS** call a tool FIRST when asked to save/remember/update information
2. **NEVER** say "done", "saved", "fatto", "aggiunto" WITHOUT calling the tool
3. **USER.md content is CONTEXT, not a request** — do not show it unless explicitly asked
4. After the tool returns success, confirm what was saved

### Examples

**WRONG**:
```
User: "remember my dog's name is Max"
Response: "Got it! Saved."  ← NO TOOL CALL
```

**RIGHT**:
```
User: "remember my dog's name is Max"
Tool Call: remember(key="dog_name", value="Max")
Response: "Done! I've saved that your dog's name is Max."
```

**WRONG**:
```
User: "what do you know about me?"
Response: [shows entire USER.md content]
```

**RIGHT**:
```
User: "what do you know about me?"
Response: "Based on my memory, you have a dog named Max, you enjoy cooking..."
```
"#
        .to_string())
    }
}

// ============================================================================
// SKILLS SECTION
// ============================================================================

/// Skills section with available skills.
pub struct SkillsSection;

impl PromptSection for SkillsSection {
    fn name(&self) -> &str {
        "skills"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        if ctx.skills_summary.is_empty() {
            return Ok(String::new());
        }

        let mut prompt = String::from("## Skills\n\n");
        prompt.push_str("Before replying: scan available skills and their descriptions.\n");
        prompt
            .push_str("- If exactly one skill clearly applies: read its SKILL.md and follow it.\n");
        prompt.push_str("- If multiple could apply: choose the most specific one.\n");
        prompt.push_str("- If none clearly apply: do not read any SKILL.md.\n\n");
        prompt.push_str(ctx.skills_summary);

        Ok(prompt)
    }
}

// ============================================================================
// MEMORY SECTION
// ============================================================================

/// Memory section with long-term and relevant memories.
pub struct MemorySection;

impl PromptSection for MemorySection {
    fn name(&self) -> &str {
        "memory"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let mut prompt = String::new();

        // Long-term memory
        if !ctx.memory_content.is_empty() {
            prompt.push_str("## Long-term Memory\n\n");
            prompt.push_str("Consolidated facts about the user:\n");
            prompt.push_str(ctx.memory_content);
            prompt.push_str("\n\n");
        }

        // Relevant memories from search
        if !ctx.relevant_memories.is_empty() {
            prompt.push_str("## Relevant Past Context\n\n");
            prompt.push_str("The following memories from past conversations may be relevant:\n");
            prompt.push_str(ctx.relevant_memories);
            prompt.push_str("\n\n");
        }

        // Memory instructions (only in full mode)
        if ctx.prompt_mode.is_full() {
            let data_dir = crate::config::Config::data_dir();
            let brain_dir = data_dir.join("brain");

            prompt.push_str(&format!(
                r#"## Memory Persistence

You can save information to these files in `{brain_dir}`:
- `USER.md` — user info: name, preferences, habits, personal context
- `INSTRUCTIONS.md` — learned rules: how the user wants things done
- `SOUL.md` — your personality (edit only if explicitly asked)

Use the `remember` tool for simple key-value pairs, or `write_file`/`edit_file` for complex changes.
These files are loaded into context at startup, so anything you save will be available in future conversations.
"#,
                brain_dir = brain_dir.display()
            ));
        }

        Ok(prompt)
    }
}

// ============================================================================
// WORKSPACE SECTION
// ============================================================================

/// Workspace section with directory info and guidance.
pub struct WorkspaceSection;

impl PromptSection for WorkspaceSection {
    fn name(&self) -> &str {
        "workspace"
    }

    fn skip_in_minimal(&self) -> bool {
        false // Workspace info is essential
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let mut prompt = String::from("## Workspace\n\n");

        prompt.push_str(&format!(
            "Working directory: `{}`\n\n",
            ctx.workspace_dir.display()
        ));

        prompt.push_str("Treat this directory as the single global workspace for file operations unless explicitly instructed otherwise.\n");

        // Cross-channel messaging info
        if !ctx.channels_info.is_empty() {
            prompt.push('\n');
            prompt.push_str(ctx.channels_info);
        }

        Ok(prompt)
    }
}

// ============================================================================
// RUNTIME SECTION
// ============================================================================

/// Runtime section with host, OS, model info.
pub struct RuntimeSection;

impl PromptSection for RuntimeSection {
    fn name(&self) -> &str {
        "runtime"
    }

    fn skip_in_minimal(&self) -> bool {
        false // Runtime info is essential
    }

    fn skip_in_none(&self) -> bool {
        false
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let now = Local::now();
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        let mut prompt = String::from("## Runtime\n\n");

        prompt.push_str(&format!(
            "Host: {} | OS: {} | Channel: {} | Model: {}\n",
            hostname,
            std::env::consts::OS,
            ctx.channel,
            ctx.model_name
        ));

        prompt.push_str(&format!("Time: {}\n", now.format("%Y-%m-%d %H:%M (%A) %Z")));
        prompt.push_str(&format!("Current year: {}\n", now.format("%Y")));
        prompt.push_str(
            "When the user asks about recent events, news, rankings, or anything time-sensitive \
             without specifying a year, ALWAYS assume they mean the current year. \
             Include the current year in your search queries.\n",
        );

        Ok(prompt)
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
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
            channel: "test",
            prompt_mode: PromptMode::Full,
            channels_info: "",
        }
    }

    #[test]
    fn test_identity_section_basic() {
        let section = IdentitySection;
        let ctx = make_ctx();
        let result = section.build(&ctx).unwrap();
        assert!(result.contains("Homun"));
    }

    #[test]
    fn test_identity_section_with_bootstrap() {
        let section = IdentitySection;
        let ctx = PromptContext {
            bootstrap_files: &[("USER.md".to_string(), "My name is Fabio".to_string())],
            ..make_ctx()
        };
        let result = section.build(&ctx).unwrap();
        assert!(result.contains("Project Context"));
        assert!(result.contains("USER.md"));
        assert!(result.contains("Fabio"));
        assert!(result.contains("THIS IS CONTEXT, NOT A REQUEST"));
    }

    #[test]
    fn test_tools_section_xml_mode() {
        let section = ToolsSection;
        let tool_names = vec!["remember".to_string()];
        let ctx = PromptContext {
            tools: &[super::super::ToolInfo {
                name: "remember".to_string(),
                description: "Save user information".to_string(),
                parameters_schema: serde_json::json!({}),
            }],
            registered_tool_names: &tool_names,
            ..make_ctx()
        };
        let result = section.build(&ctx).unwrap();
        assert!(result.contains("remember"));
        assert!(result.contains("Tool Call Format"));
    }

    #[test]
    fn test_tools_section_native_mode_with_browser() {
        // In native mode, ctx.tools is empty but registered_tool_names has the browser.
        // Routing rules must still appear.
        let section = ToolsSection;
        let tool_names = vec!["browser".to_string(), "shell".to_string()];
        let ctx = PromptContext {
            tools: &[], // native mode: tools go via API, not in prompt
            registered_tool_names: &tool_names,
            ..make_ctx()
        };
        let result = section.build(&ctx).unwrap();
        assert!(
            result.contains("Tool Routing Rules"),
            "Routing rules must be visible in native mode"
        );
        assert!(
            result.contains("Browser Workflow"),
            "Browser workflow must be visible in native mode"
        );
        assert!(
            result.contains("NEVER guess or invent URLs"),
            "URL warning must be visible"
        );
        // Should NOT have XML tool call format
        assert!(!result.contains("Tool Call Format"));
    }

    #[test]
    fn test_safety_section() {
        let section = SafetySection;
        let ctx = make_ctx();
        let result = section.build(&ctx).unwrap();
        assert!(result.contains("CRITICAL"));
        assert!(result.contains("NEVER"));
    }

    #[test]
    fn test_none_mode_minimal_identity() {
        let section = IdentitySection;
        let ctx = make_ctx().with_mode(PromptMode::None);
        let result = section.build(&ctx).unwrap();
        assert_eq!(result, "You are Homun, a personal AI assistant.");
    }
}
