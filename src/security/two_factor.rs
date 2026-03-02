//! Two-factor authentication configuration and session management.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use zeroize::Zeroizing;

use super::totp::{generate_recovery_codes, TotpManager};

/// Maximum failed attempts before lockout
const MAX_FAILED_ATTEMPTS: u32 = 5;
/// Lockout duration in seconds
const LOCKOUT_DURATION_SECS: u64 = 300; // 5 minutes
/// Default session timeout in seconds
const DEFAULT_SESSION_TIMEOUT_SECS: u64 = 300; // 5 minutes

/// 2FA configuration stored in 2fa.enc
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwoFactorConfig {
    /// Version for future migrations
    pub version: u32,
    /// Whether 2FA is enabled
    pub enabled: bool,
    /// TOTP secret (Base32 encoded)
    pub totp_secret: String,
    /// Account identifier (e.g., "user@hostname")
    pub account: String,
    /// Recovery codes (format: XXXX-XXXX)
    pub recovery_codes: Vec<String>,
    /// When 2FA was enabled
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Failed attempt counter
    pub failed_attempts: u32,
    /// Lockout until this time (if any)
    #[serde(with = "option_chrono")]
    pub locked_until: Option<chrono::DateTime<chrono::Utc>>,
    /// Session timeout in seconds
    pub session_timeout_secs: u64,
}

/// Helper for serializing Option<DateTime>
mod option_chrono {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(value: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(dt) => dt.serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::deserialize(deserializer)
    }
}

impl Default for TwoFactorConfig {
    fn default() -> Self {
        Self {
            version: 1,
            enabled: false,
            totp_secret: String::new(),
            account: String::new(),
            recovery_codes: Vec::new(),
            created_at: chrono::Utc::now(),
            failed_attempts: 0,
            locked_until: None,
            session_timeout_secs: DEFAULT_SESSION_TIMEOUT_SECS,
        }
    }
}

impl TwoFactorConfig {
    /// Create a new 2FA config with generated secret and recovery codes.
    pub fn new(account: &str, session_timeout_secs: Option<u64>) -> Self {
        Self {
            version: 1,
            enabled: true,
            totp_secret: TotpManager::generate_secret(),
            account: account.to_string(),
            recovery_codes: generate_recovery_codes(),
            created_at: chrono::Utc::now(),
            failed_attempts: 0,
            locked_until: None,
            session_timeout_secs: session_timeout_secs.unwrap_or(DEFAULT_SESSION_TIMEOUT_SECS),
        }
    }

    /// Check if currently locked out
    pub fn is_locked_out(&self) -> bool {
        if let Some(locked_until) = self.locked_until {
            locked_until > chrono::Utc::now()
        } else {
            false
        }
    }

    /// Record a failed attempt and potentially lock out
    pub fn record_failed_attempt(&mut self) {
        self.failed_attempts += 1;
        if self.failed_attempts >= MAX_FAILED_ATTEMPTS {
            self.locked_until =
                Some(chrono::Utc::now() + chrono::Duration::seconds(LOCKOUT_DURATION_SECS as i64));
        }
    }

    /// Reset failed attempts on successful authentication
    pub fn reset_failed_attempts(&mut self) {
        self.failed_attempts = 0;
        self.locked_until = None;
    }

    /// Check if a recovery code is valid and remove it if so
    pub fn use_recovery_code(&mut self, code: &str) -> bool {
        let code = code.to_uppercase();
        if let Some(pos) = self.recovery_codes.iter().position(|c| c == &code) {
            self.recovery_codes.remove(pos);
            true
        } else {
            false
        }
    }
}

/// An authenticated 2FA session
#[derive(Debug, Clone)]
pub struct TwoFactorSession {
    /// When the session was created
    pub authenticated_at: Instant,
    /// How long the session is valid
    pub ttl: Duration,
}

impl TwoFactorSession {
    /// Create a new session with the given TTL
    pub fn new(ttl: Duration) -> Self {
        Self {
            authenticated_at: Instant::now(),
            ttl,
        }
    }

    /// Check if the session is still valid
    pub fn is_valid(&self) -> bool {
        self.authenticated_at.elapsed() < self.ttl
    }
}

/// In-memory session manager for 2FA sessions
pub struct TwoFactorSessionManager {
    /// Active sessions by session ID (random UUID)
    sessions: RwLock<HashMap<String, TwoFactorSession>>,
    /// Session timeout (from config)
    default_ttl: Duration,
}

impl TwoFactorSessionManager {
    /// Create a new session manager
    pub fn new(default_timeout_secs: u64) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            default_ttl: Duration::from_secs(default_timeout_secs),
        }
    }

    /// Create a new authenticated session
    /// Returns the session ID to be used for subsequent requests
    pub async fn create_session(&self) -> String {
        let session_id = uuid::Uuid::new_v4().to_string();
        let session = TwoFactorSession::new(self.default_ttl);

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id.clone(), session);

        tracing::debug!(session_id = %session_id, "Created 2FA session");
        session_id
    }

    /// Verify a session is valid
    pub async fn verify_session(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get(session_id) {
            if session.is_valid() {
                return true;
            } else {
                // Remove expired session
                sessions.remove(session_id);
            }
        }
        false
    }

    /// Invalidate a session (logout)
    pub async fn invalidate_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
    }

    /// Clean up expired sessions
    pub async fn cleanup_expired(&self) {
        let mut sessions = self.sessions.write().await;
        let before = sessions.len();
        sessions.retain(|_, session| session.is_valid());
        let removed = before - sessions.len();
        if removed > 0 {
            tracing::debug!(removed = removed, "Cleaned up expired 2FA sessions");
        }
    }
}

/// Storage for 2FA configuration (2fa.enc)
pub struct TwoFactorStorage {
    path: PathBuf,
}

