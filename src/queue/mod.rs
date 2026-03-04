//! Generic batching queue system.
//!
//! Provides `BatchQueue<T>` — a queue that accumulates items and emits them
//! as single events or batches based on configurable thresholds and time windows.
//!
//! Designed to be reusable across the project: email digests, cron batching,
//! webhook grouping, alert deduplication, etc.

mod batch;
mod config;

pub use batch::{BatchQueue, QueueEvent, QueueItem};
pub use config::QueueConfig;
