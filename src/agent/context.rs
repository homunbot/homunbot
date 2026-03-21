//! Context builder for system prompts.
//!
//! This module provides backward-compatible context building using the new
//! modular `SystemPromptBuilder` internally.

use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::agent::prompt::{PromptContext, PromptMode, SystemPromptBuilder, ToolInfo};
use crate::config::Config;
use crate::provider::ChatMessage;

/// Bootstrap file names and their search paths.
///
/// Inspired by OpenClaw's SOUL.md pattern:
/// - SOUL.md:   personality, values, communication style
/// - AGENTS.md: directives on how the agent should behave
/// - USER.md:   user preferences, context, personal info
///
/// Files are loaded from two directories (first match wins):
/// 1. `~/.homun/brain/` — agent-written memory files (USER.md, INSTRUCTIONS.md, SOUL.md)
/// 2. `~/.homun/` — user-placed config files (SOUL.md, AGENTS.md)
const BOOTSTRAP_FILES: &[(&str, &str)] = &[
    ("SOUL.md", "Personality & Identity"),
    ("AGENTS.md", "Agent Directives"),
    ("USER.md", "User Context"),
    ("INSTRUCTIONS.md", "Learned Instructions"),
];

/// Builds the system prompt and assembles messages for the LLM.
///
/// Uses the new modular `SystemPromptBuilder` internally for a cleaner,
/// more extensible prompt architecture inspired by ZeroClaw/OpenClaw.
pub struct ContextBuilder {
    workspace: String,
    /// Shared skills summary — can be updated at runtime by the hot-reload watcher.
    skills_summary: Arc<RwLock<String>>,
    /// Shared bootstrap content — can be updated at runtime by the hot-reload watcher.
    bootstrap_content: Arc<RwLock<String>>,
    /// Bootstrap files as (filename, content) pairs for the new prompt system.
    bootstrap_files: Arc<RwLock<Vec<(String, String)>>>,
    memory_content: String,
    /// Relevant memories retrieved by vector + FTS5 search for the current query.
    /// Uses RwLock for interior mutability — updated per-request via `&self`.
    relevant_memories: RwLock<String>,
    /// Relevant knowledge from RAG knowledge base search.
    rag_knowledge: RwLock<String>,
    /// Contextual MCP setup suggestions inferred from the active request.
    mcp_suggestions: RwLock<String>,
    /// Known channels and their default chat IDs for cross-channel messaging
    channels_info: String,
    /// The modular prompt builder
    prompt_builder: SystemPromptBuilder,
    /// Current model name (for runtime section).
    /// Uses RwLock so agent_loop can update it when model changes via hot-reload.
    model_name: RwLock<String>,
    /// Names of all registered tools (always set, for routing rules in system prompt).
    /// Uses RwLock so deferred MCP tools can be added after startup.
    registered_tool_names: RwLock<Vec<String>>,
    /// Contact context for the current message sender (CTB-5).
    contact_context: RwLock<String>,
    /// Persona prompt prefix (resolved per-message from contact > channel > "bot").
    persona_context: RwLock<String>,
    /// Per-agent instructions from `AgentDefinition`.
    agent_instructions: RwLock<String>,
    /// Cognition understanding (what the user wants, natural language).
    cognition_understanding: RwLock<String>,
    /// Cognition plan steps.
    cognition_plan: RwLock<Vec<String>>,
    /// Cognition constraints extracted from the user's request.
    cognition_constraints: RwLock<Vec<String>>,
}

