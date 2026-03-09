//! Reliable provider wrapper with retry and failover.
//!
//! Wraps an ordered chain of LLM providers and adds:
//! - Exponential backoff retry on transient errors (429, 5xx, network)
//! - Automatic failover to the next provider on non-transient errors (401, 403)
//! - Structured logging of retry/failover decisions

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::utils::retry::{RetryConfig, RetryDecision, RetryableError};

use super::health::ProviderHealthTracker;
use super::traits::{ChatRequest, ChatResponse, Provider, StreamChunk};

/// A provider entry in the failover chain.
struct ProviderEntry {
    name: String,
    provider: Arc<dyn Provider>,
    model: String,
}

/// Wraps multiple providers with retry + failover logic.
///
/// On each `chat()` / `chat_stream()` call:
/// 1. Try the current provider with retries on transient errors
/// 2. If retries exhausted or non-transient error, failover to next provider
/// 3. Give up only when all providers have been tried
pub struct ReliableProvider {
    providers: Vec<ProviderEntry>,
    retry_config: RetryConfig,
    /// Index of the last provider that succeeded (sticky preference).
    last_good: AtomicUsize,
    /// Optional health tracker for circuit breaker logic.
    health: Option<Arc<ProviderHealthTracker>>,
}

impl ReliableProvider {
    /// Create a new reliable provider from an ordered chain.
    ///
    /// Each entry is `(provider_name, provider_instance, model_name)`.
    /// The first entry is the primary; subsequent entries are fallbacks.
    pub fn new(chain: Vec<(String, Arc<dyn Provider>, String)>, retry_config: RetryConfig) -> Self {
        let providers = chain
            .into_iter()
            .map(|(name, provider, model)| ProviderEntry {
                name,
                provider,
                model,
            })
            .collect();

        Self {
            providers,
            retry_config,
            last_good: AtomicUsize::new(0),
            health: None,
        }
    }

    /// Attach a health tracker for circuit breaker support.
    pub fn with_health(mut self, tracker: Arc<ProviderHealthTracker>) -> Self {
        self.health = Some(tracker);
        self
    }

    /// Classify an error to decide: retry same provider or failover to next.
    fn classify_error(err: &anyhow::Error) -> FailoverDecision {
        let err_str = err.to_string().to_lowercase();

        // Non-transient: failover immediately to next provider
        if err_str.contains("401")
            || err_str.contains("403")
            || err_str.contains("unauthorized")
            || err_str.contains("forbidden")
            || err_str.contains("invalid api key")
            || err_str.contains("invalid_api_key")
            || err_str.contains("context_length_exceeded")
            || err_str.contains("context window")
            || err_str.contains("model_not_found")
            || err_str.contains("model not found")
            || err_str.contains("does not exist")
            || err_str.contains("not available")
            || err_str.contains("not a valid")
            || err_str.contains("invalid model")
            || err_str.contains("bad request")
            || err_str.contains("not support tool")
            || err_str.contains("no endpoints found")
            || err_str.contains("unsupported")
            || err_str.contains("request body too large")
            || err_str.contains("payload too large")
            || err_str.contains("content too large")
            || err_str.contains("entity too large")
            || err_str.contains("http 413")
        {
            return FailoverDecision::NextProvider;
        }

        // Use the existing retry decision logic for transient classification
        let decision = err_str.retry_decision();
        match decision {
            RetryDecision::Retry | RetryDecision::WaitForNetwork => FailoverDecision::Retry,
            RetryDecision::Fail => FailoverDecision::NextProvider,
        }
    }
}

/// Internal decision for the failover loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailoverDecision {
    /// Retry the same provider (transient error)
    Retry,
    /// Skip to the next provider (non-transient error)
    NextProvider,
}

