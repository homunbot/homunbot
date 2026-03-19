//! Gateway-level message debounce and per-session serialisation.
//!
//! When multiple messages arrive for the same session key within a
//! configurable time window, they are aggregated into a single message
//! before being dispatched to the agent loop.  This avoids duplicate
//! LLM calls, race conditions on session history, and poor UX where the
//! agent responds to each fragment independently.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Mutex};
use tokio::time::Instant;

use crate::bus::{InboundMessage, MessageMetadata};

// ── Prepared message (post-gate, pre-dispatch) ──────────────────────

/// A message that has passed all gateway pre-processing gates and is
/// ready for debounce buffering or immediate dispatch.
pub struct PreparedMessage {
    pub inbound: InboundMessage,
    pub session_key: String,
    pub channel_name: String,
    pub chat_id: String,
    pub ctx: DispatchContext,
}

/// Context extracted by the gateway during pre-processing.
/// Carried alongside the message so the dispatch function has
/// everything it needs without re-reading config.
pub struct DispatchContext {
    pub is_system: bool,
    pub is_automation: bool,
    /// Approval routing: (notify_channel, notify_chat_id) for assisted mode on any channel.
    pub approval_notify: Option<(String, String)>,
    pub email_meta: Option<(String, Option<String>, Option<String>)>,
    pub email_from: Option<String>,
    pub email_body_preview: Option<String>,
    pub automation_run_id: Option<String>,
    pub automation_id: Option<String>,
    pub suppress_outbound: bool,
    pub blocked_tools: &'static [&'static str],
    pub thinking_override: Option<bool>,
    pub inbound_metadata: Option<MessageMetadata>,
    /// Resolved contact ID from the contact book (if sender is known).
    pub contact_id: Option<i64>,
    /// Response mode for this message (contact override > channel default > "automatic").
    pub contact_response_mode: Option<String>,
    /// Resolved contact for agent routing (MAG-2).
    pub contact: Option<crate::contacts::Contact>,
}

// ── Config ──────────────────────────────────────────────────────────

/// Debounce behaviour settings (read from `AgentConfig`).
#[derive(Debug, Clone)]
pub struct DebounceConfig {
    /// How long to wait for additional messages before flushing.
    pub window: Duration,
    /// Max messages to buffer before force-flushing.
    pub max_batch: usize,
}

impl DebounceConfig {
    pub fn from_agent_config(window_ms: u64, max_batch: usize) -> Self {
        Self {
            window: Duration::from_millis(window_ms),
            max_batch: max_batch.max(1),
        }
    }

    /// Returns true when debounce is effectively disabled.
    pub fn is_disabled(&self) -> bool {
        self.window.is_zero()
    }
}

// ── Skip predicate ──────────────────────────────────────────────────

/// Should this message bypass debounce and be dispatched immediately?
pub fn should_skip_debounce(msg: &PreparedMessage) -> bool {
    // System messages (automations, cron) — isolated sessions
    if msg.ctx.is_system {
        return true;
    }
    // Attachments — RAG ingestion already happened, needs immediate response
    if msg
        .inbound
        .metadata
        .as_ref()
        .and_then(|m| m.attachment_path.as_ref())
        .is_some()
    {
        return true;
    }
    // Web UI explicit send (has a run_id)
    if msg
        .inbound
        .metadata
        .as_ref()
        .and_then(|m| m.web_run_id.as_ref())
        .is_some()
    {
        return true;
    }
    false
}

// ── Aggregation ─────────────────────────────────────────────────────

