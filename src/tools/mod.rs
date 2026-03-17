pub mod approval;
pub mod automation;
#[cfg(feature = "browser")]
pub mod browser;
pub mod business;
pub mod cron;
#[cfg(feature = "channel-email")]
pub mod email_inbox;
pub mod file;
#[cfg(feature = "mcp")]
pub mod mcp;
#[cfg(feature = "mcp")]
pub mod mcp_token_refresh;
pub mod message;
pub mod registry;
pub mod sandbox;
pub mod shell;
pub mod skill_create;
pub mod spawn;
pub mod vault;
pub mod web;
pub mod workflow;

#[cfg(feature = "embeddings")]
pub mod knowledge;
#[cfg(feature = "embeddings")]
pub mod remember;

pub use approval::{
    global_approval_manager, init_approval_manager, ApprovalDecision, ApprovalId, ApprovalLogEntry,
    ApprovalManager, ApprovalResponse, PendingApproval,
};
pub use automation::CreateAutomationTool;
#[cfg(feature = "browser")]
pub use browser::{BrowserSession, BrowserTool};
pub use business::BusinessTool;
pub use cron::CronTool;
#[cfg(feature = "channel-email")]
pub use email_inbox::ReadEmailInboxTool;
pub use file::{EditFileTool, ListDirTool, ReadFileTool, WriteFileTool};
#[cfg(feature = "mcp")]
pub use mcp::{McpManager, McpPeer, McpServerInfo};
pub use message::MessageTool;
pub use registry::{Tool, ToolContext, ToolRegistry, ToolResult};
pub use shell::ShellTool;
pub use skill_create::CreateSkillTool;
pub use spawn::SpawnTool;
pub use vault::VaultTool;
pub use web::{WebFetchTool, WebSearchTool};
pub use workflow::WorkflowTool;

#[cfg(feature = "embeddings")]
pub use knowledge::KnowledgeTool;
#[cfg(feature = "embeddings")]
pub use remember::RememberTool;
