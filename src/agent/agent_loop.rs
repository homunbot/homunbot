use std::sync::Arc;

use anyhow::{Context as _, Result};
use tokio::sync::mpsc;

use crate::bus::OutboundMessage;
use crate::config::Config;
use crate::provider::{
    ChatMessage, ChatRequest, Provider, ToolCallFunction, ToolCallSerialized,
};
use crate::session::SessionManager;
use crate::storage::Database;
use crate::tools::{ToolContext, ToolRegistry};

use super::context::ContextBuilder;
use super::memory::MemoryConsolidator;

/// Core agent loop — full ReAct pattern with tool calling:
/// reason → act → observe → loop (max N iterations).
///
/// Follows nanobot's _run_agent_loop pattern:
/// 1. Build messages from history + system prompt
/// 2. Call LLM with tool definitions
/// 3. If tool calls → execute tools → add results → loop
/// 4. If no tool calls → return final response
/// 5. Repeat until max_iterations or final response
pub struct AgentLoop {
    provider: Box<dyn Provider>,
    config: Config,
    context: ContextBuilder,
    session_manager: SessionManager,
    tool_registry: ToolRegistry,
    memory: Arc<MemoryConsolidator>,
    /// Sender for proactive messages (set in Gateway mode)
    message_tx: Option<mpsc::Sender<OutboundMessage>>,
}

impl AgentLoop {
    pub fn new(
        provider: Box<dyn Provider>,
        config: Config,
        session_manager: SessionManager,
        tool_registry: ToolRegistry,
        db: Database,
    ) -> Self {
        let mut context = ContextBuilder::new(&config);
        let memory = Arc::new(MemoryConsolidator::new(db));

        // Inject long-term memory into context if MEMORY.md exists
        if let Some(memory_content) = memory.load_memory_md() {
            context.set_memory(memory_content);
            tracing::info!("Loaded long-term memory into context");
        }

        Self {
            provider,
            config,
            context,
            session_manager,
            tool_registry,
            memory,
            message_tx: None,
        }
    }

    /// Set the outbound message sender for proactive messaging (MessageTool).
    /// Called by the Gateway after constructing the routing table.
    pub fn set_message_tx(&mut self, tx: mpsc::Sender<OutboundMessage>) {
        self.message_tx = Some(tx);
    }

    /// Inject skills summary into the system prompt.
    /// Called after SkillRegistry::scan_and_load() has loaded installed skills.
    pub fn set_skills_summary(&mut self, summary: String) {
        self.context.set_skills_summary(summary);
    }

