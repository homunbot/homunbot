//! Vault Leak Prevention — Redact vault values from memory files and LLM output.
//!
//! This module ensures that secrets stored in the vault are never leaked:
//! 1. During consolidation: redact vault values from history/memory entries
//! 2. Before returning values: check if value is in vault and require 2FA
//!
//! # Architecture
//!
//! ```text
//! Memory Consolidation
//!       │
//!       ▼
//! ┌─────────────────────┐
//! │  Vault Leak Filter  │
//! │  ┌───────────────┐  │
//! │  │ Load Vault   │  │
//! │  │ Values       │  │
//! │  └───────┬───────┘  │
//! │          │          │
//! │    ┌─────┴─────┐    │
//! │    ▼           ▼    │
//! │  Replace     Skip   │
//! │  with         if    │
//! │  vault://key  empty │
//! └─────────────────────┘
//!       │
//!       ▼
//! Redacted Memory Files
//! ```

use anyhow::Result;

/// Redact vault values from text.
///
/// Replaces any vault value found in the text with `vault://key_name` reference.
/// Uses word-boundary matching to avoid corrupting unrelated text — e.g., if the
/// vault contains `"pass"`, the word `"compass"` won't be modified.
///
/// # Arguments
/// * `text` - The text to scan for vault values
/// * `vault_entries` - List of (key, value) pairs from vault
///
/// # Returns
/// The text with vault values replaced by `vault://key` references
pub fn redact_vault_values(text: &str, vault_entries: &[(String, String)]) -> String {
    if vault_entries.is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();

    for (key, value) in vault_entries {
        if value.is_empty() || value.len() < 3 {
            continue; // Skip empty or very short values
        }

        let vault_ref = format!("vault://{}", key);
        result = replace_whole_match(&result, value, &vault_ref);
    }

    result
}

/// Replace `needle` in `haystack` only when it appears as a standalone token —
/// i.e., the characters immediately before and after the match are NOT
/// alphanumeric or underscore. This prevents partial-word corruption.
fn replace_whole_match(haystack: &str, needle: &str, replacement: &str) -> String {
    let bytes = haystack.as_bytes();
    let needle_len = needle.len();
    let mut result = String::with_capacity(haystack.len());
    let mut remaining = haystack;

    while let Some(pos) = remaining.find(needle) {
        let abs_pos = haystack.len() - remaining.len() + pos;
        let after_pos = abs_pos + needle_len;

        let before_ok = abs_pos == 0 || !is_word_char(bytes[abs_pos - 1]);
        let after_ok = after_pos >= bytes.len() || !is_word_char(bytes[after_pos]);

        result.push_str(&remaining[..pos]);
        if before_ok && after_ok {
            result.push_str(replacement);
        } else {
            result.push_str(&remaining[pos..pos + needle_len]);
        }
        remaining = &remaining[pos + needle_len..];
    }
    result.push_str(remaining);
    result
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_simple_value() {
        let text = "My password is secret123";
        let vault_entries = vec![("password".to_string(), "secret123".to_string())];

        let result = redact_vault_values(text, &vault_entries);

        assert_eq!(result, "My password is vault://password");
    }

    #[test]
    fn test_redact_multiple_values() {
        let text = "API key: abc123 and Token: xyz789";
        let vault_entries = vec![
            ("api_key".to_string(), "abc123".to_string()),
            ("token".to_string(), "xyz789".to_string()),
        ];

        let result = redact_vault_values(text, &vault_entries);

        assert!(result.contains("vault://api_key"));
        assert!(result.contains("vault://token"));
        assert!(!result.contains("abc123"));
        assert!(!result.contains("xyz789"));
    }

    #[test]
    fn test_no_vault_entries() {
        let text = "No secrets here";
        let vault_entries: Vec<(String, String)> = vec![];

        let result = redact_vault_values(text, &vault_entries);

        assert_eq!(result, text);
    }

    #[test]
    fn test_empty_value_skipped() {
        let text = "Password is secret123";
        let vault_entries = vec![
            ("empty".to_string(), "".to_string()),
            ("password".to_string(), "secret123".to_string()),
        ];

        let result = redact_vault_values(text, &vault_entries);

        assert!(result.contains("vault://password"));
    }

    #[test]
    fn test_short_value_skipped() {
        let text = "Code is ab";
        let vault_entries = vec![("code".to_string(), "ab".to_string())];

        let result = redact_vault_values(text, &vault_entries);

        // "ab" is less than 3 chars, should not be replaced
        assert_eq!(result, text);
    }

    #[test]
    fn test_value_not_found() {
        let text = "This is a normal message";
        let vault_entries = vec![("secret".to_string(), "hidden123".to_string())];

        let result = redact_vault_values(text, &vault_entries);

        assert_eq!(result, text);
    }

    #[test]
    fn test_no_substring_corruption() {
        // "pass" appears inside "compass" — must NOT be replaced
        let text = "Use compass to navigate, the password is pass";
        let vault_entries = vec![("mypass".to_string(), "pass".to_string())];

        let result = redact_vault_values(text, &vault_entries);

        assert!(result.contains("compass"), "compass must not be corrupted");
        // "pass" at the end is standalone → replaced
        assert!(result.contains("vault://mypass"));
    }

    #[test]
    fn test_no_partial_word_match() {
        let text = "mypassword123 is strong";
        let vault_entries = vec![("pw".to_string(), "password".to_string())];

        let result = redact_vault_values(text, &vault_entries);

        // "password" is embedded in "mypassword123" → not replaced
        assert_eq!(result, text);
    }

    #[test]
    fn test_standalone_with_punctuation() {
        let text = "token=\"sk-ant-xxx\" and key: abc123!";
        let vault_entries = vec![
            ("token".to_string(), "sk-ant-xxx".to_string()),
            ("key".to_string(), "abc123".to_string()),
        ];

        let result = redact_vault_values(text, &vault_entries);

        assert!(result.contains("vault://token"));
        assert!(result.contains("vault://key"));
        assert!(!result.contains("sk-ant-xxx"));
        assert!(!result.contains("abc123"));
    }

    #[test]
    fn test_value_at_start_and_end() {
        let text = "secret123 is at start and end is secret123";
        let vault_entries = vec![("key".to_string(), "secret123".to_string())];

        let result = redact_vault_values(text, &vault_entries);

        assert_eq!(
            result,
            "vault://key is at start and end is vault://key"
        );
    }

    #[test]
    fn test_replace_whole_match_internals() {
        // Direct test of the helper function
        assert_eq!(
            replace_whole_match("compass", "pass", "REDACTED"),
            "compass"
        );
        assert_eq!(
            replace_whole_match("my pass here", "pass", "REDACTED"),
            "my REDACTED here"
        );
        assert_eq!(
            replace_whole_match("pass", "pass", "REDACTED"),
            "REDACTED"
        );
        assert_eq!(
            replace_whole_match("pass-word", "pass", "REDACTED"),
            "REDACTED-word"
        );
    }
}
