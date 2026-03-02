//! Secure secrets storage using AES-256-GCM encryption.
//!
//! Architecture:
//! - Secrets are stored in `~/.homun/secrets.enc` (encrypted JSON)
//! - Master key storage (in priority order):
//!   1. OS keychain (macOS Keychain / Linux Secret Service / Windows Credential Manager)
//!   2. File-based fallback (`~/.homun/master.key`, permissions 0600)
//! - Algorithm: AES-256-GCM with random nonce per encryption
//!
//! Security properties:
//! - Each encryption uses a fresh random nonce
//! - Authentication tag prevents tampering
//! - Memory is zeroed after use (zeroize)
//! - File-based fallback uses restrictive permissions (owner-only)

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ring::aead::{Aad, BoundKey, Nonce, NonceSequence, OpeningKey, SealingKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroizing;

/// Service name for keychain entries
const KEYCHAIN_SERVICE: &str = "dev.homun.secrets";
/// Key identifier for the master encryption key
const MASTER_KEY_ID: &str = "master";

/// Errors specific to secrets management
#[derive(Error, Debug)]
pub enum SecretsError {
    #[error("Failed to access OS keychain: {0}")]
    KeychainError(String),

    #[error("Failed to encrypt secrets: {0}")]
    EncryptionError(String),

    #[error("Failed to decrypt secrets: {0}")]
    DecryptionError(String),

    #[error("Secret not found: {0}")]
    NotFound(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Key identifier for different secret types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretKey(String);

impl SecretKey {
    pub fn provider_api_key(provider: &str) -> Self {
        Self(format!("provider.{}.api_key", provider))
    }

    pub fn channel_token(channel: &str) -> Self {
        Self(format!("channel.{}.token", channel))
    }

    pub fn custom(key: &str) -> Self {
        Self(key.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Encrypted secrets file format
#[derive(Serialize, Deserialize)]
struct SecretsFile {
    version: u32,
    nonce: String,      // Base64-encoded nonce
    ciphertext: String, // Base64-encoded ciphertext + tag
}

/// In-memory secrets container (decrypted)
#[derive(Serialize, Deserialize, Default)]
struct SecretsData {
    secrets: HashMap<String, String>,
}

/// Nonce sequence for ring AEAD
struct SingleNonce(Option<Nonce>);

impl SingleNonce {
    fn new(nonce: [u8; 12]) -> Self {
        let mut n = [0u8; 12];
        n.copy_from_slice(&nonce);
        Self(Some(Nonce::assume_unique_for_key(n)))
    }
}

impl NonceSequence for SingleNonce {
    fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
        self.0.take().ok_or(ring::error::Unspecified)
    }
}

/// Where the master key is stored
#[derive(Debug)]
enum KeyBackend {
    /// OS keychain (macOS Keychain, Linux Secret Service, Windows Credential Manager)
    Keychain(keyring::Entry),
    /// File-based fallback for headless/server environments (~/.homun/master.key)
    File(PathBuf),
}

/// Secure secrets storage with AES-256-GCM encryption
pub struct EncryptedSecrets {
    path: PathBuf,
    rng: SystemRandom,
    backend: KeyBackend,
}

impl EncryptedSecrets {
    /// Create a new secrets storage at the default location.
    ///
    /// Tries OS keychain first; falls back to file-based key storage
    /// on headless systems (servers, Docker, WSL without GUI).
    pub fn new() -> Result<Self> {
        let path = Self::default_path()?;
        let rng = SystemRandom::new();

        // Try OS keychain first
        let backend = match Self::try_keychain() {
            Ok(entry) => {
                tracing::debug!("Using OS keychain for master key");
                KeyBackend::Keychain(entry)
            }
            Err(e) => {
                let key_path = crate::config::Config::data_dir().join("master.key");
                tracing::info!(
                    reason = %e,
                    fallback = %key_path.display(),
                    "OS keychain unavailable, using file-based key storage"
                );
                KeyBackend::File(key_path)
            }
        };

        let storage = Self { path, rng, backend };
        storage.ensure_master_key()?;

        Ok(storage)
    }

    /// Attempt to create a keychain entry. Fails on headless systems.
    fn try_keychain() -> Result<keyring::Entry, String> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, MASTER_KEY_ID)
            .map_err(|e| format!("keyring init: {e}"))?;

        // Probe: try to read — NoEntry is fine (we'll create it), other errors = unsupported
        match entry.get_password() {
            Ok(_) => Ok(entry),
            Err(keyring::Error::NoEntry) => Ok(entry),
            Err(e) => Err(format!("keyring probe: {e}")),
        }
    }

    /// Get the default path for secrets file (~/.homun/secrets.enc)
    fn default_path() -> Result<PathBuf> {
        let data_dir = crate::config::Config::data_dir();
        let new_path = data_dir.join("secrets.enc");

        // Migrate from legacy location (~/Library/Application Support/homun/)
        if !new_path.exists() {
            let legacy = dirs::data_local_dir()
                .or_else(dirs::data_dir)
                .map(|d| d.join("homun").join("secrets.enc"));

            if let Some(old_path) = legacy {
                if old_path.exists() {
                    tracing::info!(
                        from = %old_path.display(),
                        to = %new_path.display(),
                        "Migrating secrets.enc to new location"
                    );
                    if let Err(e) = std::fs::rename(&old_path, &new_path) {
                        tracing::warn!("Failed to migrate secrets.enc, copying instead: {e}");
                        let _ = std::fs::copy(&old_path, &new_path);
                    }
                }
            }
        }

        Ok(new_path)
    }

    /// Ensure a master key exists (keychain or file)
    fn ensure_master_key(&self) -> Result<()> {
        match &self.backend {
            KeyBackend::Keychain(entry) => match entry.get_password() {
                Ok(_) => {
                    tracing::debug!("Master key already exists in keychain");
                }
                Err(keyring::Error::NoEntry) => {
                    tracing::info!("Generating new master key for secrets encryption");
                    let key = self.generate_key()?;
                    entry
                        .set_password(&key)
                        .map_err(|e| SecretsError::KeychainError(e.to_string()))?;
                    tracing::info!("Master key stored in OS keychain");
                }
                Err(e) => return Err(SecretsError::KeychainError(e.to_string()).into()),
            },
            KeyBackend::File(path) => {
                if !path.exists() {
                    tracing::info!("Generating new master key (file-based)");
                    let key = self.generate_key()?;

                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(path, &key)?;

                    // Set restrictive permissions (owner read/write only)
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
                    }
                    tracing::info!(path = %path.display(), "Master key stored in file (0600)");
                } else {
                    tracing::debug!(path = %path.display(), "Master key file already exists");
                }
            }
        }
        Ok(())
    }

    /// Generate a new random key (Base64-encoded 32 bytes)
    fn generate_key(&self) -> Result<String> {
        let mut key_bytes = [0u8; 32];
        self.rng
            .fill(&mut key_bytes)
            .map_err(|e| SecretsError::EncryptionError(e.to_string()))?;
        Ok(BASE64.encode(key_bytes))
    }

    /// Get the master key from keychain or file
    fn get_master_key(&self) -> Result<Zeroizing<[u8; 32]>> {
        let key_b64 = match &self.backend {
            KeyBackend::Keychain(entry) => entry
                .get_password()
                .map_err(|e| SecretsError::KeychainError(e.to_string()))?,
            KeyBackend::File(path) => std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read master key from {}", path.display()))?
                .trim()
                .to_string(),
        };

        let mut key_bytes = Zeroizing::new([0u8; 32]);
        BASE64
            .decode_slice(&key_b64, &mut *key_bytes)
            .context("Failed to decode master key")?;

        Ok(key_bytes)
    }

    /// Encrypt and save all secrets to disk
    pub fn save(&self, secrets: &HashMap<String, String>) -> Result<()> {
        let master_key = self.get_master_key()?;

        // Serialize secrets to JSON
        let data = SecretsData {
            secrets: secrets.clone(),
        };
        let plaintext = serde_json::to_vec(&data).context("Failed to serialize secrets")?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        self.rng
            .fill(&mut nonce_bytes)
            .map_err(|e| SecretsError::EncryptionError(e.to_string()))?;

        // Encrypt with AES-256-GCM
        let unbound_key = ring::aead::UnboundKey::new(&AES_256_GCM, &*master_key)
            .map_err(|e| SecretsError::EncryptionError(format!("{:?}", e)))?;
        let mut sealing_key = SealingKey::new(unbound_key, SingleNonce::new(nonce_bytes));

        let mut ciphertext = plaintext;
        sealing_key
            .seal_in_place_append_tag(Aad::empty(), &mut ciphertext)
            .map_err(|e| SecretsError::EncryptionError(format!("{:?}", e)))?;

        // Create file structure
        let file_data = SecretsFile {
            version: 1,
            nonce: BASE64.encode(nonce_bytes),
            ciphertext: BASE64.encode(&ciphertext),
        };

        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write to file with restrictive permissions
        let json = serde_json::to_string_pretty(&file_data)
            .context("Failed to serialize encrypted file")?;

        // Write atomically
        let temp_path = self.path.with_extension("tmp");
        std::fs::write(&temp_path, json)?;

        // Set restrictive permissions (owner read/write only on Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o600))?;
        }

        std::fs::rename(&temp_path, &self.path)?;

        tracing::info!("Saved {} encrypted secrets", secrets.len());
        Ok(())
    }

    /// Load and decrypt all secrets from disk
    pub fn load(&self) -> Result<HashMap<String, String>> {
        if !self.path.exists() {
            tracing::debug!("Secrets file does not exist, returning empty map");
            return Ok(HashMap::new());
        }

        let master_key = self.get_master_key()?;

        // Read encrypted file
        let json = std::fs::read_to_string(&self.path).context("Failed to read secrets file")?;

        let file_data: SecretsFile =
            serde_json::from_str(&json).context("Failed to parse secrets file")?;

        // Decode nonce and ciphertext
        let mut nonce_bytes = [0u8; 12];
        BASE64
            .decode_slice(&file_data.nonce, &mut nonce_bytes)
            .context("Failed to decode nonce")?;

        let mut ciphertext = BASE64
            .decode(&file_data.ciphertext)
            .context("Failed to decode ciphertext")?;

        // Decrypt with AES-256-GCM
        let unbound_key = ring::aead::UnboundKey::new(&AES_256_GCM, &*master_key)
            .map_err(|e| SecretsError::DecryptionError(format!("{:?}", e)))?;
        let mut opening_key = OpeningKey::new(unbound_key, SingleNonce::new(nonce_bytes));

        let plaintext = opening_key
            .open_in_place(Aad::empty(), &mut ciphertext)
            .map_err(|e| SecretsError::DecryptionError(format!("{:?}", e)))?;

        // Deserialize
        let data: SecretsData =
            serde_json::from_slice(plaintext).context("Failed to parse decrypted secrets")?;

        tracing::debug!("Loaded {} encrypted secrets", data.secrets.len());
        Ok(data.secrets)
    }

    /// Get a single secret by key
    pub fn get(&self, key: &SecretKey) -> Result<Option<String>> {
        let secrets = self.load()?;
        Ok(secrets.get(key.as_str()).cloned())
    }

    /// Set a single secret
    pub fn set(&self, key: &SecretKey, value: &str) -> Result<()> {
        let mut secrets = self.load()?;
        secrets.insert(key.as_str().to_string(), value.to_string());
        self.save(&secrets)
    }

    /// Delete a secret
    pub fn delete(&self, key: &SecretKey) -> Result<()> {
        let mut secrets = self.load()?;
        secrets.remove(key.as_str());
        self.save(&secrets)
    }

    /// List all secret keys (returns key strings, not values)
    pub fn list_keys(&self) -> Vec<String> {
        match self.load() {
            Ok(secrets) => secrets.keys().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Check if secrets file exists
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Get the path to the secrets file
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Encrypt arbitrary data using the same master key as secrets.enc.
    /// Returns a JSON string containing nonce + ciphertext (both base64).
    pub fn encrypt_data(&self, plaintext: &[u8]) -> Result<String> {
        let master_key = self.get_master_key()?;

        let mut nonce_bytes = [0u8; 12];
        self.rng
            .fill(&mut nonce_bytes)
            .map_err(|e| SecretsError::EncryptionError(e.to_string()))?;

        let unbound_key = ring::aead::UnboundKey::new(&AES_256_GCM, &*master_key)
            .map_err(|e| SecretsError::EncryptionError(format!("{:?}", e)))?;
        let mut sealing_key = SealingKey::new(unbound_key, SingleNonce::new(nonce_bytes));

        let mut ciphertext = plaintext.to_vec();
        sealing_key
            .seal_in_place_append_tag(Aad::empty(), &mut ciphertext)
            .map_err(|e| SecretsError::EncryptionError(format!("{:?}", e)))?;

        let file_data = SecretsFile {
            version: 1,
            nonce: BASE64.encode(nonce_bytes),
            ciphertext: BASE64.encode(&ciphertext),
        };

        serde_json::to_string(&file_data).context("Failed to serialize encrypted data")
    }

    /// Decrypt data previously encrypted with `encrypt_data()`.
    pub fn decrypt_data(&self, encrypted_json: &str) -> Result<Vec<u8>> {
        let master_key = self.get_master_key()?;

        let file_data: SecretsFile =
            serde_json::from_str(encrypted_json).context("Failed to parse encrypted data")?;

        let mut nonce_bytes = [0u8; 12];
        BASE64
            .decode_slice(&file_data.nonce, &mut nonce_bytes)
            .context("Failed to decode nonce")?;

        let mut ciphertext = BASE64
            .decode(&file_data.ciphertext)
            .context("Failed to decode ciphertext")?;

        let unbound_key = ring::aead::UnboundKey::new(&AES_256_GCM, &*master_key)
            .map_err(|e| SecretsError::DecryptionError(format!("{:?}", e)))?;
        let mut opening_key = OpeningKey::new(unbound_key, SingleNonce::new(nonce_bytes));

        let plaintext = opening_key
            .open_in_place(Aad::empty(), &mut ciphertext)
            .map_err(|e| SecretsError::DecryptionError(format!("{:?}", e)))?;

        Ok(plaintext.to_vec())
    }
}

/// Global secrets instance (lazy-initialized)
use std::sync::Mutex;

static SECRETS: Mutex<Option<Arc<EncryptedSecrets>>> = Mutex::new(None);

/// Get the global secrets instance
pub fn global_secrets() -> Result<Arc<EncryptedSecrets>> {
    let mut guard = SECRETS.lock().unwrap();
    if guard.is_none() {
        *guard = Some(Arc::new(EncryptedSecrets::new()?));
    }
    Ok(guard.as_ref().unwrap().clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    impl EncryptedSecrets {
        /// Create a test instance using file-based key (no keychain pollution)
        fn test_new(path: PathBuf) -> Result<Self> {
            let rng = SystemRandom::new();
            let key_path = path.with_extension("key");
            let backend = KeyBackend::File(key_path);

            let storage = Self { path, rng, backend };
            storage.ensure_master_key()?;
            Ok(storage)
        }
    }

    #[test]
    fn test_secret_keys() {
        let key = SecretKey::provider_api_key("openai");
        assert_eq!(key.as_str(), "provider.openai.api_key");

        let key = SecretKey::channel_token("telegram");
        assert_eq!(key.as_str(), "channel.telegram.token");
    }

    #[test]
    fn test_encrypt_decrypt() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("secrets.enc");

        let storage = EncryptedSecrets::test_new(path).unwrap();

        let mut secrets = HashMap::new();
        secrets.insert("test.key".to_string(), "secret_value".to_string());

        storage.save(&secrets).unwrap();
        let loaded = storage.load().unwrap();

        assert_eq!(loaded.get("test.key"), Some(&"secret_value".to_string()));
    }

    #[test]
    fn test_encrypt_decrypt_data() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("secrets.enc");
        let storage = EncryptedSecrets::test_new(path).unwrap();

        let plaintext = b"sensitive 2FA config data";
        let encrypted = storage.encrypt_data(plaintext).unwrap();

        // Encrypted output should be valid JSON and different from plaintext
        assert!(encrypted.contains("nonce"));
        assert!(encrypted.contains("ciphertext"));
        assert!(!encrypted.contains("sensitive"));

        let decrypted = storage.decrypt_data(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_data_roundtrip_json() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("secrets.enc");
        let storage = EncryptedSecrets::test_new(path).unwrap();

        // Simulate 2FA config JSON
        let json = r#"{"version":1,"enabled":true,"totp_secret":"JBSWY3DPEHPK3PXP"}"#;
        let encrypted = storage.encrypt_data(json.as_bytes()).unwrap();
        let decrypted = storage.decrypt_data(&encrypted).unwrap();

        assert_eq!(String::from_utf8(decrypted).unwrap(), json);
    }

    #[test]
    fn test_set_get_delete() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("secrets.enc");

        let storage = EncryptedSecrets::test_new(path).unwrap();

        let key = SecretKey::provider_api_key("test_provider");

        // Set
        storage.set(&key, "sk-test-123").unwrap();

        // Get
        let value = storage.get(&key).unwrap();
        assert_eq!(value, Some("sk-test-123".to_string()));

        // Delete
        storage.delete(&key).unwrap();

        // Get again (should be None)
        let value = storage.get(&key).unwrap();
        assert!(value.is_none());
    }
}
