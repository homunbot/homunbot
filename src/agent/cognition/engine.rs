//! Cognition engine — mini ReAct loop with discovery tools.
//!
//! Runs before the main execution loop to understand intent,
//! discover resources, and build a targeted plan.

use std::sync::Arc;

use anyhow::{Context as _, Result};
use tokio::sync::{mpsc, RwLock};

use crate::config::Config;
use crate::provider::{
    ChatMessage, ChatRequest, ChatResponse, Provider, RequestPriority, StreamChunk,
};
use crate::skills::loader::SkillRegistry;
use crate::storage::Database;
use crate::tools::ToolRegistry;

use super::discovery;
use super::types::{validate_cognition_result, CognitionResult, ValidationIssue};

/// Maximum iterations for the cognition mini-loop.
const MAX_COGNITION_ITERATIONS: u32 = 4;

/// Default timeout for the entire cognition phase (seconds).
const DEFAULT_COGNITION_TIMEOUT_SECS: u64 = 15;

/// Parameters for running the cognition phase.
pub struct CognitionParams<'a> {
    pub user_prompt: &'a str,
    pub config: &'a Config,
    pub tool_registry: &'a RwLock<ToolRegistry>,
    pub skill_registry: Option<&'a RwLock<SkillRegistry>>,
    #[cfg(feature = "embeddings")]
    pub memory_searcher: Option<&'a Arc<tokio::sync::Mutex<crate::agent::memory_search::MemorySearcher>>>,
    #[cfg(feature = "embeddings")]
    pub rag_engine: Option<&'a Arc<tokio::sync::Mutex<crate::rag::RagEngine>>>,
    pub contact_summary: &'a str,
    pub channel: &'a str,
    pub agent_id: Option<&'a str>,
    pub contact_id: Option<i64>,
    /// Visible profile IDs for memory/RAG scoping (active + readable_from).
    pub visible_profile_ids: Vec<i64>,
    /// Active profile slug for skill filtering.
    pub active_profile_slug: Option<String>,
    pub stream_tx: Option<&'a mpsc::Sender<StreamChunk>>,
    pub cognition_model: Option<&'a str>,
    pub max_iterations: u32,
    pub timeout_secs: u64,
}

