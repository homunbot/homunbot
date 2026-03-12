//! Browser action policy — category-based allow/deny rules.
//!
//! Checked in the agent loop *before* the browser tool executes, right
//! after the browser-task-planner veto.  Returns `Some(reason)` to deny
//! an action, `None` to allow it.

use crate::config::BrowserPolicyConfig;

/// Map a browser action name to its policy category.
fn action_category(action: &str) -> &'static str {
    match action {
        "navigate" => "navigate",
        "click" | "click_coordinates" => "click",
        "type" | "fill" | "select_option" | "press_key" => "fill",
        "snapshot" | "screenshot" => "observe",
        "hover" | "scroll" | "drag" => "interact",
        "evaluate" => "eval",
        "tab_list" | "tab_new" | "tab_select" | "tab_close" => "tabs",
        "block_resources" | "unblock_resources" => "network",
        "close" | "wait" => "_internal",
        _ => "unknown",
    }
}

/// Check whether a browser action is allowed by the configured policy.
///
/// Returns `Some(reason)` to deny, `None` to allow.
pub fn check_browser_policy(
    policy: &BrowserPolicyConfig,
    action: &str,
    args: &serde_json::Value,
) -> Option<String> {
    if !policy.enabled {
        return None;
    }

    let category = action_category(action);

    // Internal actions always allowed.
    if category == "_internal" {
        return None;
    }

    // Navigate: check URL patterns before category rules.
    if action == "navigate" {
        if let Some(url) = args.get("url").and_then(|v| v.as_str()) {
            // Blocked URLs always deny.
            for pattern in &policy.blocked_urls {
                if url_matches_pattern(url, pattern) {
                    return Some(format!(
                        "Policy: navigate to \"{url}\" blocked (matches \"{pattern}\")"
                    ));
                }
            }
            // In deny-default mode, allowed_urls is the whitelist.
            if policy.default == "deny"
                && !policy.allowed_urls.is_empty()
                && !policy.allowed_urls.iter().any(|p| url_matches_pattern(url, p))
            {
                return Some(format!(
                    "Policy: navigate to \"{url}\" denied — not in allowed_urls"
                ));
            }
        }
    }

    // Category deny list (takes precedence).
    if policy.deny.iter().any(|c| c == category) {
        return Some(format!(
            "Policy: action \"{action}\" denied (category \"{category}\" is blocked)"
        ));
    }

    // Category allow list.
    if policy.allow.iter().any(|c| c == category) {
        return None;
    }

    // Fall back to default.
    if policy.default == "deny" {
        Some(format!(
            "Policy: action \"{action}\" denied (category \"{category}\", default=deny)"
        ))
    } else {
        None
    }
}

/// Simple glob-style URL pattern matching (no external crate).
///
/// - `"*.evil.com"` — matches hosts ending with `.evil.com` (or exactly `evil.com`)
/// - `"example.com"` — substring match anywhere in the URL
fn url_matches_pattern(url: &str, pattern: &str) -> bool {
    let pattern = pattern.trim();
    if let Some(suffix) = pattern.strip_prefix("*.") {
        // Host suffix match: strip scheme, extract host.
        let host = url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("");
        host.ends_with(&format!(".{suffix}")) || host == suffix
    } else {
        url.contains(pattern)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn policy(default: &str, allow: &[&str], deny: &[&str]) -> BrowserPolicyConfig {
        BrowserPolicyConfig {
            enabled: true,
            default: default.to_string(),
            allow: allow.iter().map(|s| s.to_string()).collect(),
            deny: deny.iter().map(|s| s.to_string()).collect(),
            blocked_urls: Vec::new(),
            allowed_urls: Vec::new(),
        }
    }

    #[test]
    fn disabled_allows_everything() {
        let p = BrowserPolicyConfig::default(); // enabled = false
        assert!(check_browser_policy(&p, "evaluate", &json!({})).is_none());
        assert!(check_browser_policy(&p, "navigate", &json!({"url": "https://evil.com"})).is_none());
    }

    #[test]
    fn default_allow_denies_listed() {
        let p = policy("allow", &[], &["eval", "network"]);
        assert!(check_browser_policy(&p, "evaluate", &json!({})).is_some());
        assert!(check_browser_policy(&p, "block_resources", &json!({})).is_some());
        assert!(check_browser_policy(&p, "click", &json!({})).is_none());
        assert!(check_browser_policy(&p, "navigate", &json!({"url": "https://ok.com"})).is_none());
    }

    #[test]
    fn default_deny_allows_listed() {
        let p = policy("deny", &["navigate", "observe"], &[]);
        assert!(check_browser_policy(&p, "navigate", &json!({"url": "https://ok.com"})).is_none());
        assert!(check_browser_policy(&p, "snapshot", &json!({})).is_none());
        assert!(check_browser_policy(&p, "click", &json!({})).is_some());
    }

    #[test]
    fn internal_always_allowed() {
        let p = policy("deny", &[], &[]);
        assert!(check_browser_policy(&p, "close", &json!({})).is_none());
        assert!(check_browser_policy(&p, "wait", &json!({})).is_none());
    }

    #[test]
    fn blocked_urls_deny_navigate() {
        let mut p = policy("allow", &[], &[]);
        p.blocked_urls = vec!["*.evil.com".to_string()];

        let blocked = json!({"url": "https://sub.evil.com/page"});
        assert!(check_browser_policy(&p, "navigate", &blocked).is_some());

        let ok = json!({"url": "https://good.com"});
        assert!(check_browser_policy(&p, "navigate", &ok).is_none());
    }

    #[test]
    fn allowed_urls_in_deny_mode() {
        let mut p = policy("deny", &["navigate"], &[]);
        p.allowed_urls = vec!["*.mysite.com".to_string()];

        let ok = json!({"url": "https://app.mysite.com/dash"});
        assert!(check_browser_policy(&p, "navigate", &ok).is_none());

        let blocked = json!({"url": "https://other.com"});
        assert!(check_browser_policy(&p, "navigate", &blocked).is_some());
    }

    #[test]
    fn url_pattern_matching() {
        assert!(url_matches_pattern("https://sub.evil.com/page", "*.evil.com"));
        assert!(url_matches_pattern("https://evil.com/page", "*.evil.com"));
        assert!(!url_matches_pattern("https://notevil.com", "*.evil.com"));
        assert!(url_matches_pattern("https://example.com/search", "example.com"));
        assert!(!url_matches_pattern("https://other.com", "example.com"));
    }

    #[test]
    fn category_mapping_exhaustive() {
        let all_actions = [
            "navigate", "click", "click_coordinates", "type", "fill",
            "select_option", "press_key", "snapshot", "screenshot",
            "hover", "scroll", "drag", "evaluate", "tab_list",
            "tab_new", "tab_select", "tab_close", "block_resources",
            "unblock_resources", "close", "wait",
        ];
        for action in &all_actions {
            assert_ne!(action_category(action), "unknown", "unmapped: {action}");
        }
    }

    #[test]
    fn unknown_action_uses_default() {
        let deny = policy("deny", &[], &[]);
        assert!(check_browser_policy(&deny, "nonexistent", &json!({})).is_some());

        let allow = policy("allow", &[], &[]);
        assert!(check_browser_policy(&allow, "nonexistent", &json!({})).is_none());
    }
}
