//! Browser action types for the browser tool.

use serde::{Deserialize, Serialize};

/// Types of waiting for page state changes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WaitType {
    /// Wait for an element to appear (selector in value)
    Selector,
    /// Wait for specific text to appear
    Text,
    /// Wait for URL to change/match
    Url,
    /// Wait for a fixed duration (seconds in value)
    Time,
    /// Wait for element to be visible (ref_id in value)
    Visible,
    /// Wait for element to be hidden (ref_id in value)
    Hidden,
    /// Wait for element to be enabled (ref_id in value)
    Enabled,
    /// Wait for network idle (ms without requests in value)
    NetworkIdle,
}

/// Default console level filter
fn default_console_level() -> String {
    "all".to_string()
}

/// Browser actions that can be performed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BrowserAction {
    /// Navigate to a URL
    Navigate {
        /// URL to navigate to
        url: String,
    },

    /// Take a snapshot of the current page (get interactive elements)
    Snapshot {
        /// Also save a screenshot
        #[serde(default)]
        screenshot: bool,
    },

    /// Click on an element by its ref_id
    Click {
        /// Element reference ID from snapshot (e.g., "e1", "e2")
        ref_id: String,
    },

    /// Type text into an input element
    Type {
        /// Element reference ID from snapshot
        ref_id: String,
        /// Text to type (supports vault://key for secret values)
        text: String,
        /// Press Enter after typing
        #[serde(default)]
        submit: bool,
        /// Type slowly character by character (for key handlers)
        #[serde(default)]
        slowly: bool,
    },

    /// Select an option in a dropdown
    Select {
        /// Element reference ID from snapshot
        ref_id: String,
        /// Option value or text to select
        option: String,
    },

    /// Wait for a page state change
    Wait {
        /// What to wait for
        wait_type: WaitType,
        /// Value to wait for (selector, text, URL, or seconds)
        value: String,
    },

    /// Take a screenshot of the page
    Screenshot {
        /// Capture full page (not just viewport)
        #[serde(default)]
        full_page: bool,
    },

    /// Execute JavaScript in the page
    Evaluate {
        /// JavaScript code to execute
        code: String,
    },

    /// Navigate back in history
    Back,

    /// Navigate forward in history
    Forward,

    /// Close the current page/tab (or a specific tab by target_id)
    Close {
        /// Optional target ID of the tab to close (closes current tab if not specified)
        #[serde(default)]
        target_id: Option<String>,
    },

    /// List all open tabs
    Tabs,

    /// Open a new tab
    OpenTab {
        /// URL to open in the new tab (optional, defaults to about:blank)
        #[serde(default)]
        url: Option<String>,
    },

    /// Focus/switch to a specific tab
    FocusTab {
        /// Target ID of the tab to focus
        target_id: String,
    },

    /// Get console messages and page errors
    Console {
        /// Clear messages after retrieving them
        #[serde(default)]
        clear: bool,
        /// Filter by level: "all", "error", "warn", "info", "log"
        #[serde(default = "default_console_level")]
        level: String,
    },

    /// Scroll the page
    Scroll {
        /// Scroll direction: "up", "down", "top", "bottom"
        direction: String,
    },

    /// Hover over an element
    Hover {
        /// Element reference ID from snapshot
        ref_id: String,
    },

    /// Accept privacy/cookie consent banner automatically
    ///
    /// Searches for common consent button patterns and clicks them.
    /// Returns success if a button was found and clicked, or info if no banner detected.
    AcceptPrivacy,

    /// Press a single key (e.g., Enter, Escape, ArrowDown, Tab)
    Press {
        /// Key to press (e.g., "Enter", "Escape", "ArrowDown", "Tab", "Backspace")
        key: String,
    },

    /// Drag and drop from one element to another
    Drag {
        /// Source element reference ID from snapshot
        source_ref_id: String,
        /// Target element reference ID from snapshot
        target_ref_id: String,
    },

    /// Fill multiple form fields at once
    Fill {
        /// Array of field ref_id and value pairs: [{"ref_id": "e1", "value": "text"}, ...]
        fields: Vec<FillField>,
    },

    /// Resize the browser viewport
    Resize {
        /// Viewport width in pixels
        width: u32,
        /// Viewport height in pixels
        height: u32,
    },

    /// Handle a dialog (alert, confirm, prompt)
    Dialog {
        /// Whether to accept (true) or dismiss (false) the dialog
        accept: bool,
        /// Text to enter for prompt dialogs (optional)
        #[serde(default)]
        prompt_text: Option<String>,
    },

    /// Upload a file to a file input element
    Upload {
        /// Element reference ID from snapshot (must be a file input)
        ref_id: String,
        /// Path to the file to upload
        file_path: String,
    },

    /// Save the current page as PDF
    Pdf {
        /// Output path for the PDF file (optional, defaults to ~/Downloads/page.pdf)
        #[serde(default)]
        path: Option<String>,
        /// Paper width in inches (default: 8.5)
        #[serde(default)]
        width: Option<f64>,
        /// Paper height in inches (default: 11)
        #[serde(default)]
        height: Option<f64>,
        /// Print background graphics (default: true)
        #[serde(default = "default_true")]
        print_background: bool,
        /// Landscape orientation (default: false)
        #[serde(default)]
        landscape: bool,
    },

    /// Get network requests (HTTP requests/responses)
    Network {
        /// Clear captured requests after retrieving
        #[serde(default)]
        clear: bool,
        /// Filter by URL pattern (optional)
        #[serde(default)]
        url_filter: Option<String>,
    },

    /// Shutdown the browser completely (closes all tabs and browser process)
    /// Use this to free all browser resources when you're completely done.
    Shutdown,
}

