//! Helper functions for identifying browser tools.
//!
//! With the unified `BrowserTool`, there is now a single tool named `"browser"`.
//! The `is_browser_tool()` function also matches the old `playwright__browser_*`
//! names so that `main.rs` can filter them out during MCP tool registration.

use std::collections::HashSet;

use super::mcp_bridge::BROWSER_MCP_SERVER_NAME;

/// Prefix shared by individual Playwright MCP tools (before unification).
///
/// Still needed to filter them out during registration in `main.rs`.
fn browser_mcp_tool_prefix() -> String {
    format!("{BROWSER_MCP_SERVER_NAME}__browser_")
}

/// Returns `true` if `name` is a browser-related tool.
///
/// Matches both:
/// - `"browser"` — the unified BrowserTool
/// - `"playwright__browser_*"` — individual MCP tools (for filtering during registration)
pub fn is_browser_tool(name: &str) -> bool {
    name == "browser" || name.starts_with(&browser_mcp_tool_prefix())
}

/// `true` if at least one tool in `names` is a browser tool.
pub fn has_browser_tools(names: &HashSet<String>) -> bool {
    names.iter().any(|n| is_browser_tool(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_browser_tool() {
        // Unified tool
        assert!(is_browser_tool("browser"));
        // Individual MCP tools (filtered during registration)
        assert!(is_browser_tool("playwright__browser_navigate"));
        assert!(is_browser_tool("playwright__browser_click"));
        assert!(is_browser_tool("playwright__browser_take_screenshot"));
        // Non-browser
        assert!(!is_browser_tool("web_search"));
        assert!(!is_browser_tool("shell"));
    }

    #[test]
    fn test_has_browser_tools() {
        let with_unified = HashSet::from([
            "web_search".to_string(),
            "browser".to_string(),
        ]);
        assert!(has_browser_tools(&with_unified));

        let with_mcp = HashSet::from([
            "web_search".to_string(),
            "playwright__browser_navigate".to_string(),
        ]);
        assert!(has_browser_tools(&with_mcp));

        let without = HashSet::from(["web_search".to_string(), "shell".to_string()]);
        assert!(!has_browser_tools(&without));
    }
}
