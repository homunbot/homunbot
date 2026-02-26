//! Browser automation module for Homun.
//!
//! Provides browser control capabilities using chromiumoxide (CDP).
//! The LLM can navigate pages, interact with elements, and extract content.
//!
//! # Architecture
//!
//! - `manager`: Browser lifecycle management (singleton)
//! - `snapshot`: Page snapshot generation with element refs
//! - `actions`: Browser action types
//! - `tool`: BrowserTool implementation for the Tool trait
//!
//! # Usage
//!
//! The browser tool is automatically registered when browser is enabled in config:
//!
//! ```toml
//! [browser]
//! enabled = true
//! headless = true
//! ```

pub mod manager;
pub mod snapshot;
pub mod actions;
pub mod tool;

// Re-export config from schema
pub use crate::config::BrowserConfig;
pub use manager::{BrowserManager, global_browser_manager};
pub use snapshot::{PageSnapshot, ElementRef};
pub use actions::BrowserAction;
pub use tool::BrowserTool;
