use anyhow::{Context, Result};
use serde_json::Value;

use super::Config;

/// Get a config value by dot-path (e.g., "agent.model", "providers.anthropic.api_key")
pub fn config_get(config: &Config, key: &str) -> Result<String> {
    let json = serde_json::to_value(config).context("Failed to serialize config")?;
    let value = navigate_path(&json, key)?;
    Ok(format_value(value, key))
}

/// Set a config value by dot-path. Returns the updated Config.
/// The value string is auto-coerced: "true"/"false" → bool, numbers → number, else → string.
pub fn config_set(config: &mut Config, key: &str, value: &str) -> Result<()> {
    config_set_value(config, key, coerce_value(value))
}

/// Set a config value by dot-path using a pre-built JSON value.
/// Use this for non-scalar values (arrays, objects) or when the value is already typed.
pub fn config_set_value(config: &mut Config, key: &str, value: serde_json::Value) -> Result<()> {
    let mut json = serde_json::to_value(&*config).context("Failed to serialize config")?;

    set_path(&mut json, key, value)?;

    // Deserialize back to validate the change
    let updated: Config =
        serde_json::from_value(json).context("Invalid value for this config key")?;
    *config = updated;
    Ok(())
}

/// List all config keys with their current values as flat dot-path pairs.
pub fn config_list_keys(config: &Config) -> Vec<(String, String)> {
    let json = match serde_json::to_value(config) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut pairs = Vec::new();
    flatten_value(&json, String::new(), &mut pairs);
    pairs
}

/// Navigate a JSON value by dot-path, returning a reference to the leaf.
fn navigate_path<'a>(value: &'a Value, path: &str) -> Result<&'a Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = value;

    for (i, part) in parts.iter().enumerate() {
        match current.get(part) {
            Some(v) => current = v,
            None => {
                let traversed = parts[..=i].join(".");
                anyhow::bail!("Key '{}' not found (failed at '{}')", path, traversed);
            }
        }
    }

    Ok(current)
}

/// Set a value at a dot-path in a JSON tree, creating intermediate objects if needed.
fn set_path(root: &mut Value, path: &str, new_value: Value) -> Result<()> {
    let parts: Vec<&str> = path.split('.').collect();

    if parts.is_empty() {
        anyhow::bail!("Empty key path");
    }

    let mut current = root;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            // Last part — set (or insert) the value.
            // We allow inserting keys that were omitted by skip_serializing_if;
            // deserialization back to Config will validate the result.
            match current {
                Value::Object(map) => {
                    map.insert(part.to_string(), new_value);
                    return Ok(());
                }
                _ => anyhow::bail!("Cannot set key '{}': parent is not an object", path),
            }
        } else {
            // Intermediate part — navigate deeper
            match current.get_mut(*part) {
                Some(v) => current = v,
                None => {
                    let traversed = parts[..=i].join(".");
                    anyhow::bail!("Key '{}' not found (failed at '{}')", path, traversed);
                }
            }
        }
    }

    unreachable!()
}

/// Auto-coerce a string value to the appropriate JSON type.
fn coerce_value(s: &str) -> Value {
    // Boolean
    if s.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if s.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }

    // Integer
    if let Ok(n) = s.parse::<i64>() {
        return Value::Number(n.into());
    }

    // Float
    if let Ok(f) = s.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return Value::Number(n);
        }
    }

    // String (default)
    Value::String(s.to_string())
}

