//! Unified one-shot LLM utility.
//!
//! Provides a single entry point for all "fire-and-forget" LLM calls
//! across the codebase (web API endpoints, background tasks, etc.).
//!
//! Benefits over calling `create_single_provider` + `provider.chat()` directly:
//! - Uses `ReliableProvider` with retry and failover (same as the agent loop)
//! - Automatically disables extended thinking (avoids empty-response bugs)
//! - Consistent timeout handling
//! - Centralized error messages

use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::config::Config;
use crate::provider::traits::{ChatMessage, ChatRequest};

/// Configuration for a one-shot LLM call.
pub struct OneShotRequest {
    /// System prompt — sets the LLM's role and output constraints.
    pub system_prompt: String,
    /// User message — the actual task/question.
    pub user_message: String,
    /// Maximum tokens in the response (default: 2048).
    pub max_tokens: u32,
    /// Sampling temperature (default: 0.3).
    pub temperature: f32,
    /// Timeout in seconds (default: 30).
    pub timeout_secs: u64,
    /// Specific model to use. If `None`, uses `config.agent.model`.
    pub model: Option<String>,
}

impl Default for OneShotRequest {
    fn default() -> Self {
        Self {
            system_prompt: String::new(),
            user_message: String::new(),
            max_tokens: 2048,
            temperature: 0.3,
            timeout_secs: 30,
            model: None,
        }
    }
}

/// Result of a one-shot LLM call.
pub struct OneShotResponse {
    /// The text content returned by the LLM.
    pub content: String,
    /// Finish reason (e.g. "stop", "max_tokens").
    pub finish_reason: String,
    /// The model that was actually used.
    pub model: String,
    /// Wall-clock latency of the LLM call.
    pub latency: Duration,
}

/// Make a one-shot LLM call using the configured provider.
///
/// This is the **single engine** for all non-conversational LLM calls in
/// the codebase. It wraps the provider in `ReliableProvider` (retry on
/// transient errors, failover to fallback models) and always disables
/// extended thinking to avoid empty-response bugs with reasoning models.
///
/// # Errors
///
/// Returns an error if:
/// - No model is configured
/// - Provider creation fails (missing API key, etc.)
/// - The LLM call times out
/// - The LLM returns no text content
/// - The underlying provider returns a non-transient error
pub async fn llm_one_shot(config: &Config, req: OneShotRequest) -> Result<OneShotResponse> {
    let model = req
        .model
        .unwrap_or_else(|| config.agent.model.trim().to_string());
    anyhow::ensure!(!model.is_empty(), "No active model configured");

    // Use the full provider chain (retry + fallbacks), same as the agent loop.
    let provider = super::factory::create_provider_for_model(config, &model)
        .context("Failed to create LLM provider")?;

    let started = Instant::now();

    let chat_req = ChatRequest {
        messages: vec![
            ChatMessage::system(&req.system_prompt),
            ChatMessage::user(&req.user_message),
        ],
        tools: vec![],
        model: model.clone(),
        max_tokens: req.max_tokens,
        temperature: req.temperature,
        // Always disable thinking for one-shot utility calls.
        // Reasoning models (Claude Sonnet 4+) default to thinking-on, which
        // consumes output budget and can produce empty text content.
        think: Some(false),
    };

    tracing::debug!(
        model = %model,
        max_tokens = req.max_tokens,
        timeout_secs = req.timeout_secs,
        "llm_one_shot: sending request"
    );

    let response = tokio::time::timeout(
        Duration::from_secs(req.timeout_secs),
        provider.chat(chat_req),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "LLM request timed out after {}s (model: {})",
            req.timeout_secs,
            model
        )
    })?
    .context("LLM call failed")?;

    let latency = started.elapsed();

    tracing::debug!(
        model = %model,
        finish_reason = %response.finish_reason,
        has_content = response.content.is_some(),
        latency_ms = latency.as_millis(),
        "llm_one_shot: response received"
    );

    let content = response.content.ok_or_else(|| {
        anyhow::anyhow!(
            "LLM returned no text content (finish_reason: {}, model: {})",
            response.finish_reason,
            model
        )
    })?;

    Ok(OneShotResponse {
        content,
        finish_reason: response.finish_reason,
        model,
        latency,
    })
}
