pub mod registry;
pub mod shell;
pub mod file;
pub mod web;
pub mod cron;
pub mod spawn;
pub mod message;
pub mod mcp;
pub mod vault;

#[cfg(feature = "local-embeddings")]
pub mod remember;

pub use registry::{Tool, ToolContext, ToolRegistry, ToolResult};
pub use shell::ShellTool;
pub use file::{ReadFileTool, WriteFileTool, EditFileTool, ListDirTool};
pub use web::{WebSearchTool, WebFetchTool};
pub use cron::CronTool;
pub use spawn::SpawnTool;
pub use message::MessageTool;
pub use mcp::{McpManager, McpServerInfo};
pub use vault::VaultTool;

#[cfg(feature = "local-embeddings")]
pub use remember::RememberTool;

#[cfg(feature = "browser")]
pub use crate::browser::BrowserTool;
