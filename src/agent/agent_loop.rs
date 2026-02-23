use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context as _, Result};
use tokio::sync::{mpsc, RwLock};

use crate::bus::OutboundMessage;
use crate::config::Config;
use crate::provider::{
    ChatMessage, ChatRequest, Provider, ToolCallFunction, ToolCallSerialized,
};
use crate::provider::xml_dispatcher;
use crate::session::SessionManager;
use crate::storage::Database;
use crate::skills::loader::SkillRegistry;
use crate::tools::{ToolContext, ToolRegistry};

use super::context::ContextBuilder;
use super::memory::MemoryConsolidator;
use super::memory_search::MemorySearcher;
use super::verifier::{verify_actions, VerificationResult};


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
    provider: Arc<dyn Provider>,
    config: Config,
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
    memory_searcher: Option<Arc<tokio::sync::Mutex<MemorySearcher>>>,
    /// Set to true when the model doesn't support native function calling.
    /// Auto-detected on first error — tools are then injected into the system
    /// prompt as XML and parsed from the LLM's text response.
    use_xml_dispatch: AtomicBool,
}

impl AgentLoop {
    pub fn new(
        provider: Arc<dyn Provider>,
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

        // Determine if XML tool dispatch should be used
        // This considers: provider setting > global setting > auto-detect for Ollama
        let model = &config.agent.model;
        let provider_name = config.resolve_provider(model)
            .map(|(name, _)| name)
            .unwrap_or("unknown");
        
        let use_xml_dispatch = config.should_use_xml_dispatch(provider_name, model);
        
        if use_xml_dispatch {
            tracing::info!(
                model = %model,
                provider = %provider_name,
                "Using XML tool dispatch mode (provider/model specific or auto-detected)"
            );
        }

        Self {
            provider,
            config,
            context,
            session_manager,
            tool_registry,
            memory,
            message_tx: None,
            skill_registry: None,
            memory_searcher: None,
            use_xml_dispatch: AtomicBool::new(use_xml_dispatch),
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
    pub fn bootstrap_handles(&self) -> (Arc<RwLock<String>>, Arc<RwLock<Vec<(String, String)>>>) {
        self.context.bootstrap_handles()
    }

    /// Set the memory searcher for vector + FTS5 hybrid search.
    /// When set, each user message triggers a search for relevant past memories
    /// that are injected into the system prompt as Layer 3.5.
    pub fn set_memory_searcher(&mut self, searcher: MemorySearcher) {
        self.memory_searcher = Some(Arc::new(tokio::sync::Mutex::new(searcher)));
    }

    /// Inject available channels info for cross-channel messaging.
    /// Called after building the channel list so the agent knows where it can send.
    pub fn set_channels_info(&mut self, channels: &[(&str, &str)]) {
        self.context.set_channels_info(channels);
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
        self.process_message_inner(content, session_key, channel, chat_id, None).await
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
        self.process_message_inner(content, session_key, channel, chat_id, Some(stream_tx)).await
    }

    async fn process_message_inner(
        &self,
        content: &str,
        session_key: &str,
        channel: &str,
        chat_id: &str,
        stream_tx: Option<mpsc::Sender<crate::provider::StreamChunk>>,
    ) -> Result<String> {
        // Get conversation history from SQLite
        let history = self
            .session_manager
            .get_history(session_key, self.config.agent.memory_window)
            .await?;

        // Search for relevant past memories and inject into context (Layer 3.5)
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

        let has_tools = !tool_defs.is_empty();
        let xml_mode = self.use_xml_dispatch.load(Ordering::Relaxed);

        // Convert tool definitions to ToolInfo for the new prompt system
        let tool_infos: Vec<crate::agent::ToolInfo> = if xml_mode && has_tools {
            tool_defs.iter().map(|td| crate::agent::ToolInfo {
                name: td.function.name.clone(),
                description: td.function.description.clone(),
                parameters_schema: td.function.parameters.clone(),
            }).collect()
        } else {
            Vec::new()
        };

        // Build messages with tools integrated into the prompt (for XML mode)
        // or without tools (for native tool calling mode)
        let mut messages = if xml_mode {
            self.context.build_messages_with_tools(&history, content, &tool_infos).await
        } else {
            self.context.build_messages(&history, content).await
        };

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
                xml_mode,
                "Agent loop iteration"
            );

            // Call the LLM — in XML mode we don't send tool defs via the API.
            //
            // Streaming strategy: when stream_tx is available, always use
            // chat_stream. It handles both text and tool calls in the SSE
            // stream. Text deltas are forwarded to the client in real-time;
            // tool call deltas are accumulated and returned in ChatResponse.
            let api_tools = if xml_mode { Vec::new() } else { tool_defs.clone() };
            let use_streaming = stream_tx.is_some();

            let request = ChatRequest {
                messages: messages.clone(),
                tools: api_tools,
                model: self.config.agent.model.clone(),
                max_tokens: self.config.agent.max_tokens,
                temperature: self.config.agent.temperature,
            };

            tracing::info!(
                provider = %self.provider.name(),
                model = %self.config.agent.model,
                streaming = use_streaming,
                "Calling LLM provider"
            );

            let response = if use_streaming {
                let tx = stream_tx.as_ref().unwrap().clone();
                match self.provider.chat_stream(request, tx).await {
                    Ok(r) => r,
                    Err(e) => {
                        // Fallback: if streaming fails, try non-streaming
                        tracing::warn!(error = ?e, "Streaming failed, falling back to non-streaming");
                        let request2 = ChatRequest {
                            messages: messages.clone(),
                            tools: if xml_mode { Vec::new() } else { tool_defs.clone() },
                            model: self.config.agent.model.clone(),
                            max_tokens: self.config.agent.max_tokens,
                            temperature: self.config.agent.temperature,
                        };
                        self.provider.chat(request2).await
                            .context("Non-streaming fallback also failed")?
                    }
                }
            } else {
                match self.provider.chat(request).await {
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

                                // Inject tools into system prompt for XML mode
                                let tools_prompt = xml_dispatcher::build_tools_prompt(&tool_defs);
                                if let Some(system_msg) = messages.first_mut() {
                                    if system_msg.role == "system" {
                                        if let Some(ref mut content) = system_msg.content {
                                            content.push_str(&tools_prompt);
                                        }
                                    }
                                }

                                // Retry without tool defs
                                let retry_request = ChatRequest {
                                    messages: messages.clone(),
                                    tools: Vec::new(),
                                    model: self.config.agent.model.clone(),
                                    max_tokens: self.config.agent.max_tokens,
                                    temperature: self.config.agent.temperature,
                                };
                                self.provider
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
                            content: if clean_text.is_empty() { None } else { Some(clean_text) },
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
                    tools_used.push(tool_call.name.clone());

                    tracing::info!(
                        tool = %tool_call.name,
                        iteration,
                        "Executing tool"
                    );

                    // Notify frontend that a tool is being called
                    if let Some(ref tx) = stream_tx {
                        let _ = tx.send(crate::provider::StreamChunk {
                            delta: tool_call.name.clone(),
                            done: false,
                            event_type: Some("tool_start".to_string()),
                            tool_call_data: Some(crate::provider::ToolCallData {
                                id: tool_call.id.clone(),
                                name: tool_call.name.clone(),
                                arguments: tool_call.arguments.clone(),
                            }),
                        }).await;
                    }

                    // --- OBSERVE: Execute and add result ---
                    // First check if this is a registered tool; if not, check
                    // if it matches an installed skill (on-demand body loading).
                    let result = if self.tool_registry.get(&tool_call.name).is_some() {
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
                        let _ = tx.send(crate::provider::StreamChunk {
                            delta: tool_call.name.clone(),
                            done: false,
                            event_type: Some("tool_end".to_string()),
                            tool_call_data: None,
                        }).await;
                    }

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
                // No tool calls → check for hallucination before accepting as final
                let response_text = response.content.clone().unwrap_or_default();
                
                // Verify that claimed actions were actually executed
                match verify_actions(&response_text, &tools_used) {
                    VerificationResult::Verified => {
                        // All good — this is the final response
                        if !use_streaming {
                            if let Some(ref tx) = stream_tx {
                                if let Some(ref text) = response.content {
                                    let _ = tx.send(crate::provider::StreamChunk {
                                        delta: text.clone(),
                                        done: true,
                                        event_type: None,
                                        tool_call_data: None,
                                    }).await;
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

    /// Trigger memory consolidation if threshold exceeded.
    /// Runs in background via `tokio::spawn` — never blocks the response.
    /// After consolidation, new chunks are indexed in the HNSW vector index.
    async fn maybe_consolidate(&self, session_key: &str) {
        let memory = self.memory.clone();
        let window = self.config.agent.consolidation_threshold;
        let model = self.config.agent.model.clone();
        let provider = self.provider.clone();
        let session_key = session_key.to_string();
        let searcher = self.memory_searcher.clone();

        // Check if needed (quick DB query)
        match memory.should_consolidate(&session_key, window).await {
            Ok(true) => {
                tracing::info!(
                    session = %session_key,
                    "Memory consolidation threshold reached, spawning background task"
                );
                tokio::spawn(async move {
                    match memory.consolidate(&session_key, window, provider.as_ref(), &model).await {
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
                            if !result.new_chunks.is_empty() {
                                if let Some(searcher_mutex) = searcher {
                                    let mut s = searcher_mutex.lock().await;
                                    let mut indexed = 0;
                                    let mut skipped = 0;

                                    for (chunk_id, text) in &result.new_chunks {
                                        // Check for duplicates before indexing
                                        // Distance threshold 0.15 ≈ 85% cosine similarity
                                        match s.engine_mut().find_similar(text, 0.15) {
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

                                        if let Err(e) = s.engine_mut().index_chunk(*chunk_id, text) {
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
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(error = %e, "Failed to check consolidation status");
            }
        }
    }
}