/// Merge multiple buffered messages into one.
///
/// Content is joined with `\n`, metadata uses the latest message's
/// `thinking_override`, and the first available `thread_id`.
pub fn aggregate(mut messages: Vec<PreparedMessage>) -> PreparedMessage {
    debug_assert!(!messages.is_empty());
    if messages.len() == 1 {
        return messages.remove(0);
    }

    let mut base = messages.remove(0); // first message is the base
    let mut parts = vec![base.inbound.content.clone()];

    for msg in &messages {
        parts.push(msg.inbound.content.clone());
    }
    base.inbound.content = parts.join("\n");

    // Use latest timestamp
    if let Some(last) = messages.last() {
        base.inbound.timestamp = last.inbound.timestamp;
    }

    // thinking_override: last wins
    if let Some(last) = messages.last() {
        if let Some(ovr) = last
            .inbound
            .metadata
            .as_ref()
            .and_then(|m| m.thinking_override)
        {
            base.ctx.thinking_override = Some(ovr);
            if let Some(ref mut meta) = base.inbound.metadata {
                meta.thinking_override = Some(ovr);
            }
        }
    }

    // thread_id: keep first available
    let base_has_thread = base
        .inbound
        .metadata
        .as_ref()
        .and_then(|m| m.thread_id.as_ref())
        .is_some();
    if !base_has_thread {
        for msg in &messages {
            if let Some(tid) = msg
                .inbound
                .metadata
                .as_ref()
                .and_then(|m| m.thread_id.clone())
            {
                if let Some(ref mut meta) = base.inbound.metadata {
                    meta.thread_id = Some(tid);
                }
                break;
            }
        }
    }

    let count = parts.len();
    tracing::debug!(
        session = %base.session_key,
        messages = count,
        "Aggregated {count} messages into one"
    );
    base
}

// ── Per-session lock ────────────────────────────────────────────────

/// Shared map of per-session tokio Mutexes.  Ensures only one agent
/// processing task runs per session_key at a time.
pub type SessionLocks = Arc<std::sync::Mutex<HashMap<String, Arc<Mutex<()>>>>>;