/// Run the cognition phase: understand intent, discover resources, build plan.
///
/// Returns `Some(CognitionResult)` on success, `None` on failure (caller
/// should fall back to the old all-tools path).
pub async fn run_cognition(params: CognitionParams<'_>) -> Option<CognitionResult> {
    // Emit start event
    emit_status(params.stream_tx, "cognition_start", "Analyzing request...").await;

    let config = params.config;
    let model = params
        .cognition_model
        .filter(|m| !m.is_empty())
        .unwrap_or(&config.agent.model);

    // Create provider for the cognition model
    let provider = match crate::provider::factory::create_provider_for_model(config, model) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to create cognition provider, skipping cognition");
            return None;
        }
    };

    let system_prompt = build_cognition_prompt(params.contact_summary, params.channel);
    let tool_defs = discovery::cognition_tool_definitions();

    let mut messages = vec![
        ChatMessage::system(&system_prompt),
        ChatMessage::user(params.user_prompt),
    ];

    let max_iterations = if params.max_iterations > 0 {
        params.max_iterations
    } else {
        MAX_COGNITION_ITERATIONS
    };
    let timeout = std::time::Duration::from_secs(if params.timeout_secs > 0 {
        params.timeout_secs
    } else {
        DEFAULT_COGNITION_TIMEOUT_SECS
    });

    let started = std::time::Instant::now();
    let mut cognition_result: Option<CognitionResult> = None;

    for iteration in 1..=max_iterations {
        if started.elapsed() >= timeout {
            tracing::warn!(
                elapsed_ms = started.elapsed().as_millis(),
                "Cognition timeout reached"
            );
            break;
        }

        tracing::debug!(iteration, model = %model, "Cognition iteration");

        let request = ChatRequest {
            messages: messages.clone(),
            tools: tool_defs.clone(),
            model: model.to_string(),
            max_tokens: 1024,
            temperature: 0.2,
            think: Some(false),
            priority: RequestPriority::High,
        };

        let remaining = timeout.saturating_sub(started.elapsed());
        let response = match tokio::time::timeout(remaining, provider.chat(request)).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, iteration, "Cognition LLM call failed");
                break;
            }
            Err(_) => {
                tracing::warn!(iteration, "Cognition LLM call timed out");
                break;
            }
        };

        if !response.has_tool_calls() {
            // No tool calls — the LLM responded with text. This happens when
            // the model doesn't call plan_execution (e.g. small model just answers).
            // Try to parse it as JSON anyway, or give up.
            if let Some(ref text) = response.content {
                if let Ok(result) = serde_json::from_str::<CognitionResult>(text) {
                    cognition_result = Some(result);
                }
            }
            break;
        }

        // Process tool calls
        let mut found_plan = false;
        for tool_call in &response.tool_calls {
            let result_text = dispatch_discovery_tool(
                &tool_call.name,
                &tool_call.arguments,
                &params,
            )
            .await;

            if tool_call.name == "plan_execution" {
                // This is the output — parse as CognitionResult
                match serde_json::from_value::<CognitionResult>(tool_call.arguments.clone()) {
                    Ok(result) => {
                        tracing::info!(
                            understanding = %result.understanding,
                            complexity = ?result.complexity,
                            tools = result.tools.len(),
                            answer_directly = result.answer_directly,
                            "Cognition produced result"
                        );
                        cognition_result = Some(result);
                        found_plan = true;
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to parse plan_execution arguments");
                        // Feed error back so the LLM can retry
                        messages.push(ChatMessage::tool_result(
                            &tool_call.id,
                            &tool_call.name,
                            &format!("Error: invalid JSON structure: {}. Please call plan_execution again with valid arguments.", e),
                        ));
                    }
                }
            } else {
                // Discovery tool — emit step event with result summary
                let step_summary = format!(
                    "{}({}) → {}",
                    tool_call.name,
                    truncate_query(&tool_call.arguments),
                    summarize_discovery_result(&tool_call.name, &result_text),
                );
                emit_status(params.stream_tx, "cognition_step", &step_summary).await;

                // Add assistant message with tool call
                messages.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: response.content.clone(),
                    content_parts: None,
                    tool_calls: Some(vec![crate::provider::ToolCallSerialized {
                        id: tool_call.id.clone(),
                        call_type: "function".to_string(),
                        function: crate::provider::ToolCallFunction {
                            name: tool_call.name.clone(),
                            arguments: serde_json::to_string(&tool_call.arguments)
                                .unwrap_or_else(|_| "{}".to_string()),
                        },
                    }]),
                    tool_call_id: None,
                    name: None,
                });

                messages.push(ChatMessage::tool_result(
                    &tool_call.id,
                    &tool_call.name,
                    &result_text,
                ));
            }
        }

        if found_plan {
            break;
        }
    }

    // Validate the result if we got one
    let result = match cognition_result {
        Some(mut result) => {
            let known_tools = collect_known_tool_names(params.tool_registry).await;
            let known_skills = collect_known_skill_names(params.skill_registry).await;
            let issues = validate_cognition_result(&result, &known_tools, &known_skills);

            if !issues.is_empty() {
                tracing::warn!(
                    issues = issues.len(),
                    first_issue = %issues[0].message,
                    "Cognition result has validation issues"
                );
                // Remove invalid tools/skills instead of failing entirely
                result.tools.retain(|t| known_tools.iter().any(|kt| kt == &t.name));
                result.skills.retain(|s| known_skills.iter().any(|ks| ks == &s.name));
            }

            // Emit result summary
            let summary = format_result_summary(&result);
            emit_status(params.stream_tx, "cognition_result", &summary).await;

            tracing::info!(
                understanding = %result.understanding,
                tools = result.tools.len(),
                memory = result.memory_context.is_some(),
                plan_steps = result.plan.len(),
                constraints = ?result.constraints,
                plan = ?result.plan,
                elapsed_ms = started.elapsed().as_millis(),
                "Cognition phase complete"
            );

            Some(result)
        }
        None => {
            tracing::warn!(
                elapsed_ms = started.elapsed().as_millis(),
                "Cognition phase produced no result — falling back to full tool set"
            );
            emit_status(
                params.stream_tx,
                "cognition_result",
                "Cognition skipped — using full capabilities",
            )
            .await;
            None
        }
    };

    result
}

