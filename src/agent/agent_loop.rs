use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context as _, Result};
use tokio::sync::{mpsc, RwLock};

use crate::bus::OutboundMessage;
use crate::config::Config;
use crate::provider::xml_dispatcher;
use crate::provider::{
    ChatMessage, ChatRequest, Provider, ToolCallFunction, ToolCallSerialized, Usage,
};
use crate::security::{redact, redact_vault_values};
use crate::session::SessionManager;
use crate::skills::{loader::SkillRegistry, suggest_mcp_presets, McpServerPreset};
use crate::storage::Database;
use crate::tools::{ToolContext, ToolRegistry};

use super::context::ContextBuilder;
use super::memory::MemoryConsolidator;
use super::verifier::{verify_actions, VerificationResult};

// Conditional memory searcher type - dummy when feature not enabled
#[cfg(feature = "local-embeddings")]
use super::memory_search::MemorySearcher;

#[cfg(not(feature = "local-embeddings"))]
struct MemorySearcher;

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
    /// Current LLM provider — rebuilt lazily when `config.agent.model` changes.
    /// Wrapped in RwLock so we can swap it at runtime without &mut self.
    provider: RwLock<Arc<dyn Provider>>,
    /// The model string the current provider was built for.
    /// Compared against `config.agent.model` on each request to detect changes.
    provider_model: RwLock<String>,
    config: Arc<RwLock<Config>>,
    context: ContextBuilder,
    session_manager: SessionManager,
    tool_registry: ToolRegistry,
    memory: Arc<MemoryConsolidator>,
    /// Sender for proactive messages (set in Gateway mode)
    message_tx: Option<mpsc::Sender<OutboundMessage>>,
    /// Shared skill registry for on-demand skill body loading
    skill_registry: Option<Arc<RwLock<SkillRegistry>>>,
    /// Optional memory searcher for retrieving relevant past context.
    /// Arc-wrapped so it can be shared with background consolidation tasks.
    /// Only functional with `local-embeddings` feature - dummy otherwise.
    memory_searcher: Option<Arc<tokio::sync::Mutex<MemorySearcher>>>,
    /// Set to true when the model doesn't support native function calling.
    /// Auto-detected on first error — tools are then injected into the system
    /// prompt as XML and parsed from the LLM's text response.
    use_xml_dispatch: AtomicBool,
    /// Database handle for token usage tracking.
    db: Database,
}

impl AgentLoop {
    pub async fn new(
        provider: Arc<dyn Provider>,
        config: Arc<RwLock<Config>>,
        session_manager: SessionManager,
        tool_registry: ToolRegistry,
        db: Database,
    ) -> Self {
        let cfg = config.read().await;
        let mut context = ContextBuilder::new(&cfg);
        let memory = Arc::new(MemoryConsolidator::new(db.clone()));

        // Inject long-term memory into context if MEMORY.md exists
        if let Some(memory_content) = memory.load_memory_md() {
            context.set_memory(memory_content);
            tracing::info!("Loaded long-term memory into context");
        }

        // Determine if XML tool dispatch should be used
        // This considers: provider setting > global setting > auto-detect for Ollama
        let model = &cfg.agent.model;
        let provider_name = cfg
            .resolve_provider(model)
            .map(|(name, _)| name)
            .unwrap_or("unknown");

        let use_xml_dispatch = cfg.should_use_xml_dispatch(provider_name, model);

        if use_xml_dispatch {
            tracing::info!(
                model = %model,
                provider = %provider_name,
                "Using XML tool dispatch mode (provider/model specific or auto-detected)"
            );
        }
        let initial_model = cfg.agent.model.clone();
        drop(cfg); // release lock before storing

        Self {
            provider: RwLock::new(provider),
            provider_model: RwLock::new(initial_model),
            config,
            context,
            session_manager,
            tool_registry,
            memory,
            message_tx: None,
            skill_registry: None,
            memory_searcher: None,
            use_xml_dispatch: AtomicBool::new(use_xml_dispatch),
            db,
        }
    }

    /// Set the outbound message sender for proactive messaging (MessageTool).
    /// Called by the Gateway after constructing the routing table.
    pub fn set_message_tx(&mut self, tx: mpsc::Sender<OutboundMessage>) {
        self.message_tx = Some(tx);
    }

    /// Inject skills summary into the system prompt.
    /// Called after SkillRegistry::scan_and_load() has loaded installed skills.
    pub async fn set_skills_summary(&self, summary: String) {
        self.context.set_skills_summary(summary).await;
    }

    /// Set the shared skill registry for on-demand skill body loading.
    /// When the LLM calls a "tool" that matches a skill name, the agent loop
    /// loads the full SKILL.md body and returns it as the tool result instead
    /// of returning "Unknown tool".
    pub fn set_skill_registry(&mut self, registry: Arc<RwLock<SkillRegistry>>) {
        self.skill_registry = Some(registry);
    }