/// Obtain (or create) the lock for a given session key.
pub fn get_session_lock(locks: &SessionLocks, session_key: &str) -> Arc<Mutex<()>> {
    let mut map = locks.lock().expect("session locks poisoned");
    map.entry(session_key.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

// ── Debouncer ───────────────────────────────────────────────────────

/// Per-session pending buffer.
struct SessionBuffer {
    messages: Vec<PreparedMessage>,
    first_arrival: Instant,
}

/// The debouncer receives `PreparedMessage`s, buffers them per session,
/// and flushes aggregated batches after the debounce window expires or
/// the batch limit is reached.
pub struct MessageDebouncer {
    config: DebounceConfig,
    rx: mpsc::Receiver<PreparedMessage>,
}

impl MessageDebouncer {
    pub fn new(config: DebounceConfig, rx: mpsc::Receiver<PreparedMessage>) -> Self {
        Self { config, rx }
    }

    /// Run the debounce loop.  Calls `dispatch` for each aggregated
    /// (or immediate-skip) message.  Returns when the sender is dropped.
    pub async fn run<F>(mut self, dispatch: F)
    where
        F: Fn(PreparedMessage) + Send + Sync + 'static,
    {
        // Fast path: debounce disabled — just forward everything.
        if self.config.is_disabled() {
            while let Some(msg) = self.rx.recv().await {
                dispatch(msg);
            }
            return;
        }

        let tick_interval = Duration::from_millis(100);
        let mut interval = tokio::time::interval(tick_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut buffers: HashMap<String, SessionBuffer> = HashMap::new();

        loop {
            tokio::select! {
                biased; // prefer draining messages over tick

                maybe_msg = self.rx.recv() => {
                    let Some(msg) = maybe_msg else {
                        // Channel closed — flush remaining.
                        break;
                    };

                    if should_skip_debounce(&msg) {
                        dispatch(msg);
                        continue;
                    }

                    let key = msg.session_key.clone();
                    let buf = buffers.entry(key.clone()).or_insert_with(|| SessionBuffer {
                        messages: Vec::new(),
                        first_arrival: Instant::now(),
                    });
                    buf.messages.push(msg);

                    // Force flush on max batch
                    if buf.messages.len() >= self.config.max_batch {
                        let buf = buffers.remove(&key).unwrap();
                        dispatch(aggregate(buf.messages));
                    }
                }

                _ = interval.tick() => {
                    let now = Instant::now();
                    let expired: Vec<String> = buffers
                        .iter()
                        .filter(|(_, buf)| now.duration_since(buf.first_arrival) >= self.config.window)
                        .map(|(k, _)| k.clone())
                        .collect();

                    for key in expired {
                        if let Some(buf) = buffers.remove(&key) {
                            dispatch(aggregate(buf.messages));
                        }
                    }
                }
            }
        }

        // Flush all remaining buffers on shutdown.
        for (_, buf) in buffers {
            dispatch(aggregate(buf.messages));
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn make_msg(session: &str, content: &str) -> PreparedMessage {
        PreparedMessage {
            inbound: InboundMessage {
                channel: "telegram".into(),
                sender_id: "user1".into(),
                chat_id: session.into(),
                content: content.into(),
                timestamp: Utc::now(),
                metadata: Some(MessageMetadata::default()),
            },
            session_key: format!("telegram:{session}"),
            channel_name: "telegram".into(),
            chat_id: session.into(),
            ctx: DispatchContext {
                is_system: false,
                is_automation: false,
                approval_notify: None,
                email_meta: None,
                email_from: None,
                email_body_preview: None,
                automation_run_id: None,
                automation_id: None,
                suppress_outbound: false,
                blocked_tools: &[],
                thinking_override: None,
                inbound_metadata: None,
                contact_id: None,
                contact_response_mode: None,
                contact: None,
            },
        }
    }

    fn make_system_msg(session: &str, content: &str) -> PreparedMessage {
        let mut msg = make_msg(session, content);
        msg.ctx.is_system = true;
        if let Some(ref mut meta) = msg.inbound.metadata {
            meta.is_system = true;
        }
        msg
    }

    #[test]
    fn test_should_skip_system() {
        let msg = make_system_msg("chat1", "cron trigger");
        assert!(should_skip_debounce(&msg));
    }

    #[test]
    fn test_should_skip_attachment() {
        let mut msg = make_msg("chat1", "photo.jpg");
        if let Some(ref mut meta) = msg.inbound.metadata {
            meta.attachment_path = Some("/tmp/photo.jpg".into());
        }
        assert!(should_skip_debounce(&msg));
    }

    #[test]
    fn test_should_skip_web_run_id() {
        let mut msg = make_msg("chat1", "hello");
        if let Some(ref mut meta) = msg.inbound.metadata {
            meta.web_run_id = Some("run-123".into());
        }
        assert!(should_skip_debounce(&msg));
    }

    #[test]
    fn test_normal_message_not_skipped() {
        let msg = make_msg("chat1", "hello");
        assert!(!should_skip_debounce(&msg));
    }

    #[test]
    fn test_aggregate_single() {
        let msg = make_msg("chat1", "hello");
        let result = aggregate(vec![msg]);
        assert_eq!(result.inbound.content, "hello");
    }

    #[test]
    fn test_aggregate_multiple() {
        let m1 = make_msg("chat1", "ciao");
        let m2 = make_msg("chat1", "come stai?");
        let m3 = make_msg("chat1", "tutto bene?");
        let result = aggregate(vec![m1, m2, m3]);
        assert_eq!(result.inbound.content, "ciao\ncome stai?\ntutto bene?");
    }

    #[test]
    fn test_aggregate_thinking_override_last_wins() {
        let mut m1 = make_msg("chat1", "a");
        m1.ctx.thinking_override = Some(true);
        if let Some(ref mut meta) = m1.inbound.metadata {
            meta.thinking_override = Some(true);
        }

        let mut m2 = make_msg("chat1", "b");
        m2.ctx.thinking_override = Some(false);
        if let Some(ref mut meta) = m2.inbound.metadata {
            meta.thinking_override = Some(false);
        }

        let result = aggregate(vec![m1, m2]);
        assert_eq!(result.ctx.thinking_override, Some(false));
    }

    #[test]
    fn test_aggregate_thread_id_first_available() {
        let m1 = make_msg("chat1", "a");
        let mut m2 = make_msg("chat1", "b");
        if let Some(ref mut meta) = m2.inbound.metadata {
            meta.thread_id = Some("thread-42".into());
        }

        let result = aggregate(vec![m1, m2]);
        assert_eq!(
            result
                .inbound
                .metadata
                .as_ref()
                .and_then(|m| m.thread_id.as_deref()),
            Some("thread-42")
        );
    }

    #[tokio::test]
    async fn test_debounce_disabled_passthrough() {
        let (tx, rx) = mpsc::channel(10);
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let config = DebounceConfig {
            window: Duration::ZERO,
            max_batch: 10,
        };

        let handle = tokio::spawn(async move {
            MessageDebouncer::new(config, rx)
                .run(move |_msg| {
                    counter_clone.fetch_add(1, Ordering::Relaxed);
                })
                .await;
        });

        tx.send(make_msg("chat1", "a")).await.unwrap();
        tx.send(make_msg("chat1", "b")).await.unwrap();
        tx.send(make_msg("chat2", "c")).await.unwrap();
        drop(tx);

        handle.await.unwrap();
        assert_eq!(counter.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn test_system_message_immediate_dispatch() {
        let (tx, rx) = mpsc::channel(10);
        let dispatched = Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
        let dispatched_clone = dispatched.clone();

        let config = DebounceConfig {
            window: Duration::from_secs(10), // long window
            max_batch: 100,
        };

        let handle = tokio::spawn(async move {
            MessageDebouncer::new(config, rx)
                .run(move |msg| {
                    let d = dispatched_clone.clone();
                    tokio::spawn(async move {
                        d.lock().await.push(msg.inbound.content.clone());
                    });
                })
                .await;
        });

        // Send a system message — should dispatch immediately
        tx.send(make_system_msg("chat1", "automation run"))
            .await
            .unwrap();

        // Brief pause to let dispatch happen
        tokio::time::sleep(Duration::from_millis(50)).await;

        // System message should already be dispatched even though window is 10s
        let current = dispatched.lock().await;
        assert_eq!(current.len(), 1);
        assert_eq!(current[0], "automation run");
        drop(current);

        drop(tx);
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_max_batch_forces_flush() {
        let (tx, rx) = mpsc::channel(20);
        let dispatched = Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
        let dispatched_clone = dispatched.clone();

        let config = DebounceConfig {
            window: Duration::from_secs(60), // very long, won't expire
            max_batch: 3,
        };

        let handle = tokio::spawn(async move {
            MessageDebouncer::new(config, rx)
                .run(move |msg| {
                    let d = dispatched_clone.clone();
                    tokio::spawn(async move {
                        d.lock().await.push(msg.inbound.content.clone());
                    });
                })
                .await;
        });

        // Send exactly max_batch messages
        tx.send(make_msg("chat1", "a")).await.unwrap();
        tx.send(make_msg("chat1", "b")).await.unwrap();
        tx.send(make_msg("chat1", "c")).await.unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        let current = dispatched.lock().await;
        assert_eq!(current.len(), 1);
        assert_eq!(current[0], "a\nb\nc");
        drop(current);

        drop(tx);
        handle.await.unwrap();
    }

    #[test]
    fn test_session_lock_creation() {
        let locks: SessionLocks = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let lock1 = get_session_lock(&locks, "telegram:123");
        let lock2 = get_session_lock(&locks, "telegram:123");
        // Same Arc — both point to the same Mutex
        assert!(Arc::ptr_eq(&lock1, &lock2));

        let lock3 = get_session_lock(&locks, "telegram:456");
        assert!(!Arc::ptr_eq(&lock1, &lock3));
    }
}