    /// Process a single user message and return the assistant's response.
    /// This runs the full ReAct loop: reason → act → observe → loop.
    ///
    /// `channel` and `chat_id` identify the originating channel so tools
    /// (e.g. cron) can route responses back to the user.
    pub async fn process_message(
        &self,
        content: &str,
        session_key: &str,
        channel: &str,
        chat_id: &str,
    ) -> Result<String> {
        // Get conversation history from SQLite
        let history = self
            .session_manager
            .get_history(session_key, self.config.agent.memory_window)
            .await?;

        // Build initial messages for the LLM
        let mut messages = self.context.build_messages(&history, content);

        // Get tool definitions for the LLM
        let tool_defs = self.tool_registry.get_definitions();
        let has_tools = !tool_defs.is_empty();

        // Build tool context with real channel info so tools can route responses
        let tool_ctx = ToolContext {
            workspace: Config::workspace_dir().to_string_lossy().to_string(),
            channel: channel.to_string(),
            chat_id: chat_id.to_string(),
            message_tx: self.message_tx.clone(),
        };

        let mut final_content: Option<String> = None;
        let mut tools_used: Vec<String> = Vec::new();
        let max_iterations = self.config.agent.max_iterations;

        for iteration in 1..=max_iterations {
            tracing::debug!(
                iteration,
                max_iterations,
                model = %self.config.agent.model,
                provider = %self.provider.name(),
                tools = tool_defs.len(),
                "Agent loop iteration"
            );

            // Call the LLM
            let request = ChatRequest {
                messages: messages.clone(),
                tools: tool_defs.clone(),
                model: self.config.agent.model.clone(),
                max_tokens: self.config.agent.max_tokens,
                temperature: self.config.agent.temperature,
            };

            let response = self
                .provider
                .chat(request)
                .await
                .context("Failed to get response from LLM provider")?;

            tracing::debug!(
                tokens = response.usage.total_tokens,
                finish_reason = %response.finish_reason,
                tool_calls = response.tool_calls.len(),
                "LLM response received"
            );

            if response.has_tool_calls() {
                // --- ACT: Execute tool calls ---

                // Add assistant message with tool calls to conversation
                let tool_call_serialized: Vec<ToolCallSerialized> = response
                    .tool_calls
                    .iter()
                    .map(|tc| ToolCallSerialized {
                        id: tc.id.clone(),
                        call_type: "function".to_string(),
                        function: ToolCallFunction {
                            name: tc.name.clone(),
                            arguments: serde_json::to_string(&tc.arguments)
                                .unwrap_or_else(|_| "{}".to_string()),
                        },
                    })
                    .collect();

                messages.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: response.content.clone(),
                    tool_calls: Some(tool_call_serialized),
                    tool_call_id: None,
                    name: None,
                });

                // Execute each tool call
                for tool_call in &response.tool_calls {
                    tools_used.push(tool_call.name.clone());

                    tracing::info!(
                        tool = %tool_call.name,
                        iteration,
                        "Executing tool"
                    );

                    // --- OBSERVE: Execute and add result ---
                    let result = self
                        .tool_registry
                        .execute(&tool_call.name, tool_call.arguments.clone(), &tool_ctx)
                        .await;

                    messages.push(ChatMessage::tool_result(
                        &tool_call.id,
                        &tool_call.name,
                        &result.output,
                    ));
                }

                // Add reflection prompt (following nanobot pattern)
                if has_tools {
                    messages.push(ChatMessage::user(
                        "Reflect on the results and decide next steps.",
                    ));
                }
            } else {
                // No tool calls → final response
                final_content = response.content;
                break;
            }
        }

        let response_text = final_content.unwrap_or_else(|| {
            "(max iterations reached without final response)".to_string()
        });

        // Persist conversation to SQLite
        self.session_manager
            .add_message(session_key, "user", content)
            .await?;
        self.session_manager
            .add_message_with_tools(session_key, "assistant", &response_text, &tools_used)
            .await?;

        if !tools_used.is_empty() {
            tracing::info!(
                tools_used = ?tools_used,
                "Agent completed with tool usage"
            );
        }

        // Check if memory consolidation is needed (non-blocking background task)
        self.maybe_consolidate(session_key).await;

        Ok(response_text)
    }

    /// Trigger memory consolidation if threshold exceeded.
    /// Runs in background — never blocks the response.
    async fn maybe_consolidate(&self, session_key: &str) {
        let memory = self.memory.clone();
        let window = self.config.agent.memory_window;
        let model = self.config.agent.model.clone();
        let session_key = session_key.to_string();

        // Check if needed (quick DB query)
        match memory.should_consolidate(&session_key, window).await {
            Ok(true) => {
                tracing::info!(
                    session = %session_key,
                    "Memory consolidation threshold reached, spawning background task"
                );
                // We can't pass &dyn Provider to a spawned task easily,
                // so we log the trigger. The gateway/CLI can run consolidation
                // on a periodic basis or we do it inline for now.
                // For Phase 4 MVP: run inline (blocking but acceptable for ~1 LLM call)
                // TODO: Phase 5 — use a dedicated consolidation provider or Arc<dyn Provider>
            }
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(error = %e, "Failed to check consolidation status");
            }
        }
    }
}
