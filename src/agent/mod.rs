mod context;
mod agent_loop;
pub mod bootstrap_watcher;
pub mod embeddings;
pub mod gateway;
pub mod heartbeat;
pub mod memory;
pub mod memory_search;
pub mod prompt;  // New modular prompt system
pub mod subagent;  // Make public so spawn.rs can access it
mod verifier;

pub use agent_loop::AgentLoop;
pub use bootstrap_watcher::{BootstrapWatcher, WatcherHandle as BootstrapWatcherHandle, BootstrapContent, BootstrapFiles};
pub use context::ContextBuilder;
pub use embeddings::EmbeddingEngine;
pub use gateway::Gateway;
pub use heartbeat::HeartbeatService;
pub use memory::MemoryConsolidator;
pub use memory_search::MemorySearcher;
pub use prompt::{PromptContext, PromptMode, PromptSection, SystemPromptBuilder, ToolInfo};
pub use subagent::SubagentManager;