/// Dispatch a discovery tool call to the appropriate handler.
async fn dispatch_discovery_tool(
    name: &str,
    arguments: &serde_json::Value,
    params: &CognitionParams<'_>,
) -> String {
    let query = arguments
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match name {
        "discover_tools" => {
            discovery::discover_tools(query, params.tool_registry).await
        }
        "discover_skills" => {
            discovery::discover_skills(
                query,
                params.skill_registry,
                params.active_profile_slug.as_deref(),
            )
            .await
        }
        "discover_mcp" => {
            discovery::discover_mcp(query, params.config, params.tool_registry).await
        }
        "search_memory" => {
            #[cfg(feature = "embeddings")]
            if let Some(searcher) = params.memory_searcher {
                return discovery::search_memory(
                    query,
                    searcher,
                    params.contact_id,
                    params.agent_id,
                    &params.visible_profile_ids,
                )
                .await;
            }
            #[cfg(not(feature = "embeddings"))]
            let _ = query;
            "[]".to_string()
        }
        "search_knowledge" => {
            #[cfg(feature = "embeddings")]
            if let Some(rag) = params.rag_engine {
                return discovery::search_knowledge(query, rag).await;
            }
            "[]".to_string()
        }
        "plan_execution" => {
            // Handled by the caller — should not reach here
            "OK".to_string()
        }
        _ => format!("Unknown discovery tool: {}", name),
    }
}

/// Build the system prompt for the cognition mini-agent.
fn build_cognition_prompt(contact_summary: &str, channel: &str) -> String {
    let now = chrono::Local::now();
    let mut prompt = String::with_capacity(1200);

    prompt.push_str(
        "You are the planning module of Homun, a personal AI assistant.\n\
         Your job is to understand what the user wants and find the right resources to fulfill it.\n\n"
    );

    prompt.push_str(&format!(
        "Current time: {}\nChannel: {}\n",
        now.format("%Y-%m-%d %H:%M (%A) %Z"),
        channel,
    ));

    if !contact_summary.is_empty() {
        prompt.push_str(&format!("Sender: {}\n", contact_summary));
    }

    prompt.push_str(
        "\n## Your discovery tools\n\n\
         - **discover_tools(query)**: Find available tools by describing what's needed\n\
         - **discover_skills(query)**: Find installed skills (specialized capabilities)\n\
         - **discover_mcp(query)**: Find external services (calendar, email, GitHub, etc.)\n\
         - **search_memory(query)**: Search user's long-term memory for relevant context\n\
         - **search_knowledge(query)**: Search user's knowledge base (documents, notes)\n\n"
    );

    prompt.push_str(
        "## Workflow\n\n\
         1. Read the user's message and understand their intent\n\
         2. Call **discover_tools()** to find which tools can help\n\
         3. If past context might be relevant, call **search_memory()** with a specific query\n\
         4. If the user's documents might contain answers, call **search_knowledge()**\n\
         5. If external services (calendar, email, etc.) might help, call **discover_mcp()**\n\
         6. Call **plan_execution()** with your complete analysis\n\n\
         For simple requests (greetings, time, simple factual questions), skip discovery and \
         call plan_execution() directly with answer_directly=true and your answer.\n\n\
         **Important**: Only select tools you actually found via discover_tools. \
         Do NOT invent tool names. Use the exact names from the discovery results.\n\n\
         ## Constraints & Plan Quality\n\n\
         Extract ALL concrete parameters from the user's request into the `constraints` field:\n\
         - Dates and times (e.g. \"tomorrow evening\", \"March 22 2026 at 20:00\")\n\
         - Quantities (e.g. \"4 people\", \"budget 100€\")\n\
         - Locations (e.g. \"Novara centro\")\n\
         - Preferences (e.g. \"1st class\", \"vegetarian\")\n\n\
         Write the `plan` as specific, actionable steps — especially for browser tasks.\n\
         BAD: \"Search for restaurants\" → GOOD: \"Navigate to thefork.it, set location to Novara, \
         set date to 22 March 2026, set time to 20:00, set 4 guests, search\"\n"
    );

    prompt
}

/// Collect all known tool names from the registry.
async fn collect_known_tool_names(registry: &RwLock<ToolRegistry>) -> Vec<String> {
    registry
        .read()
        .await
        .names()
        .into_iter()
        .map(|s| s.to_string())
        .collect()
}

/// Collect all known skill names from the registry.
async fn collect_known_skill_names(registry: Option<&RwLock<SkillRegistry>>) -> Vec<String> {
    match registry {
        Some(r) => r
            .read()
            .await
            .list_for_model()
            .into_iter()
            .map(|(name, _)| name.to_string())
            .collect(),
        None => Vec::new(),
    }
}