impl ContextBuilder {
    pub fn new(config: &Config) -> Self {
        let data_dir = Config::data_dir();
        let (bootstrap_content, bootstrap_files) = Self::load_bootstrap_files(&data_dir);

        Self {
            workspace: Config::workspace_dir().to_string_lossy().to_string(),
            skills_summary: Arc::new(RwLock::new(String::new())),
            bootstrap_content: Arc::new(RwLock::new(bootstrap_content)),
            bootstrap_files: Arc::new(RwLock::new(bootstrap_files)),
            memory_content: String::new(),
            relevant_memories: RwLock::new(String::new()),
            rag_knowledge: RwLock::new(String::new()),
            mcp_suggestions: RwLock::new(String::new()),
            channels_info: String::new(),
            prompt_builder: SystemPromptBuilder::with_defaults(),
            model_name: RwLock::new(config.agent.model.clone()),
            registered_tool_names: RwLock::new(Vec::new()),
            contact_context: RwLock::new(String::new()),
            persona_context: RwLock::new(String::new()),
            agent_instructions: RwLock::new(String::new()),
            cognition_understanding: RwLock::new(String::new()),
            cognition_plan: RwLock::new(Vec::new()),
            cognition_constraints: RwLock::new(Vec::new()),
        }
    }

    /// Load bootstrap files (SOUL.md, AGENTS.md, USER.md, INSTRUCTIONS.md).
    ///
    /// Search order for each file (first match wins):
    /// 1. `~/.homun/brain/` — agent-written memory (allowed by file tool)
    /// 2. `~/.homun/` — user-placed configuration (protected by file tool)
    fn load_bootstrap_files(data_dir: &Path) -> (String, Vec<(String, String)>) {
        let mut content = String::new();
        let mut files = Vec::new();
        let brain_dir = data_dir.join("brain");

        for (filename, label) in BOOTSTRAP_FILES {
            // Try brain/ first (agent-written), then data_dir (user-placed)
            let candidates = [brain_dir.join(filename), data_dir.join(filename)];
            let file_path = match candidates.iter().find(|p| p.exists()) {
                Some(p) => p,
                None => continue,
            };

            match std::fs::read_to_string(file_path) {
                Ok(text) => {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        tracing::info!(
                            file = %filename,
                            source = %file_path.display(),
                            "Loaded bootstrap file"
                        );
                        // Old format for backward compatibility
                        content.push_str(&format!("\n\n## {label}\n{trimmed}"));
                        // New format for modular system
                        files.push((filename.to_string(), trimmed.to_string()));
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        file = %filename,
                        error = %e,
                        "Failed to read bootstrap file"
                    );
                }
            }
        }