#[async_trait]
impl Provider for ReliableProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let start_idx = self.last_good.load(Ordering::Relaxed);
        let count = self.providers.len();
        let mut last_error: Option<anyhow::Error> = None;

        for offset in 0..count {
            let idx = (start_idx + offset) % count;
            let entry = &self.providers[idx];

            // Circuit breaker: skip Down providers (unless it's the last one)
            if offset + 1 < count {
                if let Some(ref h) = self.health {
                    if !h.is_available(&entry.name) {
                        tracing::debug!(provider = %entry.name, "Skipping Down provider");
                        continue;
                    }
                }
            }

            // Build request with this provider's model
            let mut req = request.clone();
            req.model = entry.model.clone();

            for attempt in 0..=self.retry_config.max_retries {
                if attempt > 0 {
                    let delay = self.retry_config.delay_for_attempt(attempt - 1);
                    tracing::warn!(
                        provider = %entry.name,
                        model = %entry.model,
                        attempt,
                        delay_ms = delay.as_millis(),
                        "Retrying after transient error"
                    );
                    tokio::time::sleep(delay).await;
                    // Re-clone the request for retry
                    req = request.clone();
                    req.model = entry.model.clone();
                }

                let t0 = std::time::Instant::now();
                match entry.provider.chat(req).await {
                    Ok(response) => {
                        if let Some(ref h) = self.health {
                            h.record_success(&entry.name, t0.elapsed());
                        }
                        if attempt > 0 || offset > 0 {
                            tracing::info!(
                                provider = %entry.name,
                                model = %entry.model,
                                attempt = attempt + 1,
                                failover_offset = offset,
                                "Provider succeeded"
                            );
                        }
                        self.last_good.store(idx, Ordering::Relaxed);
                        return Ok(response);
                    }
                    Err(e) => {
                        if let Some(ref h) = self.health {
                            h.record_error(&entry.name, t0.elapsed(), &e.to_string());
                        }
                        let decision = Self::classify_error(&e);

                        tracing::warn!(
                            provider = %entry.name,
                            model = %entry.model,
                            attempt = attempt + 1,
                            decision = ?decision,
                            error = %e,
                            "Provider call failed"
                        );

                        match decision {
                            FailoverDecision::NextProvider => {
                                last_error = Some(e);
                                break; // Skip remaining retries, go to next provider
                            }
                            FailoverDecision::Retry => {
                                last_error = Some(e);
                                // Rebuild request for next attempt
                                req = request.clone();
                                req.model = entry.model.clone();
                            }
                        }
                    }
                }
            }

            // Retries exhausted for this provider
            if offset + 1 < count {
                let next_idx = (start_idx + offset + 1) % count;
                let next = &self.providers[next_idx];
                tracing::warn!(
                    from_provider = %entry.name,
                    to_provider = %next.name,
                    to_model = %next.model,
                    "Failing over to next provider"
                );
            }
        }

        // All providers exhausted
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("No providers configured")))
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<ChatResponse> {
        let start_idx = self.last_good.load(Ordering::Relaxed);
        let count = self.providers.len();
        let mut last_error: Option<anyhow::Error> = None;

        for offset in 0..count {
            let idx = (start_idx + offset) % count;
            let entry = &self.providers[idx];

            // Circuit breaker: skip Down providers (unless last one)
            if offset + 1 < count {
                if let Some(ref h) = self.health {
                    if !h.is_available(&entry.name) {
                        tracing::debug!(provider = %entry.name, "Skipping Down provider (stream)");
                        continue;
                    }
                }
            }

            let mut req = request.clone();
            req.model = entry.model.clone();

            // For streaming, we only retry once before failover (to avoid
            // sending partial chunks from failed attempts).
            let max_attempts = if count > 1 {
                1
            } else {
                self.retry_config.max_retries
            };

            for attempt in 0..=max_attempts {
                if attempt > 0 {
                    let delay = self.retry_config.delay_for_attempt(attempt - 1);
                    tracing::warn!(
                        provider = %entry.name,
                        model = %entry.model,
                        attempt,
                        delay_ms = delay.as_millis(),
                        "Retrying stream after error"
                    );
                    tokio::time::sleep(delay).await;
                    req = request.clone();
                    req.model = entry.model.clone();
                }

                let t0 = std::time::Instant::now();
                match entry.provider.chat_stream(req, tx.clone()).await {
                    Ok(response) => {
                        if let Some(ref h) = self.health {
                            h.record_success(&entry.name, t0.elapsed());
                        }
                        if attempt > 0 || offset > 0 {
                            tracing::info!(
                                provider = %entry.name,
                                model = %entry.model,
                                "Stream provider succeeded after failover/retry"
                            );
                        }
                        self.last_good.store(idx, Ordering::Relaxed);
                        return Ok(response);
                    }
                    Err(e) => {
                        if let Some(ref h) = self.health {
                            h.record_error(&entry.name, t0.elapsed(), &e.to_string());
                        }
                        let decision = Self::classify_error(&e);
                        tracing::warn!(
                            provider = %entry.name,
                            model = %entry.model,
                            attempt = attempt + 1,
                            decision = ?decision,
                            error = %e,
                            "Stream provider call failed"
                        );
                        last_error = Some(e);

                        if decision == FailoverDecision::NextProvider {
                            break;
                        }
                        req = request.clone();
                        req.model = entry.model.clone();
                    }
                }
            }

            if offset + 1 < count {
                let next_idx = (start_idx + offset + 1) % count;
                let next = &self.providers[next_idx];
                tracing::warn!(
                    from_provider = %entry.name,
                    to_provider = %next.name,
                    to_model = %next.model,
                    "Failing over stream to next provider"
                );
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("No providers configured")))
    }

    fn name(&self) -> &str {
        let idx = self.last_good.load(Ordering::Relaxed);
        if idx < self.providers.len() {
            &self.providers[idx].name
        } else {
            "reliable"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_transient_errors() {
        let err = anyhow::anyhow!("Error 429: Too many requests");
        assert_eq!(
            ReliableProvider::classify_error(&err),
            FailoverDecision::Retry
        );

        let err = anyhow::anyhow!("Error 503: Service unavailable");
        assert_eq!(
            ReliableProvider::classify_error(&err),
            FailoverDecision::Retry
        );

        let err = anyhow::anyhow!("Connection timed out");
        assert_eq!(
            ReliableProvider::classify_error(&err),
            FailoverDecision::Retry
        );
    }

    #[test]
    fn test_classify_non_transient_errors() {
        let err = anyhow::anyhow!("Error 401: Unauthorized");
        assert_eq!(
            ReliableProvider::classify_error(&err),
            FailoverDecision::NextProvider
        );

        let err = anyhow::anyhow!("Error 403: Forbidden");
        assert_eq!(
            ReliableProvider::classify_error(&err),
            FailoverDecision::NextProvider
        );

        let err = anyhow::anyhow!("context_length_exceeded: max 8192 tokens");
        assert_eq!(
            ReliableProvider::classify_error(&err),
            FailoverDecision::NextProvider
        );

        let err = anyhow::anyhow!("model_not_found: gpt-5-turbo");
        assert_eq!(
            ReliableProvider::classify_error(&err),
            FailoverDecision::NextProvider
        );

        // "not a valid model ID" from OpenRouter
        let err = anyhow::anyhow!("Provider openrouter error: some/model is not a valid model ID");
        assert_eq!(
            ReliableProvider::classify_error(&err),
            FailoverDecision::NextProvider
        );

        // Generic "bad request" should not be retried
        let err = anyhow::anyhow!("Bad Request: invalid parameters");
        assert_eq!(
            ReliableProvider::classify_error(&err),
            FailoverDecision::NextProvider
        );

        let err = anyhow::anyhow!("Ollama error: http: request body too large");
        assert_eq!(
            ReliableProvider::classify_error(&err),
            FailoverDecision::NextProvider
        );
    }

    #[test]
    fn test_name_returns_last_good() {
        use std::time::Duration;

        // Create a minimal mock provider chain
        struct DummyProvider;

        #[async_trait]
        impl Provider for DummyProvider {
            async fn chat(&self, _: ChatRequest) -> Result<ChatResponse> {
                unreachable!()
            }
            fn name(&self) -> &str {
                "dummy"
            }
        }

        let reliable = ReliableProvider::new(
            vec![
                ("primary".into(), Arc::new(DummyProvider), "model-a".into()),
                ("fallback".into(), Arc::new(DummyProvider), "model-b".into()),
            ],
            RetryConfig {
                max_retries: 1,
                initial_delay: Duration::from_millis(10),
                ..Default::default()
            },
        );

        assert_eq!(reliable.name(), "primary");

        // Simulate failover
        reliable.last_good.store(1, Ordering::Relaxed);
        assert_eq!(reliable.name(), "fallback");
    }
}