    /// Get a shared handle to the skills summary for hot-reload updates.
    /// The skill watcher can write to this handle, and the next
    /// `build_system_prompt()` call will pick up the changes.
    pub fn skills_summary_handle(&self) -> Arc<RwLock<String>> {
        self.context.skills_summary_handle()
    }

    /// Get a shared handle to the bootstrap content for hot-reload updates.
    /// The bootstrap watcher can write to this handle, and the next
    /// `build_system_prompt()` call will pick up the changes.
    pub fn bootstrap_content_handle(&self) -> Arc<RwLock<String>> {
        self.context.bootstrap_content_handle()
    }

    /// Get both bootstrap handles for the BootstrapWatcher.
    /// Returns (legacy_content, new_files) handles.
    #[allow(clippy::type_complexity)]
    pub fn bootstrap_handles(&self) -> (Arc<RwLock<String>>, Arc<RwLock<Vec<(String, String)>>>) {
        self.context.bootstrap_handles()
    }

    /// Set the memory searcher for vector + FTS5 hybrid search.
    /// When set, each user message triggers a search for relevant past memories
    /// that are injected into the system prompt as Layer 3.5.
    /// Only available with `local-embeddings` feature.
    #[cfg(feature = "local-embeddings")]
    pub fn set_memory_searcher(&mut self, searcher: MemorySearcher) {
        self.memory_searcher = Some(Arc::new(tokio::sync::Mutex::new(searcher)));
    }

    /// Get a clone of the shared memory searcher handle (for sharing with the web server).
    #[cfg(feature = "local-embeddings")]
    pub fn memory_searcher_handle(&self) -> Option<Arc<tokio::sync::Mutex<MemorySearcher>>> {
        self.memory_searcher.clone()
    }

    /// Set registered tool names so the system prompt can include routing rules
    /// even in native function calling mode (where ctx.tools is empty).
    pub fn set_registered_tool_names(&mut self, names: Vec<String>) {
        self.context.set_registered_tool_names(names);
    }

    /// Inject available channels info for cross-channel messaging.
    /// Called after building the channel list so the agent knows where it can send.
    pub fn set_channels_info(&mut self, channels: &[(&str, &str)]) {
        self.context.set_channels_info(channels);
    }

    /// Inject email account details (name + mode) into the system prompt.
    pub fn set_email_accounts_info(&mut self, accounts: &[(String, crate::config::EmailMode)]) {
        self.context.set_email_accounts_info(accounts);
    }

    /// Process a single user message and return the assistant's response.
    /// This runs the full ReAct loop: reason → act → observe → loop.
    ///
    /// `channel` and `chat_id` identify the originating channel so tools
    /// (e.g. cron) can route responses back to the user.
    ///
    /// If `stream_tx` is provided, intermediate text chunks are sent as they
    /// arrive from the LLM (only for the final response, not during tool execution).
    pub async fn process_message(
        &self,
        content: &str,
        session_key: &str,
        channel: &str,
        chat_id: &str,
    ) -> Result<String> {
        self.process_message_inner(content, session_key, channel, chat_id, None, &[])
            .await
    }

    /// Process a message while disabling a subset of tools for this request.
    /// Useful for constrained contexts such as automation runs.
    pub async fn process_message_with_blocked_tools(
        &self,
        content: &str,
        session_key: &str,
        channel: &str,
        chat_id: &str,
        blocked_tools: &[&str],
    ) -> Result<String> {
        self.process_message_inner(content, session_key, channel, chat_id, None, blocked_tools)
            .await
    }

    /// Process a message with optional streaming output.
    /// Streaming chunks are sent to `stream_tx` if provided.
    pub async fn process_message_streaming(
        &self,
        content: &str,
        session_key: &str,
        channel: &str,
        chat_id: &str,
        stream_tx: mpsc::Sender<crate::provider::StreamChunk>,
    ) -> Result<String> {
        self.process_message_inner(content, session_key, channel, chat_id, Some(stream_tx), &[])
            .await
    }

    /// Streaming variant with per-request blocked tools.
    pub async fn process_message_streaming_with_blocked_tools(
        &self,
        content: &str,
        session_key: &str,
        channel: &str,
        chat_id: &str,
        stream_tx: mpsc::Sender<crate::provider::StreamChunk>,
        blocked_tools: &[&str],
    ) -> Result<String> {
        self.process_message_inner(
            content,
            session_key,
            channel,
            chat_id,
            Some(stream_tx),
            blocked_tools,
        )
        .await
    }

