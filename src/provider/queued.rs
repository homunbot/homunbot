//! Priority-aware concurrency limiter for LLM providers.
//!
//! Wraps any `Provider` with a `tokio::sync::Semaphore` that limits how many
//! concurrent requests hit a single backend.  Requests with higher
//! [`RequestPriority`] are served first when permits are contended.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;

use super::traits::{ChatRequest, ChatResponse, Provider, RequestPriority, StreamChunk};

/// Pending-request counters per priority level.
struct PendingCounters {
    high: AtomicUsize,
    normal: AtomicUsize,
}

/// Provider wrapper that enforces per-backend concurrency limits with
/// priority-aware scheduling.
///
/// Concurrency is capped by a `tokio::Semaphore`.  When permits are
/// contended, higher-priority requests proceed first:
///
/// - **High** — acquires immediately (waits only for a free permit).
/// - **Normal** — yields while any High request is pending.
/// - **Low** — yields while any High or Normal request is pending.
pub struct QueuedProvider {
    inner: Arc<dyn Provider>,
    semaphore: Arc<tokio::sync::Semaphore>,
    pending: Arc<PendingCounters>,
    provider_name: String,
}

impl QueuedProvider {
    /// Wrap `inner` with a concurrency limit of `max_concurrent` requests.
    pub fn new(inner: Arc<dyn Provider>, max_concurrent: usize) -> Self {
        let provider_name = inner.name().to_string();
        Self {
            inner,
            semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent)),
            pending: Arc::new(PendingCounters {
                high: AtomicUsize::new(0),
                normal: AtomicUsize::new(0),
            }),
            provider_name,
        }
    }

    /// Wait until no higher-priority requests are pending, then acquire a
    /// semaphore permit.
    async fn acquire_permit(
        &self,
        priority: RequestPriority,
    ) -> Result<tokio::sync::OwnedSemaphorePermit> {
        // Increment our priority counter so lower-priority requests can see us.
        match priority {
            RequestPriority::High => self.pending.high.fetch_add(1, Ordering::Relaxed),
            RequestPriority::Normal => self.pending.normal.fetch_add(1, Ordering::Relaxed),
            RequestPriority::Low => 0, // Low doesn't block others
        };

        // Yield to higher-priority callers before acquiring a permit.
        loop {
            let dominated = match priority {
                RequestPriority::High => false,
                RequestPriority::Normal => self.pending.high.load(Ordering::Relaxed) > 0,
                RequestPriority::Low => {
                    self.pending.high.load(Ordering::Relaxed) > 0
                        || self.pending.normal.load(Ordering::Relaxed) > 0
                }
            };
            if !dominated {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| anyhow::anyhow!("LLM queue semaphore closed"))?;

        Ok(permit)
    }

    /// Decrement our priority counter after the request finishes.
    fn release_priority(&self, priority: RequestPriority) {
        match priority {
            RequestPriority::High => {
                self.pending.high.fetch_sub(1, Ordering::Relaxed);
            }
            RequestPriority::Normal => {
                self.pending.normal.fetch_sub(1, Ordering::Relaxed);
            }
            RequestPriority::Low => {}
        }
    }
}

#[async_trait]
impl Provider for QueuedProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let priority = request.priority;
        let _permit = self.acquire_permit(priority).await?;

        tracing::debug!(
            provider = %self.provider_name,
            priority = ?priority,
            available_permits = self.semaphore.available_permits(),
            "LLM queue: acquired permit"
        );

        let result = self.inner.chat(request).await;
        self.release_priority(priority);
        result
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<ChatResponse> {
        let priority = request.priority;
        let _permit = self.acquire_permit(priority).await?;

        tracing::debug!(
            provider = %self.provider_name,
            priority = ?priority,
            available_permits = self.semaphore.available_permits(),
            "LLM queue: acquired permit (stream)"
        );

        let result = self.inner.chat_stream(request, tx).await;
        self.release_priority(priority);
        result
    }

    fn name(&self) -> &str {
        self.inner.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::traits::{ChatMessage, Usage};

    /// Dummy provider that sleeps to simulate latency.
    struct SlowProvider {
        delay: Duration,
    }

    #[async_trait]
    impl Provider for SlowProvider {
        async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse> {
            tokio::time::sleep(self.delay).await;
            Ok(ChatResponse {
                content: Some("ok".to_string()),
                tool_calls: vec![],
                finish_reason: "stop".to_string(),
                usage: Usage::default(),
            })
        }

        fn name(&self) -> &str {
            "slow"
        }
    }

    #[tokio::test]
    async fn concurrency_is_limited() {
        let inner = Arc::new(SlowProvider {
            delay: Duration::from_millis(100),
        });
        let queued = Arc::new(QueuedProvider::new(inner, 1));

        let start = tokio::time::Instant::now();

        // Fire 3 requests in parallel — with concurrency=1 they must serialize.
        let mut handles = Vec::new();
        for _ in 0..3 {
            let q = queued.clone();
            handles.push(tokio::spawn(async move {
                let req = ChatRequest {
                    messages: vec![ChatMessage::user("hi")],
                    tools: vec![],
                    model: "test".to_string(),
                    max_tokens: 10,
                    temperature: 0.0,
                    think: None,
                    priority: RequestPriority::Normal,
                };
                q.chat(req).await.unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        let elapsed = start.elapsed();
        // 3 serial requests × 100ms ≈ 300ms minimum.
        assert!(
            elapsed >= Duration::from_millis(280),
            "Expected ~300ms serial execution, got {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn high_priority_not_blocked_by_low() {
        let inner = Arc::new(SlowProvider {
            delay: Duration::from_millis(50),
        });
        // 2 permits: one will be occupied by a low-priority request,
        // the high-priority request should still get the second permit.
        let queued = Arc::new(QueuedProvider::new(inner, 2));

        let q1 = queued.clone();
        let low_handle = tokio::spawn(async move {
            let req = ChatRequest {
                messages: vec![ChatMessage::user("bg")],
                tools: vec![],
                model: "test".to_string(),
                max_tokens: 10,
                temperature: 0.0,
                think: None,
                priority: RequestPriority::Low,
            };
            q1.chat(req).await.unwrap();
        });

        // Small delay to let Low enter the semaphore first.
        tokio::time::sleep(Duration::from_millis(5)).await;

        let q2 = queued.clone();
        let start = tokio::time::Instant::now();
        let high_handle = tokio::spawn(async move {
            let req = ChatRequest {
                messages: vec![ChatMessage::user("user")],
                tools: vec![],
                model: "test".to_string(),
                max_tokens: 10,
                temperature: 0.0,
                think: None,
                priority: RequestPriority::High,
            };
            q2.chat(req).await.unwrap();
        });

        high_handle.await.unwrap();
        let high_elapsed = start.elapsed();

        // High should complete in ~50ms (provider delay), not waiting for Low.
        assert!(
            high_elapsed < Duration::from_millis(120),
            "High priority took too long: {:?}",
            high_elapsed
        );

        low_handle.await.unwrap();
    }
}
