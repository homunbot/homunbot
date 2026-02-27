pub mod approval;
pub mod cron;
pub mod file;
#[cfg(feature = "mcp")]
pub mod mcp;
pub mod message;
pub mod registry;
pub mod shell;
pub mod spawn;
pub mod vault;
pub mod web;

#[cfg(feature = "local-embeddings")]
pub mod remember;

pub use approval::{
    global_approval_manager, init_approval_manager, ApprovalDecision, ApprovalId, ApprovalLogEntry,
    ApprovalManager, ApprovalResponse, PendingApproval,
};
pub use cron::CronTool;
pub use file::{EditFileTool, ListDirTool, ReadFileTool, WriteFileTool};
#[cfg(feature = "mcp")]
pub use mcp::{McpManager, McpServerInfo};
pub use message::MessageTool;
pub use registry::{Tool, ToolContext, ToolRegistry, ToolResult};
pub use shell::ShellTool;
pub use spawn::SpawnTool;
pub use vault::VaultTool;
pub use web::{WebFetchTool, WebSearchTool};

#[cfg(feature = "local-embeddings")]
pub use remember::RememberTool;

#[cfg(feature = "browser")]
pub use crate::browser::BrowserTool;