/// Format a JSON value for display, masking sensitive fields.
fn format_value(value: &Value, key: &str) -> String {
    match value {
        Value::String(s) => {
            if is_sensitive_key(key) {
                mask_secret(s)
            } else {
                s.clone()
            }
        }
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "(not set)".to_string(),
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(|v| format_value(v, "")).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(_) => {
            // For nested objects, show a summary
            let mut pairs = Vec::new();
            flatten_value(value, String::new(), &mut pairs);
            if pairs.is_empty() {
                "{}".to_string()
            } else {
                pairs
                    .iter()
                    .map(|(k, v)| format!("  {k} = {v}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
    }
}

/// Check if a key refers to a sensitive field (api_key, token, etc.)
fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.contains("api_key")
        || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("bridge_token")
}

/// Mask a secret string, showing only the first 6 characters.
fn mask_secret(s: &str) -> String {
    if s.is_empty() {
        "(not set)".to_string()
    } else if s.len() <= 6 {
        "***".to_string()
    } else {
        format!("{}***", &s[..6])
    }
}

/// Recursively flatten a JSON value into dot-path key-value pairs.
fn flatten_value(value: &Value, prefix: String, pairs: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let new_prefix = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten_value(v, new_prefix, pairs);
            }
        }
        _ => {
            let display = format_value(value, &prefix);
            pairs.push((prefix, display));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_get_simple() {
        let config = Config::default();
        let result = config_get(&config, "agent.model").unwrap();
        assert_eq!(result, "anthropic/claude-sonnet-4-20250514");
    }

    #[test]
    fn test_config_get_number() {
        let config = Config::default();
        let result = config_get(&config, "agent.max_tokens").unwrap();
        assert_eq!(result, "8192");
    }

    #[test]
    fn test_config_get_bool() {
        let config = Config::default();
        let result = config_get(&config, "tools.exec.restrict_to_workspace").unwrap();
        assert_eq!(result, "false");
    }

    #[test]
    fn test_config_get_nested() {
        let mut config = Config::default();
        config.providers.anthropic.api_key = "sk-ant-test-123456789".to_string();
        let result = config_get(&config, "providers.anthropic.api_key").unwrap();
        assert_eq!(result, "sk-ant***"); // masked
    }

    #[test]
    fn test_config_get_invalid_key() {
        let config = Config::default();
        let result = config_get(&config, "nonexistent.key");
        assert!(result.is_err());
    }

    #[test]
    fn test_config_set_string() {
        let mut config = Config::default();
        config_set(&mut config, "agent.model", "openai/gpt-4o").unwrap();
        assert_eq!(config.agent.model, "openai/gpt-4o");
    }

    #[test]
    fn test_config_set_number() {
        let mut config = Config::default();
        config_set(&mut config, "agent.max_tokens", "4096").unwrap();
        assert_eq!(config.agent.max_tokens, 4096);
    }

    #[test]
    fn test_config_set_float() {
        let mut config = Config::default();
        config_set(&mut config, "agent.temperature", "0.5").unwrap();
        assert!((config.agent.temperature - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_config_set_bool() {
        let mut config = Config::default();
        config_set(&mut config, "tools.exec.restrict_to_workspace", "true").unwrap();
        assert!(config.tools.exec.restrict_to_workspace);
    }

    #[test]
    fn test_config_set_invalid_key() {
        let mut config = Config::default();
        let result = config_set(&mut config, "nonexistent.key", "value");
        assert!(result.is_err());
    }

    #[test]
    fn test_config_set_memory() {
        let mut config = Config::default();
        config_set(&mut config, "memory.conversation_retention_days", "60").unwrap();
        assert_eq!(config.memory.conversation_retention_days, 60);

        config_set(&mut config, "memory.history_retention_days", "180").unwrap();
        assert_eq!(config.memory.history_retention_days, 180);

        config_set(&mut config, "memory.auto_cleanup", "true").unwrap();
        assert!(config.memory.auto_cleanup);
    }

    #[test]
    fn test_config_get_memory() {
        let config = Config::default();
        let result = config_get(&config, "memory.conversation_retention_days").unwrap();
        assert_eq!(result, "30");
    }

    #[test]
    fn test_config_list_keys() {
        let config = Config::default();
        let keys = config_list_keys(&config);
        assert!(!keys.is_empty());
        // Should contain agent.model
        assert!(keys.iter().any(|(k, _)| k == "agent.model"));
        // Should contain tools.exec.timeout
        assert!(keys.iter().any(|(k, _)| k == "tools.exec.timeout"));
    }

    #[test]
    fn test_coerce_value_types() {
        assert_eq!(coerce_value("true"), Value::Bool(true));
        assert_eq!(coerce_value("false"), Value::Bool(false));
        assert_eq!(coerce_value("42"), Value::Number(42.into()));
        assert_eq!(coerce_value("hello"), Value::String("hello".to_string()));
    }

    #[test]
    fn test_mask_secret() {
        assert_eq!(mask_secret(""), "(not set)");
        assert_eq!(mask_secret("short"), "***");
        assert_eq!(mask_secret("sk-ant-test-123456"), "sk-ant***");
    }

    #[test]
    fn test_roundtrip_set_get() {
        let mut config = Config::default();
        config_set(&mut config, "agent.model", "test-model").unwrap();
        let result = config_get(&config, "agent.model").unwrap();
        assert_eq!(result, "test-model");
    }

    #[test]
    fn test_config_set_vision_model() {
        let mut config = Config::default();
        config_set(&mut config, "agent.vision_model", "ollama/llava").unwrap();
        assert_eq!(config.agent.vision_model, "ollama/llava");
    }

    #[test]
    fn test_config_set_browser() {
        let mut config = Config::default();
        config_set(&mut config, "browser.enabled", "true").unwrap();
        assert!(config.browser.enabled);

        config_set(&mut config, "browser.headless", "false").unwrap();
        assert!(!config.browser.headless);

        config_set(&mut config, "browser.browser_type", "firefox").unwrap();
        assert_eq!(config.browser.browser_type, "firefox");

        config_set(&mut config, "browser.executable_path", "/usr/bin/chromium").unwrap();
        assert_eq!(config.browser.executable_path, "/usr/bin/chromium");
    }

    #[test]
    fn test_config_set_ui_theme() {
        let mut config = Config::default();
        config_set(&mut config, "ui.theme", "dark").unwrap();
        assert_eq!(config.ui.theme, "dark");

        config_set(&mut config, "ui.theme", "light").unwrap();
        assert_eq!(config.ui.theme, "light");
    }

    #[test]
    fn test_config_set_value_array() {
        let mut config = Config::default();
        let models = serde_json::json!(["openai/gpt-4o", "ollama/llama3"]);
        config_set_value(&mut config, "agent.fallback_models", models).unwrap();
        assert_eq!(
            config.agent.fallback_models,
            vec!["openai/gpt-4o", "ollama/llama3"]
        );
    }

    #[test]
    fn test_config_set_value_empty_array() {
        let mut config = Config::default();
        config.agent.fallback_models = vec!["old-model".to_string()];
        config_set_value(&mut config, "agent.fallback_models", serde_json::json!([])).unwrap();
        assert!(config.agent.fallback_models.is_empty());
    }
}
