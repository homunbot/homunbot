//! LLM call with automatic fallback strategies.
//!
//! Encapsulates the retry logic for calling the LLM provider:
//! 1. Primary call (streaming or non-streaming)
//! 2. XML dispatch fallback (if model rejects native tool calling)
//! 3. Non-streaming fallback (if streaming fails)
//!
//! Returns either a successful response or signals to break the agent loop.

use std::sync::Arc;

use anyhow::{Context as _, Result};
use tokio::sync::mpsc;

use crate::provider::{
    ChatMessage, ChatRequest, ChatResponse, Provider, RequestPriority, StreamChunk, ToolDefinition,
};

/// Result of an LLM call attempt.
pub(crate) enum LlmCallResult {
    /// Successful response from the provider.
    Success(ChatResponse),
    /// User requested stop — caller should break the agent loop.
    Stopped,
}

/// Configuration for a single LLM call with fallback strategies.
pub(crate) struct LlmCallParams<'a> {
    pub provider: &'a Arc<dyn Provider>,
    pub model: &'a str,
    pub max_tokens: u32,
    pub temperature: f32,
    pub think: Option<bool>,
    pub tool_defs: &'a [ToolDefinition],
    pub xml_mode: bool,
    pub has_tools: bool,
    pub iteration: u32,
    pub xml_fallback_delay_ms: u64,
}

/// Call the LLM with automatic fallback strategies.
///
/// Handles: streaming → non-streaming fallback, XML dispatch auto-detect,
/// and stop signal checks. Returns `LlmCallResult::Stopped` if the user
/// cancels during the call.
pub(crate) async fn call_llm_with_fallback(
    params: &LlmCallParams<'_>,
    messages: Vec<ChatMessage>,
    stream_tx: Option<&mpsc::Sender<StreamChunk>>,
) -> Result<LlmCallResult> {
    let request = ChatRequest {
        messages,
        tools: if params.xml_mode {
            Vec::new()
        } else {
            params.tool_defs.to_vec()
        },
        model: params.model.to_string(),
        max_tokens: params.max_tokens,
        temperature: params.temperature,
        think: params.think,
        priority: RequestPriority::High,
    };

    if let Some(tx) = stream_tx {
        // Streaming path
        let tx_clone = tx.clone();
        match tokio::select! {
            response = params.provider.chat_stream(request.clone(), tx_clone) => response,
            _ = crate::agent::stop::wait_for_stop() => Err(crate::agent::stop::cancellation_error()),
        } {
            Ok(r) => Ok(LlmCallResult::Success(r)),
            Err(e) => {
                if crate::agent::stop::is_stop_requested() {
                    return Ok(LlmCallResult::Stopped);
                }

                // Check if tool use was rejected — eligible for XML fallback
                if should_try_xml_fallback(&e, params) {
                    return Ok(LlmCallResult::Stopped); // Signal caller to handle XML rebuild
                }

                // Regular streaming failure — try non-streaming
                tracing::warn!(error = ?e, "Streaming failed, falling back to non-streaming");
                call_non_streaming_fallback(params, request).await
            }
        }
    } else {
        // Non-streaming path
        match tokio::select! {
            response = params.provider.chat(request.clone()) => response,
            _ = crate::agent::stop::wait_for_stop() => Err(crate::agent::stop::cancellation_error()),
        } {
            Ok(r) => Ok(LlmCallResult::Success(r)),
            Err(e) => {
                if crate::agent::stop::is_stop_requested() {
                    return Ok(LlmCallResult::Stopped);
                }

                if should_try_xml_fallback(&e, params) {
                    return Ok(LlmCallResult::Stopped); // Signal caller to handle XML rebuild
                }

                Err(e.context("Failed to get response from LLM provider"))
            }
        }
    }
}

/// Check if an error indicates the model rejected tool calling
/// and we should switch to XML dispatch mode.
pub(crate) fn should_try_xml_fallback(error: &anyhow::Error, params: &LlmCallParams<'_>) -> bool {
    if params.xml_mode || !params.has_tools || params.iteration != 1 {
        return false;
    }
    let err_lower = error.to_string().to_lowercase();
    err_lower.contains("tool")
        || err_lower.contains("function")
        || err_lower.contains("not supported")
        || err_lower.contains("no endpoints")
        || err_lower.contains("invalid")
}

/// Non-streaming fallback when streaming fails.
async fn call_non_streaming_fallback(
    params: &LlmCallParams<'_>,
    request: ChatRequest,
) -> Result<LlmCallResult> {
    match tokio::select! {
        response = params.provider.chat(request) => response,
        _ = crate::agent::stop::wait_for_stop() => Err(crate::agent::stop::cancellation_error()),
    } {
        Ok(r) => Ok(LlmCallResult::Success(r)),
        Err(e) => {
            if crate::agent::stop::is_stop_requested() {
                Ok(LlmCallResult::Stopped)
            } else {
                Err(e.context("Non-streaming fallback also failed"))
            }
        }
    }
}
