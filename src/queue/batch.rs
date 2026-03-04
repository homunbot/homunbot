use std::fmt;

use chrono::{DateTime, Utc};
use tokio::time::Instant;

use super::QueueConfig;

/// A generic item in the batch queue.
#[derive(Debug, Clone)]
pub struct QueueItem<T> {
    /// Unique identifier (typically UUID).
    pub id: String,
    /// The domain-specific payload (email, cron fire, webhook, etc.).
    pub payload: T,
    /// One-line summary for digest rendering.
    pub summary: String,
    /// When the item was received.
    pub received_at: DateTime<Utc>,
}

/// Event emitted by the queue when items are ready for processing.
#[derive(Debug)]
pub enum QueueEvent<T> {
    /// A single item arrived and the batch window expired with no more items.
    Single(QueueItem<T>),
    /// Multiple items accumulated (threshold reached or window expired).
    Batch(Vec<QueueItem<T>>),
}

impl<T> QueueEvent<T> {
    /// Number of items in this event.
    pub fn len(&self) -> usize {
        match self {
            Self::Single(_) => 1,
            Self::Batch(items) => items.len(),
        }
    }

    /// Consume into a vec of items regardless of variant.
    pub fn into_items(self) -> Vec<QueueItem<T>> {
        match self {
            Self::Single(item) => vec![item],
            Self::Batch(items) => items,
        }
    }
}

/// Generic batching queue.
///
/// Accumulates `QueueItem<T>` and emits them as `QueueEvent`s based on:
/// - **Threshold**: when `batch_threshold` items accumulate, emit immediately.
/// - **Time window**: after the first item, wait `batch_window_secs` then emit
///   whatever has accumulated (single item → `Single`, multiple → `Batch`).
///
/// # Usage
///
/// ```ignore
/// let mut queue = BatchQueue::new("email:lavoro", QueueConfig::default());
///
/// // Push items as they arrive
/// if let Some(event) = queue.push(item) {
///     // Threshold reached — handle the batch
///     handle(event);
/// }
///
/// // Call tick() periodically (e.g. every second) to check the time window
/// if let Some(event) = queue.tick() {
///     handle(event);
/// }
/// ```
pub struct BatchQueue<T> {
    name: String,
    pending: Vec<QueueItem<T>>,
    window_start: Option<Instant>,
    config: QueueConfig,
}

impl<T: fmt::Debug> fmt::Debug for BatchQueue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BatchQueue")
            .field("name", &self.name)
            .field("pending_count", &self.pending.len())
            .field("has_window", &self.window_start.is_some())
            .finish()
    }
}

impl<T> BatchQueue<T> {
    /// Create a new queue with the given name and config.
    pub fn new(name: impl Into<String>, config: QueueConfig) -> Self {
        Self {
            name: name.into(),
            pending: Vec::new(),
            window_start: None,
            config,
        }
    }

    /// Queue name (e.g. "email:lavoro", "cron", "alerts").
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Number of items currently waiting.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Push an item. Returns `Some(event)` if the batch threshold is reached.
    ///
    /// If the threshold is not reached, the item is buffered and `None` is returned.
    /// Call `tick()` periodically to check the time window.
    pub fn push(&mut self, item: QueueItem<T>) -> Option<QueueEvent<T>> {
        self.pending.push(item);

        // Start the window timer on first item
        if self.window_start.is_none() {
            self.window_start = Some(Instant::now());
        }

        // Check threshold
        if self.pending.len() >= self.config.batch_threshold as usize {
            return Some(self.emit());
        }

        None
    }

    /// Check if the time window has expired. Call this periodically.
    ///
    /// Returns `Some(event)` if items are pending and the window has elapsed.
    pub fn tick(&mut self) -> Option<QueueEvent<T>> {
        if self.pending.is_empty() {
            return None;
        }

        let window = std::time::Duration::from_secs(self.config.batch_window_secs);
        if let Some(start) = self.window_start {
            if start.elapsed() >= window {
                return Some(self.emit());
            }
        }

        None
    }

    /// Peek at pending items without removing them.
    pub fn peek(&self) -> &[QueueItem<T>] {
        &self.pending
    }