    async fn process_message_inner(
        &self,
        content: &str,
        session_key: &str,
        channel: &str,
        chat_id: &str,
        stream_tx: Option<mpsc::Sender<crate::provider::StreamChunk>>,
        blocked_tools: &[&str],
    ) -> Result<String> {
        crate::agent::stop::clear_stop();
        let blocked_set: HashSet<&str> = blocked_tools.iter().copied().collect();

        // Snapshot config for this request — picks up any changes from web UI.
        // Clone + drop lock immediately to avoid holding across LLM calls.
        let config = self.config.read().await.clone();

        // Lazy provider rebuild: if the model changed (e.g. user switched model
        // in the web UI), recreate the entire provider chain so the correct
        // backend (Anthropic, OpenAI-compat, Ollama) is used.
        {
            let current_model = self.provider_model.read().await;
            if *current_model != config.agent.model {
                let new_model = config.agent.model.clone();
                drop(current_model); // release read lock before acquiring write

                match crate::provider::create_provider(&config) {
                    Ok(new_provider) => {
                        *self.provider.write().await = new_provider;
                        *self.provider_model.write().await = new_model.clone();

                        // Also re-evaluate XML dispatch mode for the new model
                        let provider_name = config
                            .resolve_provider(&new_model)
                            .map(|(name, _)| name)
                            .unwrap_or("unknown");
                        let xml = config.should_use_xml_dispatch(provider_name, &new_model);
                        self.use_xml_dispatch.store(xml, Ordering::Relaxed);

                        // Update model name in system prompt so LLM knows its identity
                        self.context.set_model_name(new_model.clone()).await;

                        tracing::info!(
                            model = %new_model,
                            provider = %provider_name,
                            xml_dispatch = xml,
                            "Provider rebuilt for new model"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            model = %config.agent.model,
                            "Failed to rebuild provider for new model — using previous provider"
                        );
                    }
                }
            }
        }

        // Get the current provider for this request (clone the Arc, release lock)
        let provider = self.provider.read().await.clone();

        // Get conversation history from SQLite
        let history = self
            .session_manager
            .get_history(session_key, config.agent.memory_window)
            .await?;

        // Search for relevant past memories and inject into context (Layer 3.5)
        // Only available with local-embeddings feature
        #[cfg(feature = "local-embeddings")]
        if let Some(ref searcher_mutex) = self.memory_searcher {
            let mut searcher = searcher_mutex.lock().await;
            match searcher.search(content, 5).await {
                Ok(results) if !results.is_empty() => {
                    let memories_text = results
                        .iter()
                        .map(|r| {
                            format!(
                                "- [{}] {}: {}",
                                r.chunk.date, r.chunk.memory_type, r.chunk.content
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.context.set_relevant_memories(memories_text).await;
                    tracing::debug!(
                        results = results.len(),
                        "Injected relevant memories into context"
                    );
                }
                Ok(_) => {
                    self.context.set_relevant_memories(String::new()).await;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Memory search failed, continuing without");
                    self.context.set_relevant_memories(String::new()).await;
                }
            }
        }

        self.context
            .set_mcp_suggestions(build_mcp_suggestions(&config, content))
            .await;

        // Build initial messages for the LLM
        // Get tool definitions for the LLM (built-in tools + skills as tools)
        let mut tool_defs = self.tool_registry.get_definitions();

        // Register installed skills as tool definitions so the LLM can call them.
        // Each skill becomes a callable tool with a `query` parameter.
        if let Some(registry) = &self.skill_registry {
            let guard = registry.read().await;
            for (name, desc) in guard.list() {
                tool_defs.push(crate::provider::ToolDefinition {
                    tool_type: "function".to_string(),
                    function: crate::provider::FunctionDefinition {
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

        if !blocked_set.is_empty() {
            tool_defs.retain(|td| !blocked_set.contains(td.function.name.as_str()));
        }

        let has_tools = !tool_defs.is_empty();
        let xml_mode = self.use_xml_dispatch.load(Ordering::Relaxed);

        // Convert tool definitions to ToolInfo for the new prompt system
        let tool_infos: Vec<crate::agent::ToolInfo> = if xml_mode && has_tools {
            tool_defs
                .iter()
                .map(|td| crate::agent::ToolInfo {
                    name: td.function.name.clone(),
                    description: td.function.description.clone(),
                    parameters_schema: td.function.parameters.clone(),
                })
                .collect()
        } else {
            Vec::new()
        };

        // Build messages with tools integrated into the prompt (for XML mode)
        // or without tools (for native tool calling mode)
        let mut messages = if xml_mode {
            self.context
                .build_messages_with_tools(&history, content, &tool_infos)
                .await
        } else {
            self.context.build_messages(&history, content).await
        };

        // Build tool context with real channel info so tools can route responses
        let tool_ctx = ToolContext {
            workspace: Config::workspace_dir().to_string_lossy().to_string(),
            channel: channel.to_string(),
            chat_id: chat_id.to_string(),
            message_tx: self.message_tx.clone(),
            approval_manager: crate::tools::global_approval_manager(),
        };

        let mut final_content: Option<String> = None;
        let mut tools_used: Vec<String> = Vec::new();
        let mut total_usage = Usage::default();
        let max_iterations = config.agent.max_iterations;

        'agent_loop: for iteration in 1..=max_iterations {
            if crate::agent::stop::is_stop_requested() {
                final_content = Some("Stopped by user.".to_string());
                break;
            }
            tracing::debug!(
                iteration,
                max_iterations,
                model = %config.agent.model,
                provider = %provider.name(),
                tools = tool_defs.len(),
                xml_mode,
                "Agent loop iteration"
            );

            // Call the LLM — in XML mode we don't send tool defs via the API.
            //
            // Streaming strategy: when stream_tx is available, always use
            // chat_stream. It handles both text and tool calls in the SSE
            // stream. Text deltas are forwarded to the client in real-time;
            // tool call deltas are accumulated and returned in ChatResponse.
            let api_tools = if xml_mode {
                Vec::new()
            } else {
                tool_defs.clone()
            };
            let use_streaming = stream_tx.is_some();

            let active_model = &config.agent.model;
            let request = ChatRequest {
                messages: messages.clone(),
                tools: api_tools,
                model: active_model.clone(),
                max_tokens: config.agent.effective_max_tokens(active_model),
                temperature: config.agent.effective_temperature(active_model),
            };

            // Estimate context size for debugging
            let ctx_chars: usize = messages
                .iter()
                .map(|m| m.content.as_ref().map_or(0, |c| c.len()))
                .sum();
            let ctx_msgs = messages.len();
            tracing::info!(
                provider = %provider.name(),
                model = %config.agent.model,
                streaming = use_streaming,
                context_chars = ctx_chars,
                messages = ctx_msgs,
                iteration,
                "Calling LLM provider"
            );

            let response = if use_streaming {
                let tx = stream_tx.as_ref().unwrap().clone();
                match provider.chat_stream(request, tx).await {
                    Ok(r) => r,
                    Err(e) => {
                        // Check if the model rejected tool use — if so, switch to XML dispatch
                        let err_lower = e.to_string().to_lowercase();
                        let tool_rejected = !xml_mode
                            && has_tools
                            && iteration == 1
                            && (err_lower.contains("tool")
                                || err_lower.contains("function")
                                || err_lower.contains("not supported")
                                || err_lower.contains("no endpoints"));

                        if tool_rejected {
                            tracing::info!(
                                "Model rejected tool use (streaming), switching to XML dispatch mode"
                            );
                            self.use_xml_dispatch.store(true, Ordering::Relaxed);

                            // Brief delay before retrying to avoid hitting rate limits
                            // (especially on free-tier models with aggressive limits)
                            let delay_ms = config.agent.xml_fallback_delay_ms;
                            if delay_ms > 0 {
                                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms))
                                    .await;
                            }

                            let xml_tool_infos: Vec<crate::agent::ToolInfo> = tool_defs
                                .iter()
                                .map(|td| crate::agent::ToolInfo {
                                    name: td.function.name.clone(),
                                    description: td.function.description.clone(),
                                    parameters_schema: td.function.parameters.clone(),
                                })
                                .collect();
                            messages = self
                                .context
                                .build_messages_with_tools(&history, content, &xml_tool_infos)
                                .await;

                            let retry_request = ChatRequest {
                                messages: messages.clone(),
                                tools: Vec::new(),
                                model: active_model.clone(),
                                max_tokens: config.agent.effective_max_tokens(active_model),
                                temperature: config.agent.effective_temperature(active_model),
                            };
                            provider
                                .chat(retry_request)
                                .await
                                .context("XML dispatch fallback also failed")?
                        } else {
                            // Regular streaming failure — try non-streaming with same tools
                            tracing::warn!(error = ?e, "Streaming failed, falling back to non-streaming");
                            let request2 = ChatRequest {
                                messages: messages.clone(),
                                tools: if xml_mode {
                                    Vec::new()
                                } else {
                                    tool_defs.clone()
                                },
                                model: active_model.clone(),
                                max_tokens: config.agent.effective_max_tokens(active_model),
                                temperature: config.agent.effective_temperature(active_model),
                            };
                            provider
                                .chat(request2)
                                .await
                                .context("Non-streaming fallback also failed")?
                        }
                    }
                }
            } else {
                match provider.chat(request).await {
                    Ok(r) => r,
                    Err(e) => {
                        // Auto-detect: if the model rejects tool specs,
                        // switch to XML dispatch mode and retry this iteration
                        if !xml_mode && has_tools && iteration == 1 {
                            let err_msg = e.to_string().to_lowercase();
                            if err_msg.contains("tool")
                                || err_msg.contains("function")
                                || err_msg.contains("not supported")
                                || err_msg.contains("invalid")
                            {
                                tracing::info!(
                                    "Model rejected native tool calling, switching to XML dispatch mode"
                                );
                                self.use_xml_dispatch.store(true, Ordering::Relaxed);

                                // Brief delay before retrying to avoid hitting rate limits
                                let delay_ms = config.agent.xml_fallback_delay_ms;
                                if delay_ms > 0 {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(
                                        delay_ms,
                                    ))
                                    .await;
                                }

                                // Rebuild system prompt from scratch with tools in XML mode.
                                // The previous prompt said "No tools" — we need a clean rebuild.
                                let xml_tool_infos: Vec<crate::agent::ToolInfo> = tool_defs
                                    .iter()
                                    .map(|td| crate::agent::ToolInfo {
                                        name: td.function.name.clone(),
                                        description: td.function.description.clone(),
                                        parameters_schema: td.function.parameters.clone(),
                                    })
                                    .collect();
                                messages = self
                                    .context
                                    .build_messages_with_tools(&history, content, &xml_tool_infos)
                                    .await;

                                let retry_request = ChatRequest {
                                    messages: messages.clone(),
                                    tools: Vec::new(),
                                    model: active_model.clone(),
                                    max_tokens: config.agent.effective_max_tokens(active_model),
                                    temperature: config.agent.effective_temperature(active_model),
                                };
                                provider
                                    .chat(retry_request)
                                    .await
                                    .context("Failed to get response from LLM (XML mode retry)")?
                            } else {
                                return Err(e.context("Failed to get response from LLM provider"));
                            }
                        } else {
                            return Err(e.context("Failed to get response from LLM provider"));
                        }
                    }
                }
            };

            tracing::debug!(
                tokens = response.usage.total_tokens,
                finish_reason = %response.finish_reason,
                tool_calls = response.tool_calls.len(),
                "LLM response received"
            );

            // Accumulate token usage across iterations
            total_usage.prompt_tokens += response.usage.prompt_tokens;
            total_usage.completion_tokens += response.usage.completion_tokens;
            total_usage.total_tokens += response.usage.total_tokens;

            // In XML mode, parse tool calls from the text response
            let response = if self.use_xml_dispatch.load(Ordering::Relaxed) {
                if let Some(ref text) = response.content {
                    let (clean_text, xml_calls) = xml_dispatcher::parse_tool_calls(text);
                    if !xml_calls.is_empty() {
                        tracing::info!(
                            count = xml_calls.len(),
                            "Parsed tool calls from XML in LLM response"
                        );
                        crate::provider::ChatResponse {
                            content: if clean_text.is_empty() {
                                None
                            } else {
                                Some(clean_text)
                            },
                            tool_calls: xml_calls,
                            finish_reason: "tool_calls".to_string(),
                            usage: response.usage,
                        }
                    } else {
                        response
                    }
                } else {
                    response
                }
            } else {
                response
            };

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
                    if crate::agent::stop::is_stop_requested() {
                        final_content = Some("Stopped by user.".to_string());
                        break 'agent_loop;
                    }
                    tools_used.push(tool_call.name.clone());

                    tracing::info!(
                        tool = %tool_call.name,
                        iteration,
                        "Executing tool"
                    );

                    // Notify frontend that a tool is being called
                    if let Some(ref tx) = stream_tx {
                        if let Err(e) = tx
                            .send(crate::provider::StreamChunk {
                                delta: tool_call.name.clone(),
                                done: false,
                                event_type: Some("tool_start".to_string()),
                                tool_call_data: Some(crate::provider::ToolCallData {
                                    id: tool_call.id.clone(),
                                    name: tool_call.name.clone(),
                                    arguments: tool_call.arguments.clone(),
                                }),
                            })
                            .await
                        {
                            tracing::warn!(error = %e, "Failed to send tool_start stream event");
                        }
                    }

                    // --- OBSERVE: Execute and add result ---
                    // First check if this is a registered tool; if not, check
                    // if it matches an installed skill (on-demand body loading).
                    let result = if blocked_set.contains(tool_call.name.as_str()) {
                        crate::tools::ToolResult::error(format!(
                            "Tool '{}' is disabled in this execution context.",
                            tool_call.name
                        ))
                    } else if self.tool_registry.get(&tool_call.name).is_some() {
                        self.tool_registry
                            .execute(&tool_call.name, tool_call.arguments.clone(), &tool_ctx)
                            .await
                    } else if let Some(body) = self.try_load_skill_body(&tool_call.name).await {
                        tracing::info!(
                            skill = %tool_call.name,
                            body_len = body.len(),
                            "Skill activated — returning SKILL.md body"
                        );
                        let output = format!(
                            "[SKILL INSTRUCTIONS — follow these steps to complete the task]\n\n{}\n\n\
                            [END SKILL INSTRUCTIONS — now execute the commands above using the shell tool to get the answer]",
                            body
                        );
                        crate::tools::ToolResult {
                            output,
                            is_error: false,
                        }
                    } else {
                        self.tool_registry
                            .execute(&tool_call.name, tool_call.arguments.clone(), &tool_ctx)
                            .await
                    };

                    // Notify frontend that tool execution finished
                    if let Some(ref tx) = stream_tx {
                        if let Err(e) = tx
                            .send(crate::provider::StreamChunk {
                                delta: tool_call.name.clone(),
                                done: false,
                                event_type: Some("tool_end".to_string()),
                                tool_call_data: None,
                            })
                            .await
                        {
                            tracing::warn!(error = %e, "Failed to send tool_end stream event");
                        }
                    }

                    messages.push(ChatMessage::tool_result(
                        &tool_call.id,
                        &tool_call.name,
                        &result.output,
                    ));

                    if crate::agent::stop::is_stop_requested() {
                        final_content = Some("Stopped by user.".to_string());
                        break 'agent_loop;
                    }
                }

                // No reflection prompt needed — modern LLMs reason about
                // tool results naturally without an extra user nudge.
            } else {
                // No tool calls → check for hallucination before accepting as final
                let response_text = response.content.clone().unwrap_or_default();

                // Verify that claimed actions were actually executed
                match verify_actions(&response_text, &tools_used) {
                    VerificationResult::Verified => {
                        // All good — this is the final response
                        if !use_streaming {
                            if let Some(ref tx) = stream_tx {
                                if let Some(ref text) = response.content {
                                    let _ = tx
                                        .send(crate::provider::StreamChunk {
                                            delta: text.clone(),
                                            done: true,
                                            event_type: None,
                                            tool_call_data: None,
                                        })
                                        .await;
                                }
                            }
                        }
                        final_content = response.content;
                        break;
                    }
                    VerificationResult::NeedsVerification {
                        claimed_action,
                        expected_tool,
                        verification_prompt,
                    } => {
                        // Hallucination detected! LLM claimed an action but didn't call the tool.
                        // Inject verification prompt and continue the loop.
                        tracing::warn!(
                            claimed_action = %claimed_action,
                            expected_tool = %expected_tool,
                            "LLM claimed action without calling tool — injecting verification prompt"
                        );

                        // Add the LLM's response to messages (so it sees what it said)
                        messages.push(ChatMessage::assistant(&response_text));

                        // Inject verification prompt
                        messages.push(ChatMessage::user(&verification_prompt));

                        // Continue the loop — LLM must now actually use the tool
                        continue;
                    }
                }
            }
        }

        if final_content.is_none() && !crate::agent::stop::is_stop_requested() {
            tracing::warn!(
                max_iterations,
                tools_used = tools_used.len(),
                "Max iterations reached without final response; attempting forced finalization"
            );

            let mut finalization_messages = messages.clone();
            finalization_messages.push(ChatMessage::user(
                "The tool and iteration budget is exhausted. Do not call any tools, browser actions, functions, or MCP integrations. Using only the evidence, tool outputs, and sources already collected in this conversation, provide the best possible final answer now. If the information is incomplete, clearly separate confirmed findings, likely but unconfirmed points, and remaining unknowns. Do not ask to continue browsing unless it is strictly necessary.",
            ));

            let finalization_request = ChatRequest {
                messages: finalization_messages,
                tools: Vec::new(),
                model: config.agent.model.clone(),
                max_tokens: config.agent.effective_max_tokens(&config.agent.model),
                temperature: config.agent.effective_temperature(&config.agent.model),
            };

            match provider.chat(finalization_request).await {
                Ok(response) => {
                    total_usage.prompt_tokens += response.usage.prompt_tokens;
                    total_usage.completion_tokens += response.usage.completion_tokens;
                    total_usage.total_tokens += response.usage.total_tokens;

                    if let Some(text) = response.content.filter(|text| !text.trim().is_empty()) {
                        tracing::info!(
                            finish_reason = %response.finish_reason,
                            "Forced finalization produced a final answer"
                        );
                        if let Some(ref tx) = stream_tx {
                            if let Err(e) = tx
                                .send(crate::provider::StreamChunk {
                                    delta: text.clone(),
                                    done: true,
                                    event_type: None,
                                    tool_call_data: None,
                                })
                                .await
                            {
                                tracing::warn!(
                                    error = %e,
                                    "Failed to send forced finalization stream chunk"
                                );
                            }
                        }
                        final_content = Some(text);
                    } else {
                        tracing::warn!(
                            finish_reason = %response.finish_reason,
                            tool_calls = response.tool_calls.len(),
                            "Forced finalization returned no usable content"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Forced finalization failed");
                }
            }
        }

        let response_text = final_content
            .unwrap_or_else(|| "(max iterations reached without final response)".to_string());

        // Apply exfiltration filter to prevent secret leaks in output
        // This scans the response for API keys, tokens, passwords, etc.
        // and redacts them before returning to the user.
        let mut safe_response = redact(&response_text);

        // Also redact any vault values that might have leaked into the response
        // This ensures that even if the LLM retrieved a vault value and tries to
        // output it, we catch it and replace with vault://key reference
        if let Ok(secrets) = crate::storage::global_secrets() {
            let vault_entries: Vec<(String, String)> = secrets
                .list_keys()
                .into_iter()
                .filter(|k| k.starts_with("vault."))
                .filter_map(|k| {
                    let short_key = k.strip_prefix("vault.")?.to_string();
                    let value = secrets.get(&crate::storage::SecretKey::custom(&k)).ok()??;
                    Some((short_key, value))
                })
                .collect();

            if !vault_entries.is_empty() {
                let redacted = redact_vault_values(&safe_response, &vault_entries);
                if redacted != safe_response {
                    tracing::info!(
                        redacted_count = vault_entries.len(),
                        "Redacted vault values from LLM output"
                    );
                    safe_response = redacted;
                }
            }
        }

        // Persist conversation to SQLite
        self.session_manager
            .add_message(session_key, "user", content)
            .await?;
        self.session_manager
            .add_message_with_tools(session_key, "assistant", &safe_response, &tools_used)
            .await?;

        if !tools_used.is_empty() {
            tracing::info!(
                tools_used = ?tools_used,
                "Agent completed with tool usage"
            );
        }

        // Auto-cleanup browser if it was used in this session
        // This ensures the browser process is closed even if the LLM forgot to call 'close'
        #[cfg(feature = "browser")]
        if tools_used.iter().any(|t| t == "browser") {
            let chat_id = chat_id.to_string();
            tokio::spawn(async move {
                let manager = crate::browser::global_browser_manager();
                if manager.is_running().await {
                    tracing::info!(chat_id = %chat_id, "Auto-cleaning up browser after task completion");
                    // Close all pages for this chat_id (across all profiles)
                    if let Err(e) = manager.close_page(&chat_id).await {
                        tracing::warn!(error = %e, "Failed to cleanup browser pages");
                    }
                }
            });
        }

        // Record token usage (fire-and-forget)
        if total_usage.total_tokens > 0 {
            let db = self.db.clone();
            let sk = session_key.to_string();
            let model = config.agent.model.clone();
            let prov = provider.name().to_string();
            tokio::spawn(async move {
                if let Err(e) = db
                    .insert_token_usage(
                        &sk,
                        &model,
                        &prov,
                        total_usage.prompt_tokens,
                        total_usage.completion_tokens,
                        total_usage.total_tokens,
                    )
                    .await
                {
                    tracing::warn!(error = %e, "Failed to record token usage");
                }
            });
        }

        // Check if memory consolidation is needed (non-blocking background task)
        self.maybe_consolidate(session_key).await;

        Ok(safe_response)
    }

    /// Try to load a skill's full SKILL.md body by name.
    /// Returns `Some(body)` if a matching skill is found, `None` otherwise.
    /// This enables "progressive disclosure": the system prompt only lists
    /// skill names + descriptions, and the full body is loaded on-demand
    /// when the LLM decides to activate a skill.
    async fn try_load_skill_body(&self, name: &str) -> Option<String> {
        let registry = self.skill_registry.as_ref()?;
        let mut guard = registry.write().await;
        let skill = guard.get_mut(name)?;

        match skill.load_body().await {
            Ok(body) => Some(body.to_string()),
            Err(e) => {
                tracing::warn!(skill = %name, error = %e, "Failed to load skill body");
                None
            }
        }
    }

    /// Trigger memory consolidation and session compaction if thresholds exceeded.
    /// Runs in background via `tokio::spawn` — never blocks the response.
    /// After consolidation, new chunks are indexed in the HNSW vector index,
    /// then session compaction prunes old messages and inserts a summary.
    async fn maybe_consolidate(&self, session_key: &str) {
        let memory = self.memory.clone();
        let cfg = self.config.read().await;
        let window = cfg.agent.consolidation_threshold;
        let memory_window = cfg.agent.memory_window;
        let model = cfg.agent.model.clone();
        drop(cfg);
        let provider = self.provider.read().await.clone();
        let session_key = session_key.to_string();
        #[cfg(feature = "local-embeddings")]
        let searcher = self.memory_searcher.clone();

        // Check if consolidation is needed (quick DB query)
        match memory.should_consolidate(&session_key, window).await {
            Ok(true) => {
                tracing::info!(
                    session = %session_key,
                    "Memory consolidation threshold reached, spawning background task"
                );
                tokio::spawn(async move {
                    match memory
                        .consolidate(&session_key, window, provider.as_ref(), &model)
                        .await
                    {
                        Ok(result) => {
                            tracing::info!(
                                session = %session_key,
                                messages_processed = result.messages_processed,
                                memory_updated = result.memory_updated,
                                instructions = result.instructions_learned,
                                secrets = result.secrets_stored,
                                new_chunks = result.new_chunks.len(),
                                "Background memory consolidation complete"
                            );

                            // Index new chunks in HNSW vector index (with deduplication)
                            // Only available with local-embeddings feature
                            #[cfg(feature = "local-embeddings")]
                            if !result.new_chunks.is_empty() {
                                if let Some(searcher_mutex) = searcher {
                                    let mut s = searcher_mutex.lock().await;
                                    let mut indexed = 0;
                                    let mut skipped = 0;

                                    for (chunk_id, text) in &result.new_chunks {
                                        // Check for duplicates before indexing
                                        // Distance threshold 0.15 ≈ 85% cosine similarity
                                        match s.engine_mut().find_similar(text, 0.15).await {
                                            Ok(Some((existing_id, distance))) => {
                                                tracing::debug!(
                                                    chunk_id,
                                                    existing_id,
                                                    distance = format!("{:.3}", distance),
                                                    "Skipping duplicate memory chunk"
                                                );
                                                skipped += 1;
                                                continue;
                                            }
                                            Ok(None) => {} // No duplicate, proceed
                                            Err(e) => {
                                                tracing::warn!(
                                                    chunk_id,
                                                    error = %e,
                                                    "Failed to check for duplicates, indexing anyway"
                                                );
                                            }
                                        }

                                        if let Err(e) =
                                            s.engine_mut().index_chunk(*chunk_id, text).await
                                        {
                                            tracing::warn!(
                                                chunk_id,
                                                error = %e,
                                                "Failed to index chunk in HNSW"
                                            );
                                        } else {
                                            indexed += 1;
                                        }
                                    }

                                    if let Err(e) = s.save_index() {
                                        tracing::warn!(error = %e, "Failed to save HNSW index");
                                    }
                                    tracing::info!(
                                        total = result.new_chunks.len(),
                                        indexed,
                                        skipped,
                                        "Indexed memory chunks in HNSW (duplicates skipped)"
                                    );
                                }
                            }

                            // Session compaction: prune old messages after consolidation
                            Self::try_compact(
                                &memory,
                                &session_key,
                                memory_window,
                                provider.as_ref(),
                                &model,
                            )
                            .await;
                        }
                        Err(e) => {
                            tracing::error!(
                                session = %session_key,
                                error = %e,
                                "Background memory consolidation failed"
                            );
                        }
                    }
                });
            }
            Ok(false) => {
                // Consolidation not needed, but compaction might be
                // (e.g., many messages accumulated but consolidation already ran)
                let memory_c = memory.clone();
                let sk = session_key.clone();
                let prov = provider.clone();
                let m = model.clone();
                tokio::spawn(async move {
                    Self::try_compact(&memory_c, &sk, memory_window, prov.as_ref(), &m).await;
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to check consolidation status");
            }
        }
    }

    /// Try to compact session if message count exceeds memory_window.
    async fn try_compact(
        memory: &MemoryConsolidator,
        session_key: &str,
        memory_window: u32,
        provider: &dyn Provider,
        model: &str,
    ) {
        match memory.should_compact(session_key, memory_window).await {
            Ok(true) => {
                match memory
                    .compact_session(session_key, memory_window, provider, model)
                    .await
                {
                    Ok(r) => {
                        tracing::info!(
                            session = %session_key,
                            messages_removed = r.messages_removed,
                            summary_inserted = r.summary_inserted,
                            "Background session compaction complete"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            session = %session_key,
                            error = %e,
                            "Background session compaction failed"
                        );
                    }
                }
            }
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(error = %e, "Failed to check compaction status");
            }
        }
    }
}

fn build_mcp_suggestions(config: &Config, content: &str) -> String {
    let suggestions = suggest_mcp_presets(content);
    if suggestions.is_empty() {
        return String::new();
    }

    suggestions
        .into_iter()
        .filter(|preset| !has_configured_mcp_server(config, preset))
        .take(2)
        .map(|preset| {
            format!(
                "- {} (`{}`): suggest connecting it from the MCP page or with `homun mcp setup {}` if the user wants {}.",
                preset.display_name,
                preset.id,
                preset.id,
                mcp_user_value_hint(&preset)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn has_configured_mcp_server(config: &Config, preset: &McpServerPreset) -> bool {
    config.mcp.servers.iter().any(|(name, server)| {
        if !server.enabled {
            return false;
        }

        let searchable = format!(
            "{} {} {} {}",
            name,
            server.command.as_deref().unwrap_or_default(),
            server.args.join(" "),
            server.url.as_deref().unwrap_or_default()
        )
        .to_lowercase();

        searchable.contains(&preset.id.to_lowercase())
            || preset
                .aliases
                .iter()
                .any(|alias| searchable.contains(&alias.to_lowercase()))
    })
}

fn mcp_user_value_hint(preset: &McpServerPreset) -> &'static str {
    let id = preset.id.to_lowercase();
    if id.contains("gmail") {
        return "email access";
    }
    if id.contains("calendar") {
        return "calendar access";
    }
    if id.contains("github") {
        return "repository or issue access";
    }
    if id.contains("notion") {
        return "workspace or notes access";
    }
    if id.contains("fetch") {
        return "web page access";
    }
    if id.contains("filesystem") {
        return "local file access";
    }
    "that service"
}
