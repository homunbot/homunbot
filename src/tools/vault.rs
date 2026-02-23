//! Vault tool — encrypted secret storage accessible by the LLM.
//!
//! Provides 4 actions: store, retrieve, list, delete.
//! Secrets are encrypted with AES-256-GCM and stored in the OS keychain-backed vault.
//! In memory/context, only `vault://key_name` references appear — never plaintext values.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::storage::{global_secrets, SecretKey};

use super::registry::{Tool, ToolContext, ToolResult, get_string_param, get_optional_string};

/// Vault prefix for user secrets (namespaced away from provider/channel keys)
const VAULT_PREFIX: &str = "vault.";

/// Encrypted secret vault — LLM-accessible tool for storing sensitive data.
///
/// When the user mentions passwords, tokens, or other secrets in conversation,
/// the consolidation system (or the LLM directly) can store them here.
/// Memory files only ever contain `vault://key_name` references.
pub struct VaultTool;

impl VaultTool {
    pub fn new() -> Self {
        Self
    }

    fn vault_key(name: &str) -> SecretKey {
        SecretKey::custom(&format!("{VAULT_PREFIX}{name}"))
    }
}

#[async_trait]
impl Tool for VaultTool {
    fn name(&self) -> &str {
        "vault"
    }

    fn description(&self) -> &str {
        "Encrypted secret vault. Store and retrieve sensitive data (passwords, tokens, API keys). \
         Data is encrypted with AES-256-GCM and protected by the OS keychain. \
         In memory and context, only vault://key_name references appear — never plaintext values. \
         Actions: store (save a secret), retrieve (get a secret), list (show stored keys), delete (remove a secret)."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["store", "retrieve", "list", "delete"],
                    "description": "The vault action to perform"
                },
                "key": {
                    "type": "string",
                    "description": "The secret name/key (e.g., 'aws_password', 'github_token'). Required for store, retrieve, delete."
                },
                "value": {
                    "type": "string",
                    "description": "The secret value to store. Required for store action only. NEVER include this value in memory or conversation summaries."
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let action = get_string_param(&args, "action")?;

        match action.as_str() {
            "store" => {
                let key = get_string_param(&args, "key")?;
                let value = get_string_param(&args, "value")?;

                let secrets = global_secrets()
                    .map_err(|e| anyhow::anyhow!("Failed to access vault: {e}"))?;
                secrets.set(&Self::vault_key(&key), &value)?;

                tracing::info!(key = %key, "Stored secret in vault");
                Ok(ToolResult::success(format!(
                    "Secret '{key}' stored securely in encrypted vault. \
                     Reference it as vault://{key} — NEVER include the actual value in memory or responses."
                )))
            }
            "retrieve" => {
                let key = get_string_param(&args, "key")?;

                let secrets = global_secrets()
                    .map_err(|e| anyhow::anyhow!("Failed to access vault: {e}"))?;

                match secrets.get(&Self::vault_key(&key))? {
                    Some(value) => {
                        tracing::info!(key = %key, "Retrieved secret from vault");
                        Ok(ToolResult::success(format!(
                            "vault://{key} = {value}\n\n\
                             ⚠️ This is sensitive data. Show it to the user if they asked, \
                             but NEVER store this value in memory, history, or conversation summaries."
                        )))
                    }
                    None => Ok(ToolResult::error(format!(
                        "Secret '{key}' not found in vault."
                    ))),
                }
            }
            "list" => {
                let secrets = global_secrets()
                    .map_err(|e| anyhow::anyhow!("Failed to access vault: {e}"))?;

                let all = secrets.load()?;
                let vault_keys: Vec<&str> = all
                    .keys()
                    .filter_map(|k| k.strip_prefix(VAULT_PREFIX))
                    .collect();

                if vault_keys.is_empty() {
                    Ok(ToolResult::success("Vault is empty. No secrets stored."))
                } else {
                    let list = vault_keys
                        .iter()
                        .map(|k| format!("- vault://{k}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(ToolResult::success(format!(
                        "Stored secrets ({} total):\n{list}",
                        vault_keys.len()
                    )))
                }
            }
            "delete" => {
                let key = get_string_param(&args, "key")?;

                let secrets = global_secrets()
                    .map_err(|e| anyhow::anyhow!("Failed to access vault: {e}"))?;

                // Check if it exists first
                if secrets.get(&Self::vault_key(&key))?.is_none() {
                    return Ok(ToolResult::error(format!(
                        "Secret '{key}' not found in vault."
                    )));
                }

                secrets.delete(&Self::vault_key(&key))?;
                tracing::info!(key = %key, "Deleted secret from vault");
                Ok(ToolResult::success(format!(
                    "Secret '{key}' deleted from vault."
                )))
            }
            other => Ok(ToolResult::error(format!(
                "Unknown vault action: '{other}'. Valid actions: store, retrieve, list, delete"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_key_namespacing() {
        let key = VaultTool::vault_key("aws_password");
        assert_eq!(key.as_str(), "vault.aws_password");
    }

    #[test]
    fn test_vault_tool_metadata() {
        let tool = VaultTool::new();
        assert_eq!(tool.name(), "vault");
        assert!(tool.description().contains("AES-256-GCM"));

        let params = tool.parameters();
        assert!(params["properties"]["action"].is_object());
        assert!(params["properties"]["key"].is_object());
        assert!(params["properties"]["value"].is_object());
    }
}
