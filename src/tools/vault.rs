//! Vault tool — encrypted secret storage accessible by the LLM.
//!
//! Provides 5 actions: store, retrieve, list, delete, confirm.
//! Secrets are encrypted with AES-256-GCM and stored in the OS keychain-backed vault.
//! In memory/context, only `vault://key_name` references appear — never plaintext values.
//!
//! # Two-Factor Authentication
//!
//! When 2FA is enabled, the `retrieve` action requires authentication:
//! 1. First call to `retrieve` returns "2FA_REQUIRED"
//! 2. LLM asks user for authenticator code
//! 3. Call `confirm` with the code to get a session_id
//! 4. Call `retrieve` with session_id to get the secret
//!
//! Alternatively, pass `code` directly to `retrieve` for one-shot authentication.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

#[cfg(feature = "vault-2fa")]
use crate::security::{global_session_manager, TotpManager, TwoFactorConfig, TwoFactorStorage};
use crate::storage::{global_secrets, SecretKey};

use super::registry::{get_optional_string, get_string_param, Tool, ToolContext, ToolResult};

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

    /// Check if 2FA is enabled.
    ///
    /// Returns true if:
    /// - 2FA config exists and is enabled
    /// - 2FA config exists but can't be loaded (fail closed for security)
    ///
    /// Returns false only if 2FA config doesn't exist (not configured)
    #[cfg(feature = "vault-2fa")]
    fn is_2fa_enabled() -> bool {
        match TwoFactorStorage::new() {
            Ok(storage) => {
                // If file doesn't exist, 2FA is not configured
                if !storage.exists() {
                    tracing::debug!("2FA config file does not exist, 2FA not configured");
                    return false;
                }

                // If file exists, try to load it
                match storage.load() {
                    Ok(config) => {
                        tracing::debug!(twofa_enabled = config.enabled, "Checked 2FA status");
                        config.enabled
                    }
                    Err(e) => {
                        // File exists but can't be loaded - fail closed for security
                        tracing::error!(error = ?e, "2FA config exists but failed to load, denying access");
                        true
                    }
                }
            }
            Err(e) => {
                // Can't create storage - fail closed for security
                tracing::error!(error = ?e, "Failed to create 2FA storage, denying access");
                true
            }
        }
    }

    /// Check if 2FA is enabled (stub when feature disabled)
    #[cfg(not(feature = "vault-2fa"))]
    fn is_2fa_enabled() -> bool {
        false
    }

    /// Load 2FA config
    #[cfg(feature = "vault-2fa")]
    fn load_2fa_config() -> Result<TwoFactorConfig> {
        let storage = TwoFactorStorage::new()?;
        storage.load()
    }

    /// Verify a TOTP code and optionally create a session
    #[cfg(feature = "vault-2fa")]
    async fn verify_and_create_session(code: &str) -> Result<Result<String, String>> {
        let config = Self::load_2fa_config()?;

        if !config.enabled {
            return Ok(Ok("2fa_disabled".to_string()));
        }

        // Check lockout
        if config.is_locked_out() {
            return Ok(Err(
                "Too many failed attempts. Please wait a few minutes.".to_string()
            ));
        }

        // Verify code
        let manager = TotpManager::new(&config.totp_secret, &config.account)?;
        if manager.verify(code) {
            // Success - create session
            let session_manager = global_session_manager();
            let session_id = session_manager.create_session().await;

            // Reset failed attempts
            let mut config = config;
            config.reset_failed_attempts();
            TwoFactorStorage::new()?.save(&config)?;

            tracing::info!("2FA verification successful, session created");
            Ok(Ok(session_id))
        } else {
            // Failed - record attempt
            let mut config = config;
            config.record_failed_attempt();
            TwoFactorStorage::new()?.save(&config)?;

            tracing::warn!(attempts = config.failed_attempts, "2FA verification failed");
            Ok(Err(format!(
                "Invalid code. {} attempts remaining.",
                MAX_FAILED_ATTEMPTS.saturating_sub(config.failed_attempts)
            )))
        }
    }

    /// Verify a TOTP code (stub when feature disabled)
    #[cfg(not(feature = "vault-2fa"))]
    async fn verify_and_create_session(_code: &str) -> Result<Result<String, String>> {
        Ok(Err("2FA feature not enabled in this build".to_string()))
    }

    /// Verify session (feature-gated)
    #[cfg(feature = "vault-2fa")]
    async fn verify_session(session_id: &str) -> bool {
        let session_manager = global_session_manager();
        session_manager.verify_session(session_id).await
    }

    /// Verify session (stub when feature disabled)
    #[cfg(not(feature = "vault-2fa"))]
    async fn verify_session(_session_id: &str) -> bool {
        true
    }
}

