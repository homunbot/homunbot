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

/// Redact vault values from text,///
/// Replaces any vault value found in the text with `vault://key_name` reference.
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

        // Replace the value with vault reference
        // Use literal replacement to avoid regex escaping issues
        let vault_ref = format!("vault://{}", key);
        result = result.replace(value, &vault_ref);
    }

    result
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
}