        (content, files)
    }

    /// Reload bootstrap files from disk (called by hot-reload watcher).
    pub async fn reload_bootstrap_files(&self) {
        let data_dir = Config::data_dir();
        let (content, files) = Self::load_bootstrap_files(&data_dir);

        {
            let mut guard = self.bootstrap_content.write().await;
            *guard = content;
        }
        {
            let mut guard = self.bootstrap_files.write().await;
            *guard = files;
        }

        tracing::info!("Reloaded bootstrap files");
    }

    /// Set the skills summary (called after skills are loaded)
    pub async fn set_skills_summary(&self, summary: String) {
        let mut guard = self.skills_summary.write().await;
        *guard = summary;
    }

    /// Get a shared handle to the skills summary for hot-reload updates.
    pub fn skills_summary_handle(&self) -> Arc<RwLock<String>> {
        self.skills_summary.clone()
    }

    /// Get a shared handle to the bootstrap content for hot-reload updates.
    /// The watcher can update this `Arc<RwLock<String>>` and the context
    /// will pick up changes on the next `build_system_prompt()` call.
    pub fn bootstrap_content_handle(&self) -> Arc<RwLock<String>> {
        self.bootstrap_content.clone()
    }

    /// Get a shared handle to the bootstrap files (new format) for hot-reload updates.
    /// The watcher can update this `Arc<RwLock<Vec<(String, String)>>>` and the context
    /// will pick up changes on the next `build_system_prompt()` call.
    pub fn bootstrap_files_handle(&self) -> Arc<RwLock<Vec<(String, String)>>> {
        self.bootstrap_files.clone()
    }

    /// Get both handles for the BootstrapWatcher (convenience method).
    #[allow(clippy::type_complexity)]
    pub fn bootstrap_handles(&self) -> (Arc<RwLock<String>>, Arc<RwLock<Vec<(String, String)>>>) {
        (self.bootstrap_content.clone(), self.bootstrap_files.clone())
    }

    /// Set long-term memory content (loaded from MEMORY.md)
    pub fn set_memory(&mut self, memory: String) {
        self.memory_content = memory;
    }

    /// Set relevant memories from vector + FTS5 search.
    pub async fn set_relevant_memories(&self, memories: String) {
        let mut guard = self.relevant_memories.write().await;
        *guard = memories;
    }

    /// Set relevant RAG knowledge base results for the current query.
    pub async fn set_rag_knowledge(&self, knowledge: String) {
        let mut guard = self.rag_knowledge.write().await;
        *guard = knowledge;
    }

    /// Set contextual MCP setup suggestions for the current request.
    pub async fn set_mcp_suggestions(&self, suggestions: String) {
        let mut guard = self.mcp_suggestions.write().await;
        *guard = suggestions;
    }

    /// Set the contact context for the current message sender (CTB-5).
    pub async fn set_contact_context(&self, ctx: String) {
        *self.contact_context.write().await = ctx;
    }

    /// Update the persona prompt prefix (called per-message after persona resolution).
    pub async fn set_persona_context(&self, ctx: String) {
        *self.persona_context.write().await = ctx;
    }

    /// Set per-agent instructions (from `AgentDefinition.instructions`).
    pub async fn set_agent_instructions(&self, instructions: &str) {
        *self.agent_instructions.write().await = instructions.to_string();
    }

    /// Set cognition results for injection into the system prompt.
    pub async fn set_cognition_context(
        &self,
        understanding: String,
        plan: Vec<String>,
        constraints: Vec<String>,
    ) {
        *self.cognition_understanding.write().await = understanding;
        *self.cognition_plan.write().await = plan;
        *self.cognition_constraints.write().await = constraints;
    }

    /// Clear cognition context (called when cognition is disabled or fails).
    pub async fn clear_cognition_context(&self) {
        *self.cognition_understanding.write().await = String::new();
        *self.cognition_plan.write().await = Vec::new();
        *self.cognition_constraints.write().await = Vec::new();
    }

    /// Update the model name shown in the system prompt (called on hot-reload).
    pub async fn set_model_name(&self, model: String) {
        *self.model_name.write().await = model;
    }

    /// Set the names of all registered tools (for routing rules in system prompt).
    pub async fn set_registered_tool_names(&self, names: Vec<String>) {
        *self.registered_tool_names.write().await = names;
    }

    /// Append additional tool names (called when deferred MCP tools register).
    pub async fn append_registered_tool_names(&self, names: &[String]) {
        let mut current = self.registered_tool_names.write().await;
        for name in names {
            if !current.contains(name) {
                current.push(name.clone());
            }
        }
    }

    /// Get a snapshot of the registered tool names.
    pub async fn registered_tool_names_snapshot(&self) -> Vec<String> {
        self.registered_tool_names.read().await.clone()
    }

    /// Set available channels info for cross-channel messaging.
    pub fn set_channels_info(&mut self, channels: &[(&str, &str)]) {
        if channels.is_empty() {
            return;
        }
        let mut info = String::from("\n\n## Available Channels\n");
        info.push_str("You can send messages using the `send_message` tool ");
        info.push_str("with `channel` and `chat_id` parameters:\n");
        for (name, chat_id) in channels {
            info.push_str(&format!("- **{name}**: default chat_id = `{chat_id}`\n"));
        }
        info.push_str("\n### Cross-channel messaging\n");
        info.push_str("When the user asks you to reply on a different channel (e.g. \"rispondimi su WhatsApp\"), ");
        info.push_str("use `send_message` with the appropriate channel and chat_id from above.\n");
        info.push_str("\n### Sending emails\n");
        info.push_str("To send an email, use `send_message` with the email channel name (e.g. `channel=\"email:lavoro\"`) ");
        info.push_str(
            "and `chat_id` set to the **recipient's email address** (not the bot's address).\n",
        );
        info.push_str(
            "Format the content as: `Subject: <subject>\\n<body>` to set the subject line.\n",
        );
        info.push_str(
            "If no `Subject:` prefix is provided, the subject defaults to \"Homun Response\".\n",
        );
        // Append channel capabilities so the LLM knows what each channel supports
        let channel_names: Vec<&str> = channels.iter().map(|(name, _)| *name).collect();
        info.push_str(&crate::channels::capabilities::build_capabilities_prompt(
            &channel_names,
        ));
        self.channels_info = info;
    }

    /// Set email account details in the system prompt.
    ///
    /// Informs the agent about each email account's mode and behavior.
    pub fn set_email_accounts_info(&mut self, accounts: &[(String, crate::config::EmailMode)]) {
        if accounts.is_empty() {
            return;
        }
        let mut info = String::from("\n\n## Email Accounts\n");
        info.push_str("You manage the following email accounts:\n\n");
        for (name, mode) in accounts {
            let mode_desc = match mode {
                crate::config::EmailMode::Assisted => {
                    "ASSISTED — When you receive an email on this account, \
                     generate a summary and a draft reply. Present both to the user \
                     on the notify channel and wait for approval before sending."
                }
                crate::config::EmailMode::Automatic => {
                    "AUTOMATIC — Respond directly to emails when you have enough information. \
                     If you lack info or the response would include vault secrets, \
                     escalate to assisted mode (show summary + draft on notify channel)."
                }
                crate::config::EmailMode::OnDemand => {
                    "ON-DEMAND — Only process emails containing a trigger word. \
                     When triggered, behave as ASSISTED (summary + draft + approval)."
                }
            };
            info.push_str(&format!("- **{name}**: {mode_desc}\n"));
        }
        info.push_str("\n### Email Security Rules\n");
        info.push_str(
            "- NEVER include vault secrets (API keys, passwords, tokens) in email responses.\n",
        );
        info.push_str(
            "- If a response would contain vault data, always escalate to assisted mode.\n",
        );
        info.push_str("- When in batch/digest mode, present the digest to the user and wait for instructions.\n");
        self.channels_info.push_str(&info);
    }

    /// Build the system prompt using the new modular system.
    pub async fn build_system_prompt(&self) -> String {
        self.build_system_prompt_with_tools(&[]).await
    }

    /// Build the system prompt with tool definitions for the ToolsSection.
    pub async fn build_system_prompt_with_tools(&self, tools: &[ToolInfo]) -> String {
        // Gather all context
        let bootstrap_files = self.bootstrap_files.read().await;
        let skills_summary = self.skills_summary.read().await;
        let relevant_memories = self.relevant_memories.read().await;
        let rag_knowledge = self.rag_knowledge.read().await;
        let mcp_suggestions = self.mcp_suggestions.read().await;
        let model_name = self.model_name.read().await;
        let registered_tool_names = self.registered_tool_names.read().await;
        let contact_context = self.contact_context.read().await;
        let persona_context = self.persona_context.read().await;
        let agent_instructions = self.agent_instructions.read().await;
        let cognition_understanding = self.cognition_understanding.read().await;
        let cognition_plan = self.cognition_plan.read().await;
        let cognition_constraints = self.cognition_constraints.read().await;

        // Build PromptContext
        let ctx = PromptContext {
            workspace_dir: std::path::Path::new(&self.workspace),
            model_name: &model_name,
            tools,
            registered_tool_names: &registered_tool_names,
            skills_summary: &skills_summary,
            bootstrap_files: &bootstrap_files,
            memory_content: &self.memory_content,
            relevant_memories: &relevant_memories,
            rag_knowledge: &rag_knowledge,
            mcp_suggestions: &mcp_suggestions,
            channel: "main",
            prompt_mode: PromptMode::Full,
            channels_info: &self.channels_info,
            contact_context: &contact_context,
            persona_context: &persona_context,
            agent_instructions: &agent_instructions,
            cognition_understanding: &cognition_understanding,
            cognition_plan: &cognition_plan,
            cognition_constraints: &cognition_constraints,
        };

        // Build prompt using modular system
        match self.prompt_builder.build(&ctx) {
            Ok(prompt) => prompt,
            Err(e) => {
                tracing::error!(error = %e, "Failed to build modular prompt, using fallback");
                self.build_fallback_prompt().await
            }
        }
    }

    /// Fallback prompt in case modular builder fails.
    async fn build_fallback_prompt(&self) -> String {
        let now = chrono::Local::now();
        let mut prompt = format!(
            "You are Homun, a personal AI assistant.\n\nTime: {}\nWorkspace: {}",
            now.format("%Y-%m-%d %H:%M (%A) %Z"),
            self.workspace
        );

        let bootstrap = self.bootstrap_content.read().await;
        if !bootstrap.is_empty() {
            prompt.push_str(&bootstrap);
        }

        if !self.memory_content.is_empty() {
            prompt.push_str("\n\n## Memory\n");
            prompt.push_str(&self.memory_content);
        }

        prompt
    }

    /// Build the full message list for the LLM: system prompt + history + current user message
    pub async fn build_messages(
        &self,
        history: &[ChatMessage],
        user_message: &str,
    ) -> Vec<ChatMessage> {
        self.build_messages_with_user_message(history, ChatMessage::user(user_message), &[])
            .await
    }

    /// Build messages with tool definitions included in the prompt.
    pub async fn build_messages_with_tools(
        &self,
        history: &[ChatMessage],
        user_message: &str,
        tools: &[ToolInfo],
    ) -> Vec<ChatMessage> {
        self.build_messages_with_user_message(history, ChatMessage::user(user_message), tools)
            .await
    }

    pub async fn build_messages_with_user_message(
        &self,
        history: &[ChatMessage],
        user_message: ChatMessage,
        tools: &[ToolInfo],
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::with_capacity(history.len() + 2);

        // System prompt with tools
        messages.push(ChatMessage::system(
            &self.build_system_prompt_with_tools(tools).await,
        ));

        // Conversation history
        messages.extend_from_slice(history);

        // Current user message
        messages.push(user_message);

        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_build_system_prompt_default() {
        let config = Config::default();
        let ctx = ContextBuilder::new(&config);
        let prompt = ctx.build_system_prompt().await;

        assert!(prompt.contains("Homun"));
        assert!(prompt.contains("Safety"));
    }

    #[tokio::test]
    async fn test_build_system_prompt_with_tools() {
        let config = Config::default();
        let ctx = ContextBuilder::new(&config);
        let tools = vec![ToolInfo {
            name: "remember".to_string(),
            description: "Save user info".to_string(),
            parameters_schema: serde_json::json!({}),
        }];
        let prompt = ctx.build_system_prompt_with_tools(&tools).await;

        assert!(prompt.contains("remember"));
        assert!(prompt.contains("Tool Call Format"));
    }

    #[tokio::test]
    async fn test_build_messages() {
        let config = Config::default();
        let ctx = ContextBuilder::new(&config);
        let history = vec![
            ChatMessage::user("Hello"),
            ChatMessage {
                role: "assistant".to_string(),
                content: Some("Hi!".to_string()),
                content_parts: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ];
        let messages = ctx.build_messages(&history, "How are you?").await;

        assert_eq!(messages.len(), 4); // system + 2 history + user
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[3].content.as_deref(), Some("How are you?"));
    }

    #[test]
    fn test_bootstrap_files_from_nonexistent_dir() {
        let (content, files) =
            ContextBuilder::load_bootstrap_files(std::path::Path::new("/nonexistent"));
        assert!(content.is_empty());
        assert!(files.is_empty());
    }

    #[tokio::test]
    async fn test_bootstrap_files_loaded() {
        let dir = tempfile::TempDir::new().unwrap();

        // Create a SOUL.md
        std::fs::write(
            dir.path().join("SOUL.md"),
            "You are a friendly and witty assistant.\nYou love puns.",
        )
        .unwrap();

        // Create a USER.md
        std::fs::write(
            dir.path().join("USER.md"),
            "The user is a Rust developer named Fabio.",
        )
        .unwrap();

        let (content, files) = ContextBuilder::load_bootstrap_files(dir.path());

        assert!(content.contains("Personality & Identity"));
        assert!(content.contains("friendly and witty"));
        assert!(content.contains("User Context"));
        assert!(content.contains("Fabio"));

        // Check new format
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|(n, _)| n == "SOUL.md"));
        assert!(files.iter().any(|(n, _)| n == "USER.md"));
    }
}
