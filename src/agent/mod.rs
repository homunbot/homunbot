mod agent_loop;
mod attachment_router;
pub mod bootstrap_watcher;
mod browser_task_plan;
mod context;
pub mod debounce;
pub mod email_approval;
mod execution_plan;
pub mod gateway;
pub mod heartbeat;
pub mod memory;
pub mod prompt; // New modular prompt system
pub mod stop;
pub mod subagent; // Make public so spawn.rs can access it
mod verifier;

#[cfg(feature = "embeddings")]
pub mod embeddings;

#[cfg(feature = "embeddings")]
pub mod memory_search;

pub use agent_loop::AgentLoop;
pub use bootstrap_watcher::{
    BootstrapContent, BootstrapFiles, BootstrapWatcher, WatcherHandle as BootstrapWatcherHandle,
};
pub use browser_task_plan::BrowserTaskPlanState;
pub use context::ContextBuilder;
pub use execution_plan::{ExecutionPlanSnapshot, ExecutionPlanState};
pub use gateway::Gateway;
pub use heartbeat::HeartbeatService;
pub use memory::MemoryConsolidator;
pub use prompt::{PromptContext, PromptMode, PromptSection, SystemPromptBuilder, ToolInfo};
pub use subagent::SubagentManager;

#[cfg(feature = "embeddings")]
pub use embeddings::{create_embedding_provider, EmbeddingEngine};

#[cfg(feature = "embeddings")]
pub use memory_search::MemorySearcher;