    /// Drain all pending items, returning them as a vec. Resets the window.
    pub fn drain(&mut self) -> Vec<QueueItem<T>> {
        self.window_start = None;
        std::mem::take(&mut self.pending)
    }

    /// The configured delay between processing successive items.
    pub fn process_delay(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.config.process_delay_secs)
    }

    /// Emit pending items as a `QueueEvent` and reset the window.
    fn emit(&mut self) -> QueueEvent<T> {
        self.window_start = None;
        let items = std::mem::take(&mut self.pending);
        if items.len() == 1 {
            // Safety: we just checked len == 1
            QueueEvent::Single(items.into_iter().next().unwrap())
        } else {
            QueueEvent::Batch(items)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(id: &str, summary: &str) -> QueueItem<String> {
        QueueItem {
            id: id.to_string(),
            payload: format!("payload-{id}"),
            summary: summary.to_string(),
            received_at: Utc::now(),
        }
    }

    #[test]
    fn single_item_does_not_trigger_batch() {
        let mut queue = BatchQueue::new("test", QueueConfig::default());
        let result = queue.push(make_item("1", "first"));
        assert!(result.is_none());
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn threshold_triggers_batch() {
        let config = QueueConfig {
            batch_threshold: 3,
            ..Default::default()
        };
        let mut queue = BatchQueue::new("test", config);

        assert!(queue.push(make_item("1", "first")).is_none());
        assert!(queue.push(make_item("2", "second")).is_none());

        let event = queue.push(make_item("3", "third"));
        assert!(event.is_some());

        let event = event.unwrap();
        assert_eq!(event.len(), 3);
        assert!(queue.is_empty());
    }

    #[test]
    fn threshold_one_always_emits_single() {
        let config = QueueConfig {
            batch_threshold: 1,
            ..Default::default()
        };
        let mut queue = BatchQueue::new("test", config);

        let event = queue.push(make_item("1", "only one")).unwrap();
        match event {
            QueueEvent::Single(item) => assert_eq!(item.id, "1"),
            QueueEvent::Batch(_) => panic!("expected Single"),
        }
    }

    #[test]
    fn drain_returns_all_and_resets() {
        let mut queue = BatchQueue::new("test", QueueConfig::default());
        queue.push(make_item("1", "a"));
        queue.push(make_item("2", "b"));

        let items = queue.drain();
        assert_eq!(items.len(), 2);
        assert!(queue.is_empty());
        assert!(queue.window_start.is_none());
    }

    #[test]
    fn peek_does_not_drain() {
        let mut queue = BatchQueue::new("test", QueueConfig::default());
        queue.push(make_item("1", "a"));

        assert_eq!(queue.peek().len(), 1);
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn tick_on_empty_returns_none() {
        let mut queue: BatchQueue<String> = BatchQueue::new("test", QueueConfig::default());
        assert!(queue.tick().is_none());
    }

    #[tokio::test]
    async fn tick_emits_after_window() {
        let config = QueueConfig {
            batch_threshold: 100, // high threshold so only window triggers
            batch_window_secs: 1, // 1 second window
            ..Default::default()
        };
        let mut queue = BatchQueue::new("test", config);

        queue.push(make_item("1", "a"));
        queue.push(make_item("2", "b"));

        // Not yet
        assert!(queue.tick().is_none());

        // Wait for the window
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let event = queue.tick();
        assert!(event.is_some());
        assert_eq!(event.unwrap().len(), 2);
        assert!(queue.is_empty());
    }

    #[test]
    fn into_items_works_for_both_variants() {
        let single = QueueEvent::Single(make_item("1", "s"));
        assert_eq!(single.into_items().len(), 1);

        let batch = QueueEvent::Batch(vec![make_item("1", "a"), make_item("2", "b")]);
        assert_eq!(batch.into_items().len(), 2);
    }

    #[test]
    fn process_delay_from_config() {
        let config = QueueConfig {
            process_delay_secs: 45,
            ..Default::default()
        };
        let queue: BatchQueue<String> = BatchQueue::new("test", config);
        assert_eq!(queue.process_delay(), std::time::Duration::from_secs(45));
    }
}
