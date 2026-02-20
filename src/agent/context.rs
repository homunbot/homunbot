use std::path::Path;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::config::Config;
use crate::provider::ChatMessage;

/// Bootstrap file names, loaded from ~/.homun/ if they exist.
///
/// Inspired by OpenClaw's SOUL.md pattern:
/// - SOUL.md:   personality, values, communication style
/// - AGENTS.md: directives on how the agent should behave
/// - USER.md:   user preferences, context, personal info
const BOOTSTRAP_FILES: &[(&str, &str)] = &[
    ("SOUL.md", "Personality & Identity"),
    ("AGENTS.md", "Agent Directives"),
    ("USER.md", "User Context"),
];

/// Builds the system prompt and assembles messages for the LLM.
///
/// Prompt layers (in order):
/// 1. Core identity + time + workspace
/// 2. Bootstrap files (SOUL.md, AGENTS.md, USER.md) — if present
/// 3. Guidelines
/// 4. Skills summary
pub struct ContextBuilder {
    workspace: String,
    /// Shared skills summary — can be updated at runtime by the hot-reload watcher.
    skills_summary: Arc<RwLock<String>>,
    bootstrap_content: String,
    memory_content: String,
    /// Known channels and their default chat IDs for cross-channel messaging
    channels_info: String,
}

impl ContextBuilder {
    pub fn new(_config: &Config) -> Self {
        let data_dir = Config::data_dir();
        let bootstrap_content = Self::load_bootstrap_files(&data_dir);

        Self {
            workspace: Config::workspace_dir()
                .to_string_lossy()
                .to_string(),
            skills_summary: Arc::new(RwLock::new(String::new())),
            bootstrap_content,
            memory_content: String::new(),
            channels_info: String::new(),
        }
    }

    /// Load bootstrap files (SOUL.md, AGENTS.md, USER.md) from data directory.
    /// Returns combined content, or empty string if none exist.
    fn load_bootstrap_files(data_dir: &Path) -> String {
        let mut content = String::new();

        for (filename, label) in BOOTSTRAP_FILES {
            let file_path = data_dir.join(filename);
            if file_path.exists() {
                match std::fs::read_to_string(&file_path) {
                    Ok(text) => {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            tracing::info!(file = %filename, "Loaded bootstrap file");
                            content.push_str(&format!("\n\n## {label}\n{trimmed}"));
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
        }

        content
    }

    /// Set the skills summary (called after skills are loaded)
    pub async fn set_skills_summary(&self, summary: String) {
        let mut guard = self.skills_summary.write().await;
        *guard = summary;
    }

    /// Get a shared handle to the skills summary for hot-reload updates.
    /// The watcher can update this `Arc<RwLock<String>>` and the context
    /// will pick up changes on the next `build_system_prompt()` call.
    pub fn skills_summary_handle(&self) -> Arc<RwLock<String>> {
        self.skills_summary.clone()
    }

    /// Set long-term memory content (loaded from MEMORY.md)
    pub fn set_memory(&mut self, memory: String) {
        self.memory_content = memory;
    }

    /// Set available channels info for cross-channel messaging.
    /// Format: list of (channel_name, default_chat_id) pairs.
    pub fn set_channels_info(&mut self, channels: &[(&str, &str)]) {
        if channels.is_empty() {
            return;
        }
        let mut info = String::from("\n\n## Available Channels\n");
        info.push_str("You can send messages to the user on any of these channels using the `send_message` tool ");
        info.push_str("with the `channel` and `chat_id` parameters:\n");
        for (name, chat_id) in channels {
            info.push_str(&format!("- **{name}**: chat_id = `{chat_id}`\n"));
        }
        info.push_str("\nWhen the user asks you to reply on a different channel (e.g. \"rispondimi su WhatsApp\"), ");
        info.push_str("use `send_message` with the appropriate channel and chat_id from above.\n");
        self.channels_info = info;
    }

    /// Build the system prompt
    pub async fn build_system_prompt(&self) -> String {
        let now = chrono::Local::now();

        // Layer 1: Core identity
        let mut prompt = format!(
            "You are Homun, a personal AI assistant — a digital homunculus that helps your user with tasks.\n\
            \n\
            Current Time: {}\n\
            Workspace: ~/workspace",
            now.format("%Y-%m-%d %H:%M (%A) %Z"),
        );

        // Layer 2: Bootstrap files (SOUL.md, AGENTS.md, USER.md)
        if !self.bootstrap_content.is_empty() {
            prompt.push_str(&self.bootstrap_content);
        }

        // Layer 3: Long-term memory (consolidated facts about the user)
        if !self.memory_content.is_empty() {
            prompt.push_str("\n\n## Long-term Memory\n");
            prompt.push_str(&self.memory_content);
        }

        // Layer 4: Guidelines
        prompt.push_str(
            "\n\n\
            Guidelines:\n\
            - Be concise and helpful\n\
            - When asked to perform tasks, use available tools\n\
            - If you cannot do something, explain why clearly\n\
            - Reply in the same language as the user's message",
        );

        // Layer 5: Skills summary (read from shared RwLock — may be hot-reloaded)
        let skills = self.skills_summary.read().await;
        if !skills.is_empty() {
            prompt.push_str(&skills);
        }
        drop(skills);

        // Layer 6: Available channels for cross-channel messaging
        if !self.channels_info.is_empty() {
            prompt.push_str(&self.channels_info);
        }

        prompt
    }

    /// Build the full message list for the LLM: system prompt + history + current user message
    pub async fn build_messages(
        &self,
        history: &[ChatMessage],
        user_message: &str,
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::with_capacity(history.len() + 2);

        // System prompt
        messages.push(ChatMessage::system(&self.build_system_prompt().await));

        // Conversation history
        messages.extend_from_slice(history);

        // Current user message
        messages.push(ChatMessage::user(user_message));

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
        assert!(prompt.contains("Guidelines"));
        assert!(prompt.contains("Workspace"));
    }

    #[tokio::test]
    async fn test_build_system_prompt_with_skills() {
        let config = Config::default();
        let ctx = ContextBuilder::new(&config);
        ctx.set_skills_summary("\n\nAvailable Skills:\n- test: A test skill\n".to_string()).await;
        let prompt = ctx.build_system_prompt().await;

        assert!(prompt.contains("Available Skills"));
        assert!(prompt.contains("test: A test skill"));
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
        let content = ContextBuilder::load_bootstrap_files(std::path::Path::new("/nonexistent"));
        assert!(content.is_empty());
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

        let content = ContextBuilder::load_bootstrap_files(dir.path());

        assert!(content.contains("Personality & Identity"));
        assert!(content.contains("friendly and witty"));
        assert!(content.contains("User Context"));
        assert!(content.contains("Fabio"));
        // AGENTS.md was not created, should not appear
        assert!(!content.contains("Agent Directives"));
    }
}
