use serde::{Deserialize, Serialize};

/// Configuration for a `BatchQueue`.
///
/// Controls when items are emitted as a batch vs single events,
/// and the delay between processing successive items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    /// How many items trigger an immediate batch emission.
    /// Default: 3
    #[serde(default = "default_batch_threshold")]
    pub batch_threshold: u32,

    /// Time window in seconds to accumulate items before emitting.
    /// After the first item arrives, the queue waits this long for more items.
    /// Default: 120 (2 minutes)
    #[serde(default = "default_batch_window_secs")]
    pub batch_window_secs: u64,

    /// Delay in seconds between processing successive items from a batch.
    /// Prevents flooding the recipient with rapid-fire messages.
    /// Default: 30
    #[serde(default = "default_process_delay_secs")]
    pub process_delay_secs: u64,
}

fn default_batch_threshold() -> u32 {
    3
}
fn default_batch_window_secs() -> u64 {
    120
}
fn default_process_delay_secs() -> u64 {
    30
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            batch_threshold: default_batch_threshold(),
            batch_window_secs: default_batch_window_secs(),
            process_delay_secs: default_process_delay_secs(),
        }
    }
}
