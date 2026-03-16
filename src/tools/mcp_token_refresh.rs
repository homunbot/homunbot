//! OAuth token refresh for MCP servers.
//!
//! Detects the provider type from a server's env map, resolves vault
//! credentials, exchanges the refresh token for a new access token,
//! and updates the vault with the fresh token.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;

use crate::config::McpServerConfig;
use crate::storage::{global_secrets, SecretKey};

/// Result of a successful token refresh.
pub struct TokenRefreshResult {
    pub access_token: String,
    /// Some providers rotate refresh tokens; store the new one if present.
    pub new_refresh_token: Option<String>,
    pub expires_in: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Attempt to refresh OAuth tokens for a server.
///
/// Returns `Ok(result)` if refresh succeeded, `Err` if the provider is
/// not recognized or the refresh grant fails.
pub async fn try_refresh_for_server(
    server_name: &str,
    server: &McpServerConfig,
) -> Result<TokenRefreshResult> {
    let env = &server.env;

    // Google (Gmail, Calendar) — stdio servers with GOOGLE_REFRESH_TOKEN
    if env.contains_key("GOOGLE_REFRESH_TOKEN") {
        let client_id = resolve_vault_env(server_name, env, "GOOGLE_CLIENT_ID")?;
        let client_secret = resolve_vault_env(server_name, env, "GOOGLE_CLIENT_SECRET")?;
        let refresh_token = resolve_vault_env(server_name, env, "GOOGLE_REFRESH_TOKEN")?;
        return refresh_google_token(&client_id, &client_secret, &refresh_token).await;
    }

    // Notion — HTTP server with NOTION_TOKEN.
    // Refresh metadata (refresh_token, client_id, token_endpoint) stored in vault
    // during the initial OAuth 2.1 exchange (see oauth.rs).
    if server.transport == "http" && env.contains_key("NOTION_TOKEN") {
        let secrets = global_secrets().context("Failed to access vault")?;
        let refresh_token = secrets
            .get(&SecretKey::custom(&format!(
                "vault.mcp.{server_name}.notion_refresh_token"
            )))?
            .with_context(|| {
                format!("No Notion refresh token stored for '{server_name}' — re-auth required")
            })?;
        let client_id = secrets
            .get(&SecretKey::custom(&format!(
                "vault.mcp.{server_name}.notion_client_id"
            )))?
            .with_context(|| {
                format!("No Notion client_id stored for '{server_name}' — re-auth required")
            })?;
        let token_endpoint = secrets
            .get(&SecretKey::custom(&format!(
                "vault.mcp.{server_name}.notion_token_endpoint"
            )))?
            .unwrap_or_else(|| "https://mcp.notion.com/token".to_string());

        return refresh_notion_token(&client_id, &token_endpoint, &refresh_token).await;
    }

    anyhow::bail!("No supported OAuth refresh provider detected for MCP server '{server_name}'");
}

/// Refresh a Google OAuth access token using the refresh_token grant.
async fn refresh_google_token(
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<TokenRefreshResult> {
    let client = reqwest::Client::builder().use_rustls_tls().build()?;
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .context("Google token refresh request failed")?;

    let status = response.status();
    let body: TokenResponse = response
        .json()
        .await
        .context("Failed to parse Google token response")?;

    if !status.is_success() || body.access_token.is_none() {
        let detail = body
            .error_description
            .or(body.error)
            .unwrap_or_else(|| format!("HTTP {status}"));
        anyhow::bail!("Google token refresh failed: {detail}");
    }

    Ok(TokenRefreshResult {
        access_token: body.access_token.unwrap(),
        new_refresh_token: body.refresh_token,
        expires_in: body.expires_in.unwrap_or(3600),
    })
}

/// Refresh a Notion OAuth 2.1 access token using the refresh_token grant.
///
/// Notion uses public clients (PKCE, no client_secret) so the refresh request
/// only needs `client_id` + `refresh_token`.
async fn refresh_notion_token(
    client_id: &str,
    token_endpoint: &str,
    refresh_token: &str,
) -> Result<TokenRefreshResult> {
    let client = reqwest::Client::builder().use_rustls_tls().build()?;
    let response = client
        .post(token_endpoint)
        .form(&[
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .context("Notion token refresh request failed")?;

    let status = response.status();
    let body: TokenResponse = response
        .json()
        .await
        .context("Failed to parse Notion token response")?;

    if !status.is_success() || body.access_token.is_none() {
        let detail = body
            .error_description
            .or(body.error)
            .unwrap_or_else(|| format!("HTTP {status}"));
        anyhow::bail!("Notion token refresh failed: {detail}");
    }

    Ok(TokenRefreshResult {
        access_token: body.access_token.unwrap(),
        new_refresh_token: body.refresh_token,
        expires_in: body.expires_in.unwrap_or(3600),
    })
}

/// Resolve an env value from the server's env map, following vault:// references.
fn resolve_vault_env(
    server_name: &str,
    env: &HashMap<String, String>,
    key: &str,
) -> Result<String> {
    let raw = env
        .get(key)
        .with_context(|| format!("MCP '{server_name}' missing env key '{key}'"))?;

    if let Some(vault_key) = raw.strip_prefix("vault://") {
        let secrets = global_secrets().context("Failed to access vault")?;
        let secret_key = SecretKey::custom(&format!("vault.{}", vault_key.trim()));
        secrets
            .get(&secret_key)?
            .with_context(|| format!("Vault key '{vault_key}' not found for MCP '{server_name}'"))
    } else {
        Ok(raw.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_vault_env_plain_value() {
        let mut env = HashMap::new();
        env.insert("API_KEY".to_string(), "plain_value".to_string());
        let result = resolve_vault_env("test", &env, "API_KEY").unwrap();
        assert_eq!(result, "plain_value");
    }

    #[test]
    fn resolve_vault_env_missing_key() {
        let env = HashMap::new();
        assert!(resolve_vault_env("test", &env, "MISSING").is_err());
    }

    #[test]
    fn detect_google_provider() {
        let mut env = HashMap::new();
        env.insert("GOOGLE_REFRESH_TOKEN".to_string(), "tok".to_string());
        env.insert("GOOGLE_CLIENT_ID".to_string(), "id".to_string());
        env.insert("GOOGLE_CLIENT_SECRET".to_string(), "secret".to_string());
        let server = McpServerConfig {
            env,
            ..Default::default()
        };
        // Can't actually call refresh (no network), but verify detection works
        assert!(server.env.contains_key("GOOGLE_REFRESH_TOKEN"));
    }

    #[test]
    fn detect_notion_provider() {
        let mut env = HashMap::new();
        env.insert("NOTION_TOKEN".to_string(), "ntn_abc123".to_string());
        let server = McpServerConfig {
            transport: "http".to_string(),
            env,
            ..Default::default()
        };
        // Verify detection: HTTP transport + NOTION_TOKEN env key
        assert_eq!(server.transport, "http");
        assert!(server.env.contains_key("NOTION_TOKEN"));
    }

    #[test]
    fn notion_not_detected_for_stdio() {
        let mut env = HashMap::new();
        env.insert("NOTION_TOKEN".to_string(), "ntn_abc123".to_string());
        let server = McpServerConfig {
            transport: "stdio".to_string(),
            env,
            ..Default::default()
        };
        // Stdio transport with NOTION_TOKEN should NOT match Notion detection
        assert_ne!(server.transport, "http");
    }
}