/// Maximum failed attempts before lockout
const MAX_FAILED_ATTEMPTS: u32 = 5;

#[async_trait]
impl Tool for VaultTool {
    fn name(&self) -> &str {
        "vault"
    }

    fn description(&self) -> &str {
        "Encrypted secret vault. Store and retrieve sensitive data (passwords, tokens, API keys). \
         Data is encrypted with AES-256-GCM and protected by the OS keychain. \
         In memory and context, only vault://key_name references appear — never plaintext values. \
         Actions: store (save a secret), retrieve (get a secret), list (show stored keys), delete (remove a secret), confirm (verify 2FA code)."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["store", "retrieve", "list", "delete", "confirm"],
                    "description": "The vault action to perform"
                },
                "key": {
                    "type": "string",
                    "description": "The secret name/key (e.g., 'aws_password', 'github_token'). Required for store, retrieve, delete."
                },
                "value": {
                    "type": "string",
                    "description": "The secret value to store. Required for store action only. NEVER include this value in memory or conversation summaries."
                },
                "code": {
                    "type": "string",
                    "description": "6-digit authenticator code for 2FA. Can be passed to 'confirm' or directly to 'retrieve' for one-shot auth."
                },
                "session_id": {
                    "type": "string",
                    "description": "Session ID from a previous 'confirm' call. Use this to retrieve secrets without re-entering the code."
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
                let session_id = get_optional_string(&args, "session_id");
                let code = get_optional_string(&args, "code");

                tracing::debug!(
                    key = %key,
                    has_session = session_id.is_some(),
                    has_code = code.is_some(),
                    "Vault retrieve request"
                );

                // Check if 2FA is enabled
                if Self::is_2fa_enabled() {
                    // Check if we have a valid session
                    if let Some(sid) = session_id {
                        if !Self::verify_session(&sid).await {
                            return Ok(ToolResult::error(
                                "Session expired or invalid. Please authenticate again with 'confirm' action."
                            ));
                        }
                    } else if let Some(c) = code {
                        // One-shot authentication with code
                        match Self::verify_and_create_session(&c).await? {
                            Ok(_) => { /* Success, proceed */ }
                            Err(e) => return Ok(ToolResult::error(e)),
                        }
                    } else {
                        // No session, no code - require 2FA
                        return Ok(ToolResult::success(
                            "2FA_REQUIRED: Two-factor authentication is enabled. \
                             Please provide your authenticator code using the 'code' parameter, \
                             or first call 'confirm' with the code to get a session_id."
                        ));
                    }
                }

                let secrets = global_secrets()
                    .map_err(|e| anyhow::anyhow!("Failed to access vault: {e}"))?;

                match secrets.get(&Self::vault_key(&key))? {
                    Some(value) => {
                        tracing::info!(key = %key, "Retrieved secret from vault");
                        Ok(ToolResult::success(format!(
                            "**Secret value:**\n```\n{value}\n```\n\n\
                             ⚠️ This is sensitive data. Show it to the user if they asked, \
                             but NEVER store this value in memory, history, or conversation summaries."
                        )))
                    }
                    None => Ok(ToolResult::error(format!(
                        "Secret '{key}' not found in vault."
                    ))),
                }
            }
            "confirm" => {
                let code = get_string_param(&args, "code")?;

                if !Self::is_2fa_enabled() {
                    return Ok(ToolResult::error(
                        "Two-factor authentication is not enabled. Enable it in Settings first."
                    ));
                }

                match Self::verify_and_create_session(&code).await? {
                    Ok(session_id) => Ok(ToolResult::success(format!(
                        "2FA verified successfully. Session ID: {}\n\
                         Use this session_id with 'retrieve' to access secrets. \
                         Session expires in 5 minutes.",
                        session_id
                    ))),
                    Err(e) => Ok(ToolResult::error(e)),
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
                "Unknown vault action: '{other}'. Valid actions: store, retrieve, list, delete, confirm"
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
        assert!(params["properties"]["code"].is_object());
        assert!(params["properties"]["session_id"].is_object());
    }
}
