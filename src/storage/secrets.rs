//! Secure secrets storage using AES-256-GCM encryption with OS keychain-backed keys.
//!
//! Architecture:
//! - Secrets are stored in `~/.homun/secrets.enc` (encrypted JSON)
//! - Encryption key is stored in OS keychain (macOS Keychain / Linux Secret Service)
//! - Algorithm: AES-256-GCM with random nonce per encryption
//! - Key derivation: Direct key storage (no password-based derivation needed)
//!
//! Security properties:
//! - Keys never written to disk in plaintext
//! - Each encryption uses a fresh random nonce
//! - Authentication tag prevents tampering
//! - Memory is zeroed after use (zeroize)

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ring::aead::{Aad, BoundKey, Nonce, SealingKey, OpeningKey, AES_256_GCM, NonceSequence};
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
    nonce: String,        // Base64-encoded nonce
    ciphertext: String,   // Base64-encoded ciphertext + tag
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

/// Secure secrets storage with AES-256-GCM encryption
pub struct EncryptedSecrets {
    path: PathBuf,
    rng: SystemRandom,
    #[allow(dead_code)]
    keyring: keyring::Entry,
}

impl EncryptedSecrets {
    /// Create a new secrets storage at the default location
    pub fn new() -> Result<Self> {
        let path = Self::default_path()?;

        // Create keyring entry for master key
        let keyring = keyring::Entry::new(KEYCHAIN_SERVICE, MASTER_KEY_ID)
            .map_err(|e| SecretsError::KeychainError(e.to_string()))?;

        let rng = SystemRandom::new();

        let storage = Self { path, rng, keyring };

        // Ensure the master key exists
        storage.ensure_master_key()?;

        Ok(storage)
    }

    /// Get the default path for secrets file
    fn default_path() -> Result<PathBuf> {
        let data_dir = dirs::data_local_dir()
            .or_else(|| dirs::data_dir())
            .context("Cannot determine data directory")?;
        Ok(data_dir.join("homun").join("secrets.enc"))
    }

    /// Ensure a master key exists in the keychain
    fn ensure_master_key(&self) -> Result<()> {
        // Try to get existing key
        match self.keyring.get_password() {
            Ok(_) => {
                tracing::debug!("Master key already exists in keychain");
                Ok(())
            }
            Err(keyring::Error::NoEntry) => {
                tracing::info!("Generating new master key for secrets encryption");
                let key = self.generate_key()?;
                self.keyring
                    .set_password(&key)
                    .map_err(|e| SecretsError::KeychainError(e.to_string()))?;
                tracing::info!("Master key stored in OS keychain");
                Ok(())
            }
            Err(e) => Err(SecretsError::KeychainError(e.to_string()).into()),
        }
    }

    /// Generate a new random key (Base64-encoded 32 bytes)
    fn generate_key(&self) -> Result<String> {
        let mut key_bytes = [0u8; 32];
        self.rng.fill(&mut key_bytes)
            .map_err(|e| SecretsError::EncryptionError(e.to_string()))?;
        Ok(BASE64.encode(key_bytes))
    }

    /// Get the master key from keychain
    fn get_master_key(&self) -> Result<Zeroizing<[u8; 32]>> {
        let key_b64 = self.keyring
            .get_password()
            .map_err(|e| SecretsError::KeychainError(e.to_string()))?;

        let mut key_bytes = Zeroizing::new([0u8; 32]);
        BASE64.decode_slice(&key_b64, &mut *key_bytes)
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
        let plaintext = serde_json::to_vec(&data)
            .context("Failed to serialize secrets")?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        self.rng.fill(&mut nonce_bytes)
            .map_err(|e| SecretsError::EncryptionError(e.to_string()))?;

        // Encrypt with AES-256-GCM
        let unbound_key = ring::aead::UnboundKey::new(&AES_256_GCM, &*master_key)
            .map_err(|e| SecretsError::EncryptionError(format!("{:?}", e)))?;
        let mut sealing_key = SealingKey::new(unbound_key, SingleNonce::new(nonce_bytes));

        let mut ciphertext = plaintext;
        sealing_key.seal_in_place_append_tag(Aad::empty(), &mut ciphertext)
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
        let json = std::fs::read_to_string(&self.path)
            .context("Failed to read secrets file")?;

        let file_data: SecretsFile = serde_json::from_str(&json)
            .context("Failed to parse secrets file")?;

        // Decode nonce and ciphertext
        let mut nonce_bytes = [0u8; 12];
        BASE64.decode_slice(&file_data.nonce, &mut nonce_bytes)
            .context("Failed to decode nonce")?;

        let mut ciphertext = BASE64.decode(&file_data.ciphertext)
            .context("Failed to decode ciphertext")?;

        // Decrypt with AES-256-GCM
        let unbound_key = ring::aead::UnboundKey::new(&AES_256_GCM, &*master_key)
            .map_err(|e| SecretsError::DecryptionError(format!("{:?}", e)))?;
        let mut opening_key = OpeningKey::new(unbound_key, SingleNonce::new(nonce_bytes));

        let plaintext = opening_key.open_in_place(Aad::empty(), &mut ciphertext)
            .map_err(|e| SecretsError::DecryptionError(format!("{:?}", e)))?;

        // Deserialize
        let data: SecretsData = serde_json::from_slice(plaintext)
            .context("Failed to parse decrypted secrets")?;

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

    /// Check if secrets file exists
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Get the path to the secrets file
    pub fn path(&self) -> &std::path::Path {
        &self.path
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
        /// Create a test instance with a custom path
        fn test_new(path: PathBuf) -> Result<Self> {
            let rng = SystemRandom::new();
            let keyring = keyring::Entry::new(
                &format!("{}.test", KEYCHAIN_SERVICE),
                &format!("{}.{:?}", MASTER_KEY_ID, std::time::Instant::now()),
            ).map_err(|e| SecretsError::KeychainError(e.to_string()))?;

            let storage = Self { path, rng, keyring };
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
