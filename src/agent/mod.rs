mod agent_loop;
pub mod bootstrap_watcher;
mod context;
pub mod email_approval;
pub mod gateway;
pub mod heartbeat;
pub mod memory;
pub mod prompt; // New modular prompt system
pub mod subagent; // Make public so spawn.rs can access it
mod verifier;

#[cfg(feature = "local-embeddings")]
pub mod embeddings;

#[cfg(feature = "local-embeddings")]
pub mod memory_search;

pub use agent_loop::AgentLoop;
pub use bootstrap_watcher::{
    BootstrapContent, BootstrapFiles, BootstrapWatcher, WatcherHandle as BootstrapWatcherHandle,
};
pub use context::ContextBuilder;
pub use gateway::Gateway;
pub use heartbeat::HeartbeatService;
pub use memory::MemoryConsolidator;
pub use prompt::{PromptContext, PromptMode, PromptSection, SystemPromptBuilder, ToolInfo};
pub use subagent::SubagentManager;

#[cfg(feature = "local-embeddings")]
pub use embeddings::EmbeddingEngine;

#[cfg(feature = "local-embeddings")]
pub use memory_search::MemorySearcher;
