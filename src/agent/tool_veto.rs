//! Tool call veto system — minimal runtime safety checks.
//!
//! With cognition always active, these are lightweight guardrails:
//! - Search-first: web_fetch should follow web_search unless a URL is given
//! - Shell not for web: don't use shell for web lookups when web tools exist

use std::collections::HashSet;

/// Check if a tool call should be vetoed based on runtime safety policies.
///
/// Returns `Some(reason)` if the call should be blocked, `None` to allow it.
/// Only minimal safety-net checks — cognition already selected the right tools.
pub(crate) fn veto_tool_call(
    tool_name: &str,
    user_prompt: &str,
    available_tool_names: &HashSet<String>,
    tools_already_used: &[String],
) -> Option<String> {
    let text = user_prompt.to_ascii_lowercase();
    let has_web_search = available_tool_names.contains("web_search");
    let has_known_url =
        text.contains("http://") || text.contains("https://") || text.contains("www.");

    // Search-first: web_fetch should not be used before web_search
    // unless the user explicitly gave a URL to read.
    if tool_name == "web_fetch" && has_web_search && !has_known_url {
        let web_search_already_tried = tools_already_used.iter().any(|t| t == "web_search");
        if !web_search_already_tried {
            return Some(
                "Tool vetoed: use web_search first to find the right source, \
                then use web_fetch on the most relevant result URL. \
                Direct web_fetch is only appropriate when the user explicitly provides a URL."
                    .to_string(),
            );
        }
    }

    None
}