/// Format a human-readable summary of the cognition result for the stream.
fn format_result_summary(result: &CognitionResult) -> String {
    let mut parts = Vec::new();

    if result.answer_directly {
        return "Direct answer (no tools needed)".to_string();
    }

    if !result.tools.is_empty() {
        let names: Vec<&str> = result.tools.iter().map(|t| t.name.as_str()).collect();
        parts.push(format!("Tools: {}", names.join(", ")));
    }
    if !result.skills.is_empty() {
        let names: Vec<&str> = result.skills.iter().map(|s| s.name.as_str()).collect();
        parts.push(format!("Skills: {}", names.join(", ")));
    }
    if result.memory_context.is_some() {
        parts.push("Memory: loaded".to_string());
    }
    if result.rag_context.is_some() {
        parts.push("Knowledge: loaded".to_string());
    }
    if !result.plan.is_empty() {
        parts.push(format!("Plan: {} steps", result.plan.len()));
    }

    if parts.is_empty() {
        result.understanding.clone()
    } else {
        format!("{} | {}", result.understanding, parts.join(" | "))
    }
}

/// Emit a status event to the frontend stream.
async fn emit_status(tx: Option<&mpsc::Sender<StreamChunk>>, event_type: &str, message: &str) {
    if let Some(tx) = tx {
        let _ = tx
            .send(StreamChunk {
                delta: message.to_string(),
                done: false,
                event_type: Some(event_type.to_string()),
                tool_call_data: None,
            })
            .await;
    }
}

/// Summarize a discovery tool result for the UI cognition step.
fn summarize_discovery_result(tool_name: &str, result_json: &str) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(result_json) {
        Ok(v) => v,
        Err(_) => return "done".to_string(),
    };

    let items = match parsed.as_array() {
        Some(arr) => arr,
        None => return "done".to_string(),
    };

    if items.is_empty() {
        return "0 results".to_string();
    }

    let names: Vec<&str> = items
        .iter()
        .take(4)
        .filter_map(|item| item.get("name").and_then(|n| n.as_str()))
        .collect();

    match tool_name {
        "discover_tools" | "discover_skills" | "discover_mcp" => {
            if names.is_empty() {
                format!("{} found", items.len())
            } else if items.len() > names.len() {
                format!("{} found: {}, …", items.len(), names.join(", "))
            } else {
                format!("{} found: {}", items.len(), names.join(", "))
            }
        }
        "search_memory" => format!("{} memories", items.len()),
        "search_knowledge" => format!("{} documents", items.len()),
        _ => format!("{} results", items.len()),
    }
}

/// Extract and truncate the query from tool arguments for logging.
fn truncate_query(args: &serde_json::Value) -> String {
    args.get("query")
        .and_then(|v| v.as_str())
        .map(|s| {
            if s.len() > 50 {
                format!("{}...", &s[..50])
            } else {
                s.to_string()
            }
        })
        .unwrap_or_else(|| "...".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_cognition_prompt_has_tools() {
        let prompt = build_cognition_prompt("Fabio (informal)", "telegram");
        assert!(prompt.contains("discover_tools"));
        assert!(prompt.contains("discover_skills"));
        assert!(prompt.contains("discover_mcp"));
        assert!(prompt.contains("search_memory"));
        assert!(prompt.contains("search_knowledge"));
        assert!(prompt.contains("plan_execution"));
        assert!(prompt.contains("telegram"));
        assert!(prompt.contains("Fabio"));
    }

    #[test]
    fn test_format_result_summary_direct() {
        let result = CognitionResult::direct("test");
        let summary = format_result_summary(&result);
        assert!(summary.contains("Direct answer"));
    }

    #[test]
    fn test_format_result_summary_with_tools() {
        let result = CognitionResult {
            understanding: "Search for trains".to_string(),
            complexity: super::super::types::Complexity::Complex,
            answer_directly: false,
            direct_answer: None,
            tools: vec![super::super::types::DiscoveredTool {
                name: "web_search".to_string(),
                description: "Search".to_string(),
                reason: "Need to search".to_string(),
            }],
            skills: Vec::new(),
            mcp_tools: Vec::new(),
            memory_context: Some("User prefers Frecciarossa".to_string()),
            rag_context: None,
            plan: vec!["Step 1".to_string(), "Step 2".to_string()],
            constraints: Vec::new(),
            autonomy_override: None,
        };
        let summary = format_result_summary(&result);
        assert!(summary.contains("web_search"));
        assert!(summary.contains("Memory: loaded"));
        assert!(summary.contains("Plan: 2 steps"));
    }

    #[test]
    fn test_truncate_query() {
        let args = serde_json::json!({"query": "short"});
        assert_eq!(truncate_query(&args), "short");

        let long = "a".repeat(100);
        let args = serde_json::json!({"query": long});
        let result = truncate_query(&args);
        assert!(result.ends_with("..."));
        assert!(result.len() < 60);
    }
}
