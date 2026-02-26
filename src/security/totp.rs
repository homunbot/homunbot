//! TOTP (Time-based One-Time Password) management.
//!
//! Compatible with Google Authenticator, Authy, 1Password, Bitwarden, etc.
//! Uses RFC 6238 with SHA-1, 6-digit codes, 30-second periods.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rand::Rng;
use thiserror::Error;
use totp_rs::{Algorithm, Secret, TOTP};

/// TOTP-related errors
#[derive(Error, Debug)]
pub enum TotpError {
    #[error("Invalid TOTP code")]
    InvalidCode,

    #[error("TOTP secret error: {0}")]
    SecretError(String),

    #[error("Failed to generate QR code: {0}")]
    QrError(String),

    #[error("Clock skew too large")]
    ClockSkew,
}

/// TOTP manager for generating and verifying codes.
pub struct TotpManager {
    totp: TOTP,
}

impl TotpManager {
    /// Create a new TOTP manager with the given Base32 secret.
    pub fn new(secret: &str, account: &str) -> Result<Self> {
        let secret_bytes = Secret::Encoded(secret.to_string())
            .to_bytes()
            .map_err(|e| TotpError::SecretError(e.to_string()))?;

        let totp = TOTP::new(
            Algorithm::SHA1,           // Standard algorithm
            6,                         // 6 digits
            1,                         // Skew (allowed steps before/after)
            30,                        // Period in seconds
            secret_bytes,              // Secret bytes
            Some("Homun".to_string()), // Issuer
            account.to_string(),       // Account name (e.g., user@host)
        )
        .context("Failed to create TOTP instance")?;

        Ok(Self { totp })
    }

    /// Generate a new random TOTP secret (Base32 encoded).
    pub fn generate_secret() -> String {
        // Generate 20 random bytes (160 bits, typical for TOTP)
        let mut rng = rand::thread_rng();
        let bytes: Vec<u8> = (0..20).map(|_| rng.gen::<u8>()).collect();
        Secret::Raw(bytes).to_encoded().to_string()
    }

    /// Get the otpauth:// URL for QR code generation.
    pub fn get_url(&self) -> String {
        self.totp.get_url()
    }

    /// Generate a QR code as base64-encoded PNG.
    /// Returns a data URL suitable for HTML img src.
    pub fn generate_qr_base64(&self) -> Result<String> {
        let qr = self
            .totp
            .get_qr_base64()
            .map_err(|e| TotpError::QrError(e.to_string()))?;

        Ok(format!("data:image/png;base64,{}", qr))
    }

    /// Generate the current TOTP code.
    pub fn generate_current(&self) -> Result<String> {
        self.totp
            .generate_current()
            .context("Failed to generate TOTP code")
    }

    /// Verify a TOTP code with ±1 window tolerance for clock skew.
    ///
    /// The ±1 window allows codes from the previous, current, and next
    /// 30-second periods to be valid (90 seconds total window).
    pub fn verify(&self, code: &str) -> bool {
        // Strip any whitespace from user input
        let code = code.trim();

        // Check current and ±1 window (90 seconds total)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Generate expected code for logging
        let expected = self.generate_current().unwrap_or_default();
        tracing::debug!(
            provided_code = %code,
            expected_code = %expected,
            now_ts = now,
            "TOTP verify attempt"
        );

        // Try current, previous, and next period
        for offset in [-1i64, 0, 1] {
            let time = (now as i64 + offset * 30) as u64;
            if self.totp.check(code, time) {
                tracing::debug!(offset = offset, "TOTP code verified");
                return true;
            }
        }

        tracing::warn!("TOTP code verification failed");
        false
    }

    /// Verify a TOTP code with strict timing (no window tolerance).
    pub fn verify_strict(&self, code: &str) -> bool {
        self.totp.check_current(code.trim()).unwrap_or(false)
    }
}

/// Generate recovery codes (10 codes, format: XXXX-XXXX)
pub fn generate_recovery_codes() -> Vec<String> {
    let mut rng = rand::thread_rng();
    (0..10)
        .map(|_| {
            let part1: u16 = rng.gen();
            let part2: u16 = rng.gen();
            format!("{:04X}-{:04X}", part1, part2)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_secret() {
        let secret = TotpManager::generate_secret();
        // Base32 secrets are typically 16+ characters
        assert!(secret.len() >= 16);
        // Should only contain valid Base32 characters
        assert!(secret
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
    }

    #[test]
    fn test_totp_url_format() {
        // Use a 26-character secret (130 bits) to meet the 128-bit minimum requirement
        let secret = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3P";
        let manager = TotpManager::new(secret, "test@example.com").unwrap();
        let url = manager.get_url();

        assert!(url.starts_with("otpauth://totp/"));
        assert!(url.contains("secret="));
        assert!(url.contains("issuer=Homun"));
    }

    #[test]
    fn test_generate_and_verify() {
        let secret = TotpManager::generate_secret();
        let manager = TotpManager::new(&secret, "test@example.com").unwrap();

        let code = manager.generate_current().unwrap();
        assert_eq!(code.len(), 6);
        assert!(manager.verify(&code));
    }

    #[test]
    fn test_verify_with_whitespace() {
        let secret = TotpManager::generate_secret();
        let manager = TotpManager::new(&secret, "test@example.com").unwrap();

        let code = manager.generate_current().unwrap();
        // Should handle whitespace in input
        assert!(manager.verify(&format!(" {} ", code)));
    }

    #[test]
    fn test_generate_recovery_codes() {
        let codes = generate_recovery_codes();
        assert_eq!(codes.len(), 10);

        // Each code should be in format XXXX-XXXX
        for code in &codes {
            assert_eq!(code.len(), 9);
            assert!(code.chars().nth(4).unwrap() == '-');
        }

        // Codes should be unique
        let unique: std::collections::HashSet<_> = codes.iter().collect();
        assert_eq!(unique.len(), 10);
    }
}