impl TwoFactorStorage {
    /// Create a new storage instance
    pub fn new() -> Result<Self> {
        let path = Self::default_path();
        Ok(Self { path })
    }

    /// Get the default path for 2fa.enc
    fn default_path() -> PathBuf {
        crate::config::Config::data_dir().join("2fa.enc")
    }

    /// Check if 2FA config exists
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Load the 2FA configuration.
    /// Returns default config if file doesn't exist.
    /// Supports both encrypted (new) and plaintext JSON (legacy) formats.
    pub fn load(&self) -> Result<TwoFactorConfig> {
        if !self.path.exists() {
            return Ok(TwoFactorConfig::default());
        }

        let raw = std::fs::read_to_string(&self.path)
            .with_context(|| format!("Failed to read 2fa config from {}", self.path.display()))?;

        // Try encrypted format first (new)
        if let Ok(secrets) = crate::storage::global_secrets() {
            if let Ok(plaintext) = secrets.decrypt_data(&raw) {
                let config: TwoFactorConfig = serde_json::from_slice(&plaintext)
                    .context("Failed to parse decrypted 2fa config")?;
                return Ok(config);
            }
        }

        // Fallback: try legacy plaintext JSON for migration
        let config: TwoFactorConfig =
            serde_json::from_str(&raw).context("Failed to parse 2fa config")?;

        tracing::warn!(
            "Loaded 2FA config from legacy plaintext format, will re-encrypt on next save"
        );
        Ok(config)
    }

    /// Save the 2FA configuration encrypted with AES-256-GCM (same master key as secrets.enc).
    pub fn save(&self, config: &TwoFactorConfig) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        let json =
            serde_json::to_string_pretty(config).context("Failed to serialize 2fa config")?;

        // Encrypt with vault master key
        let encrypted = crate::storage::global_secrets()
            .context("Failed to access vault for 2FA encryption")?
            .encrypt_data(json.as_bytes())
            .context("Failed to encrypt 2fa config")?;

        // Write atomically
        let temp_path = self.path.with_extension("tmp");
        std::fs::write(&temp_path, &encrypted)
            .with_context(|| format!("Failed to write 2fa config to {}", temp_path.display()))?;

        // Set restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o600))?;
        }

        std::fs::rename(&temp_path, &self.path)
            .with_context(|| format!("Failed to save 2fa config to {}", self.path.display()))?;

        tracing::info!("Saved 2FA configuration (encrypted)");
        Ok(())
    }

    /// Delete the 2FA configuration
    pub fn delete(&self) -> Result<()> {
        if self.path.exists() {
            std::fs::remove_file(&self.path).with_context(|| {
                format!("Failed to delete 2fa config at {}", self.path.display())
            })?;
        }
        Ok(())
    }
}

/// Global 2FA session manager (lazy-initialized)
static SESSION_MANAGER: std::sync::OnceLock<Arc<TwoFactorSessionManager>> =
    std::sync::OnceLock::new();

/// Get the global session manager
pub fn global_session_manager() -> Arc<TwoFactorSessionManager> {
    SESSION_MANAGER
        .get_or_init(|| {
            // Try to load config to get timeout, otherwise use default
            let timeout = if let Ok(storage) = TwoFactorStorage::new() {
                if let Ok(config) = storage.load() {
                    config.session_timeout_secs
                } else {
                    DEFAULT_SESSION_TIMEOUT_SECS
                }
            } else {
                DEFAULT_SESSION_TIMEOUT_SECS
            };
            Arc::new(TwoFactorSessionManager::new(timeout))
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_two_factor_config_default() {
        let config = TwoFactorConfig::default();
        assert!(!config.enabled);
        assert!(config.totp_secret.is_empty());
    }

    #[test]
    fn test_two_factor_config_new() {
        let config = TwoFactorConfig::new("test@example.com", None);
        assert!(config.enabled);
        assert!(!config.totp_secret.is_empty());
        assert_eq!(config.recovery_codes.len(), 10);
        assert_eq!(config.session_timeout_secs, 300);
    }

    #[test]
    fn test_lockout() {
        let mut config = TwoFactorConfig::default();

        // Should not be locked initially
        assert!(!config.is_locked_out());

        // Record failed attempts up to limit
        for _ in 0..MAX_FAILED_ATTEMPTS {
            config.record_failed_attempt();
        }

        // Should be locked now
        assert!(config.is_locked_out());

        // Reset should clear lockout
        config.reset_failed_attempts();
        assert!(!config.is_locked_out());
        assert_eq!(config.failed_attempts, 0);
    }

    #[test]
    fn test_recovery_code() {
        let mut config = TwoFactorConfig::new("test@example.com", None);
        let code = config.recovery_codes[0].clone();

        // Should accept valid code
        assert!(config.use_recovery_code(&code));

        // Should have removed the code
        assert!(!config.recovery_codes.contains(&code));

        // Should reject already-used code
        assert!(!config.use_recovery_code(&code));

        // Should have 9 codes left
        assert_eq!(config.recovery_codes.len(), 9);
    }

    #[test]
    fn test_session_validity() {
        let session = TwoFactorSession::new(Duration::from_secs(1));
        assert!(session.is_valid());

        // Wait for expiry
        std::thread::sleep(Duration::from_millis(1100));
        assert!(!session.is_valid());
    }

    #[tokio::test]
    async fn test_session_manager() {
        let manager = TwoFactorSessionManager::new(60);
        let session_id = manager.create_session().await;

        // Should be valid immediately
        assert!(manager.verify_session(&session_id).await);

        // Should still be valid on second check
        assert!(manager.verify_session(&session_id).await);

        // Invalidate
        manager.invalidate_session(&session_id).await;
        assert!(!manager.verify_session(&session_id).await);
    }
}