/// Default true helper
fn default_true() -> bool {
    true
}

/// A field to fill in a form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FillField {
    /// Element reference ID from snapshot
    pub ref_id: String,
    /// Value to fill (supports vault://key for secret values)
    pub value: String,
}

impl BrowserAction {
    /// Get the action name as a string.
    pub fn action_name(&self) -> &'static str {
        match self {
            BrowserAction::Navigate { .. } => "navigate",
            BrowserAction::Snapshot { .. } => "snapshot",
            BrowserAction::Click { .. } => "click",
            BrowserAction::Type { .. } => "type",
            BrowserAction::Select { .. } => "select",
            BrowserAction::Wait { .. } => "wait",
            BrowserAction::Screenshot { .. } => "screenshot",
            BrowserAction::Evaluate { .. } => "evaluate",
            BrowserAction::Back => "back",
            BrowserAction::Forward => "forward",
            BrowserAction::Close { .. } => "close",
            BrowserAction::Tabs => "tabs",
            BrowserAction::OpenTab { .. } => "open_tab",
            BrowserAction::FocusTab { .. } => "focus_tab",
            BrowserAction::Console { .. } => "console",
            BrowserAction::Scroll { .. } => "scroll",
            BrowserAction::Hover { .. } => "hover",
            BrowserAction::AcceptPrivacy => "accept_privacy",
            BrowserAction::Press { .. } => "press",
            BrowserAction::Drag { .. } => "drag",
            BrowserAction::Fill { .. } => "fill",
            BrowserAction::Resize { .. } => "resize",
            BrowserAction::Dialog { .. } => "dialog",
            BrowserAction::Upload { .. } => "upload",
            BrowserAction::Pdf { .. } => "pdf",
            BrowserAction::Network { .. } => "network",
            BrowserAction::Shutdown => "shutdown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_navigate() {
        let action = BrowserAction::Navigate {
            url: "https://example.com".to_string(),
        };
        assert_eq!(action.action_name(), "navigate");
    }

    #[test]
    fn test_action_type() {
        let action = BrowserAction::Type {
            ref_id: "e1".to_string(),
            text: "hello".to_string(),
            submit: true,
            slowly: false,
        };
        assert_eq!(action.action_name(), "type");
    }

    #[test]
    fn test_wait_type_deserialization() {
        let wt: WaitType = serde_json::from_str("\"selector\"").unwrap();
        assert_eq!(wt, WaitType::Selector);

        let wt: WaitType = serde_json::from_str("\"time\"").unwrap();
        assert_eq!(wt, WaitType::Time);
    }
}
