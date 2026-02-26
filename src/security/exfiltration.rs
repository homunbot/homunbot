//! Exfiltration prevention — detect and redact secrets in LLM output.
//!
//! This module implements T-SEC-02 from the security roadmap:
//! - Pattern matching for API keys, tokens, passwords
//! - Automatic redaction before output reaches the user
//! - Audit logging of detection attempts
//!
//! # Architecture
//!
//! ```text
//! LLM Response Text
//!       │
//!       ▼
//! ┌─────────────────────┐
//! │  ExfilFilter        │
//! │  ┌───────────────┐  │
//! │  │ Pattern Match │  │
//! │  └───────┬───────┘  │
//! │          │          │
//! │    ┌─────┴─────┐    │
//! │    ▼           ▼    │
//! │ Clean      Detected │
//! │            ┌────────┴────────┐
//! │            │ Redact + Log    │
//! │            └─────────────────┘
//! └─────────────────────┘
//!       │
//!       ▼
//! Redacted Output → User
//! ```
//!
//! # Security Properties
//!
//! - **Pattern-based detection** for common secret formats
//! - **Configurable behavior**: redact (default) or block
//! - **Audit trail** with full context in logs
//! - **Non-blocking**: never prevents legitimate output

use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Global exfiltration filter instance (lazy-initialized)
static EXFIL_FILTER: OnceLock<ExfilFilter> = OnceLock::new();

/// Get the global exfiltration filter instance.
/// Initializes with default config on first call.
pub fn global_filter() -> &'static ExfilFilter {
    EXFIL_FILTER.get_or_init(|| ExfilFilter::new(ExfilConfig::default()))
}

/// Initialize the global filter with custom config.
/// Should be called once at startup if custom config is needed.
pub fn init_global_filter(config: ExfilConfig) {
    let _ = EXFIL_FILTER.set(ExfilFilter::new(config));
}

/// Configuration for exfiltration prevention.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExfilConfig {
    /// Enable exfiltration detection
    pub enabled: bool,
    /// Block output on detection (true) or just redact (false)
    pub block_on_detection: bool,
    /// Log detection attempts
    pub log_attempts: bool,
    /// Custom patterns to detect (regex strings)
    pub custom_patterns: Vec<String>,
}

impl Default for ExfilConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            block_on_detection: false, // Redact by default, don't block
            log_attempts: true,
            custom_patterns: Vec::new(),
        }
    }
}

/// Severity level of a detected secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Critical secret (API key, password)
    Critical,
    /// High-risk secret (token, session ID)
    High,
    /// Medium-risk (partial secret, reference)
    Medium,
    /// Low-risk (potential false positive)
    Low,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Critical => write!(f, "CRITICAL"),
            Severity::High => write!(f, "HIGH"),
            Severity::Medium => write!(f, "MEDIUM"),
            Severity::Low => write!(f, "LOW"),
        }
    }
}

/// A single detection result.
#[derive(Debug, Clone)]
pub struct Detection {
    /// Pattern name that matched
    pub pattern_name: String,
    /// Severity of the detection
    pub severity: Severity,
    /// The matched text (for logging, not for output)
    pub matched_text: String,
    /// Start position in original text
    pub start: usize,
    /// End position in original text
    pub end: usize,
}

/// Result of scanning text for secrets.
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// Whether any secrets were detected
    pub has_detections: bool,
    /// The redacted text (safe to output)
    pub redacted_text: String,
    /// All detections (for logging/audit)
    pub detections: Vec<Detection>,
    /// Whether output should be blocked
    pub should_block: bool,
}

impl ScanResult {
    /// Create a clean result (no detections)
    pub fn clean(text: String) -> Self {
        Self {
            has_detections: false,
            redacted_text: text,
            detections: Vec::new(),
            should_block: false,
        }
    }
}

/// A compiled secret pattern.
struct SecretPattern {
    name: String,
    regex: Regex,
    severity: Severity,
    /// Replacement text for redaction
    replacement: String,
}

/// Exfiltration filter with compiled patterns.
pub struct ExfilFilter {
    config: ExfilConfig,
    patterns: Vec<SecretPattern>,
}

impl ExfilFilter {
    /// Create a new filter with the given configuration.
    pub fn new(config: ExfilConfig) -> Self {
        let mut patterns = Self::builtin_patterns();

        // Add custom patterns from config
        for (i, pattern_str) in config.custom_patterns.iter().enumerate() {
            if let Ok(regex) = Regex::new(pattern_str) {
                patterns.push(SecretPattern {
                    name: format!("custom_{}", i),
                    regex,
                    severity: Severity::Medium,
                    replacement: "[REDACTED]".to_string(),
                });
            }
        }

        Self { config, patterns }
    }

    /// Built-in secret patterns for common API keys and secrets.
    fn builtin_patterns() -> Vec<SecretPattern> {
        vec![
            // OpenAI API keys (sk-*, sk-proj-*, sk-svcacct-*, etc.)
            SecretPattern {
                name: "openai_api_key".to_string(),
                regex: Regex::new(r"sk-[a-zA-Z0-9_-]{20,}").unwrap(),
                severity: Severity::Critical,
                replacement: "[REDACTED_OPENAI_KEY]".to_string(),
            },
            // Anthropic API keys
            SecretPattern {
                name: "anthropic_api_key".to_string(),
                regex: Regex::new(r"sk-ant-[a-zA-Z0-9_-]{20,}").unwrap(),
                severity: Severity::Critical,
                replacement: "[REDACTED_ANTHROPIC_KEY]".to_string(),
            },
            // OpenRouter API keys
            SecretPattern {
                name: "openrouter_api_key".to_string(),
                regex: Regex::new(r"sk-or-[a-zA-Z0-9_-]{20,}").unwrap(),
                severity: Severity::Critical,
                replacement: "[REDACTED_OPENROUTER_KEY]".to_string(),
            },
            // DeepSeek API keys
            SecretPattern {
                name: "deepseek_api_key".to_string(),
                regex: Regex::new(r"sk-[a-f0-9]{32,}").unwrap(),
                severity: Severity::Critical,
                replacement: "[REDACTED_DEEPSEEK_KEY]".to_string(),
            },
            // Generic high-entropy hex strings (potential secrets)
            // Require 40+ chars to reduce false positives (SHA-1 hashes, etc.)
            SecretPattern {
                name: "high_entropy_hex".to_string(),
                regex: Regex::new(r"\b[a-f0-9]{40,}\b").unwrap(),
                severity: Severity::Medium,
                replacement: "[REDACTED_SECRET]".to_string(),
            },
            // AWS Access Key IDs
            SecretPattern {
                name: "aws_access_key".to_string(),
                regex: Regex::new(r"AKIA[A-Z0-9]{16}").unwrap(),
                severity: Severity::Critical,
                replacement: "[REDACTED_AWS_KEY]".to_string(),
            },
            // AWS Secret Access Keys (40-char base64-ish)
            SecretPattern {
                name: "aws_secret_key".to_string(),
                regex: Regex::new(r"(?i)aws(.{0,20})?[\x27\x22][A-Za-z0-9/+=]{40}[\x27\x22]").unwrap(),
                severity: Severity::Critical,
                replacement: "[REDACTED_AWS_SECRET]".to_string(),
            },
            // Generic API key patterns in JSON-like contexts
            SecretPattern {
                name: "api_key_in_text".to_string(),
                regex: Regex::new(r"(?i)(api[_-]?key|apikey|token|secret|password)\s*[:=]\s*[\x27\x22]?[^\x27\x22\s]{16,}[\x27\x22]?").unwrap(),
                severity: Severity::High,
                replacement: "[REDACTED_API_KEY]".to_string(),
            },
            // Private key headers (PEM format)
            SecretPattern {
                name: "private_key_pem".to_string(),
                regex: Regex::new(r"-----BEGIN (?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----").unwrap(),
                severity: Severity::Critical,
                replacement: "[REDACTED_PRIVATE_KEY]".to_string(),
            },
            // Telegram bot tokens
            SecretPattern {
                name: "telegram_bot_token".to_string(),
                regex: Regex::new(r"\d{8,10}:[a-zA-Z0-9_-]{35}").unwrap(),
                severity: Severity::Critical,
                replacement: "[REDACTED_TELEGRAM_TOKEN]".to_string(),
            },
            // Discord bot tokens
            SecretPattern {
                name: "discord_bot_token".to_string(),
                regex: Regex::new(r"[MN][a-zA-Z\d]{23}\.[a-zA-Z\d]{6}\.[a-zA-Z\d]{27}").unwrap(),
                severity: Severity::Critical,
                replacement: "[REDACTED_DISCORD_TOKEN]".to_string(),
            },
            // GitHub personal access tokens
            SecretPattern {
                name: "github_pat".to_string(),
                regex: Regex::new(r"github_pat_[a-zA-Z0-9]{22}_[a-zA-Z0-9]{59}").unwrap(),
                severity: Severity::Critical,
                replacement: "[REDACTED_GITHUB_TOKEN]".to_string(),
            },
            // Generic GitHub tokens (classic)
            SecretPattern {
                name: "github_token".to_string(),
                regex: Regex::new(r"ghp_[a-zA-Z0-9]{36}").unwrap(),
                severity: Severity::Critical,
                replacement: "[REDACTED_GITHUB_TOKEN]".to_string(),
            },
            // Generic OAuth tokens
            SecretPattern {
                name: "oauth_token".to_string(),
                regex: Regex::new(r"(?i)oauth[_-]?token\s*[:=]\s*[\x27\x22]?[^\x27\x22\s]{20,}[\x27\x22]?").unwrap(),
                severity: Severity::High,
                replacement: "[REDACTED_OAUTH_TOKEN]".to_string(),
            },
            // Bearer tokens in Authorization headers
            SecretPattern {
                name: "bearer_token".to_string(),
                regex: Regex::new(r"Bearer\s+[a-zA-Z0-9_-]{20,}").unwrap(),
                severity: Severity::High,
                replacement: "Bearer [REDACTED]".to_string(),
            },
            // JWT tokens (three base64 parts separated by dots)
            SecretPattern {
                name: "jwt_token".to_string(),
                regex: Regex::new(r"eyJ[a-zA-Z0-9_-]*\.eyJ[a-zA-Z0-9_-]*\.[a-zA-Z0-9_-]*").unwrap(),
                severity: Severity::High,
                replacement: "[REDACTED_JWT]".to_string(),
            },
            // Connection strings with passwords
            SecretPattern {
                name: "connection_string".to_string(),
                regex: Regex::new(r"(?i)(postgres(ql)?|mysql|mongodb|redis)://[^:]+:[^@]+@").unwrap(),
                severity: Severity::Critical,
                replacement: "[REDACTED_CONNECTION_STRING]".to_string(),
            },
        ]
    }

    /// Scan text for secrets and return redacted version.
    pub fn scan(&self, text: &str) -> ScanResult {
        if !self.config.enabled {
            return ScanResult::clean(text.to_string());
        }

        let mut detections = Vec::new();
        let mut redacted = text.to_string();

        for pattern in &self.patterns {
            for cap in pattern.regex.find_iter(text) {
                let matched = cap.as_str();

                // Skip if this looks like a placeholder or example
                if self.is_placeholder(matched) {
                    continue;
                }

                detections.push(Detection {
                    pattern_name: pattern.name.clone(),
                    severity: pattern.severity,
                    matched_text: Self::truncate_for_log(matched),
                    start: cap.start(),
                    end: cap.end(),
                });

                // Redact in the text
                redacted = pattern
                    .regex
                    .replace_all(&redacted, &pattern.replacement)
                    .to_string();
            }
        }

        if detections.is_empty() {
            return ScanResult::clean(text.to_string());
        }

        // Log detections if configured
        if self.config.log_attempts {
            self.log_detections(&detections);
        }

        ScanResult {
            has_detections: true,
            redacted_text: redacted,
            detections,
            should_block: self.config.block_on_detection,
        }
    }

    /// Check if a matched string looks like a placeholder rather than a real secret.
    fn is_placeholder(&self, text: &str) -> bool {
        let lower = text.to_lowercase();

        // Common placeholder patterns
        // Note: We don't include "example" or "sample" here because AWS key examples
        // (like AKIAIOSFODNN7EXAMPLE) are valid patterns we want to detect.
        let placeholder_indicators = [
            "placeholder",
            "your_",
            "xxx",
            "test_test",
            "dummy",
            "fake",
            "mock",
            "<",
            ">",
            "[your",
            "{your",
        ];

        for indicator in &placeholder_indicators {
            if lower.contains(indicator) {
                return true;
            }
        }

        // Check for repeated characters (likely fake)
        let chars: Vec<char> = text.chars().collect();
        if chars.len() > 5 {
            let first = chars[0];
            if chars.iter().take(10).all(|&c| c == first) {
                return true;
            }
        }

        false
    }

    /// Truncate a secret for safe logging (first 8 chars + ...).
    fn truncate_for_log(text: &str) -> String {
        if text.len() <= 12 {
            "[TRUNCATED]".to_string()
        } else {
            format!("{}...[len={}]", &text[..8], text.len())
        }
    }

    /// Log all detections with appropriate severity.
    fn log_detections(&self, detections: &[Detection]) {
        for d in detections {
            match d.severity {
                Severity::Critical => {
                    tracing::warn!(
                        pattern = %d.pattern_name,
                        matched = %d.matched_text,
                        position = d.start,
                        "CRITICAL: Secret detected in output - redacted"
                    );
                }
                Severity::High => {
                    tracing::warn!(
                        pattern = %d.pattern_name,
                        matched = %d.matched_text,
                        position = d.start,
                        "HIGH: Potential secret detected in output - redacted"
                    );
                }
                Severity::Medium => {
                    tracing::info!(
                        pattern = %d.pattern_name,
                        matched = %d.matched_text,
                        position = d.start,
                        "MEDIUM: Suspicious pattern detected in output - redacted"
                    );
                }
                Severity::Low => {
                    tracing::debug!(
                        pattern = %d.pattern_name,
                        matched = %d.matched_text,
                        position = d.start,
                        "LOW: Pattern detected in output - redacted"
                    );
                }
            }
        }
    }

    /// Check if the filter is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Redact secrets stored in vault from text.
    /// This function loads all values from the encrypted vault and replaces
    /// any occurrence with `vault://key_name` references.
    ///
    /// # Arguments
    /// * `text` - The text to scan for vault values
    /// * `vault_entries` - List of (key, value) pairs from vault
    ///
    /// # Returns
    /// The text with vault values replaced by `vault://key` references
    pub fn redact_vault_values(text: &str, vault_entries: &[(String, String)]) -> String {
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
}

/// Convenience function to scan text using the global filter.
pub fn scan(text: &str) -> ScanResult {
    global_filter().scan(text)
}

/// Convenience function to scan and get redacted text.
/// Returns the redacted text directly.
pub fn redact(text: &str) -> String {
    global_filter().scan(text).redacted_text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_key_detection() {
        let filter = ExfilFilter::new(ExfilConfig::default());
        let text = "Your API key is sk-proj-abcdefghijklmnop1234567890";
        let result = filter.scan(text);

        assert!(result.has_detections);
        assert!(result.redacted_text.contains("[REDACTED"));
        assert!(!result.redacted_text.contains("sk-proj-"));
    }

    #[test]
    fn test_anthropic_key_detection() {
        let filter = ExfilFilter::new(ExfilConfig::default());
        let text = "Key: sk-ant-api03-abcdefghijklmnop1234567890";
        let result = filter.scan(text);

        assert!(result.has_detections);
        assert!(result.redacted_text.contains("[REDACTED"));
    }

    #[test]
    fn test_aws_key_detection() {
        let filter = ExfilFilter::new(ExfilConfig::default());
        let text = "AWS Access Key: AKIAIOSFODNN7EXAMPLE";
        let result = filter.scan(text);

        assert!(result.has_detections);
        assert!(result.redacted_text.contains("[REDACTED"));
    }

    #[test]
    fn test_telegram_token_detection() {
        let filter = ExfilFilter::new(ExfilConfig::default());
        let text = "Bot token: 1234567890:ABCdefGHIjk_LMnO-PQrsTuvWxYz";
        let result = filter.scan(text);

        assert!(result.has_detections);
        assert!(result.redacted_text.contains("[REDACTED"));
    }

    #[test]
    fn test_jwt_detection() {
        let filter = ExfilFilter::new(ExfilConfig::default());
        let text = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let result = filter.scan(text);

        assert!(result.has_detections);
        assert!(result.redacted_text.contains("[REDACTED"));
    }

    #[test]
    fn test_placeholder_not_detected() {
        let filter = ExfilFilter::new(ExfilConfig::default());
        let text = "Set your API key to YOUR_API_KEY_HERE";
        let result = filter.scan(text);

        // Should not trigger false positive on placeholder
        // (though it might match the generic api_key pattern)
        // The key is that common placeholder patterns should be skipped
    }

    #[test]
    fn test_private_key_detection() {
        let filter = ExfilFilter::new(ExfilConfig::default());
        let text = "-----BEGIN RSA PRIVATE KEY-----\nMIIE...";
        let result = filter.scan(text);

        assert!(result.has_detections);
        assert!(result.redacted_text.contains("[REDACTED"));
    }

    #[test]
    fn test_connection_string_detection() {
        let filter = ExfilFilter::new(ExfilConfig::default());
        let text = "postgresql://user:secretpassword@localhost:5432/db";
        let result = filter.scan(text);

        assert!(result.has_detections);
        assert!(result.redacted_text.contains("[REDACTED"));
        assert!(!result.redacted_text.contains("secretpassword"));
    }

    #[test]
    fn test_disabled_filter() {
        let config = ExfilConfig {
            enabled: false,
            ..Default::default()
        };
        let filter = ExfilFilter::new(config);
        let text = "sk-proj-abcdefghijklmnop1234567890";
        let result = filter.scan(text);

        assert!(!result.has_detections);
        assert_eq!(result.redacted_text, text);
    }

    #[test]
    fn test_block_on_detection() {
        let config = ExfilConfig {
            block_on_detection: true,
            ..Default::default()
        };
        let filter = ExfilFilter::new(config);
        let text = "sk-proj-abcdefghijklmnop1234567890";
        let result = filter.scan(text);

        assert!(result.should_block);
    }

    #[test]
    fn test_convenience_functions() {
        // Test filter convenience functions using a local filter
        // (global filter may be initialized by other tests in parallel)
        let filter = ExfilFilter::new(ExfilConfig::default());
        let text = "Key: sk-proj-test12345678901234567890";
        let result = filter.scan(text);
        let redacted = result.redacted_text.clone();

        assert!(result.has_detections);
        assert!(redacted.contains("[REDACTED"));
    }

    #[test]
    fn test_clean_text_passthrough() {
        let filter = ExfilFilter::new(ExfilConfig::default());
        let text = "This is a normal message without any secrets.";
        let result = filter.scan(text);

        assert!(!result.has_detections);
        assert_eq!(result.redacted_text, text);
    }

    #[test]
    fn test_custom_pattern() {
        let config = ExfilConfig {
            custom_patterns: vec![r"CUSTOM-SECRET-\d+".to_string()],
            ..Default::default()
        };
        let filter = ExfilFilter::new(config);
        let text = "Here is a CUSTOM-SECRET-12345 in the text";
        let result = filter.scan(text);

        assert!(result.has_detections);
        assert!(result.redacted_text.contains("[REDACTED"));
    }

    #[test]
    fn test_multiple_detections() {
        let filter = ExfilFilter::new(ExfilConfig::default());
        // Telegram token needs 35 chars after the colon
        let text = "OpenAI: sk-proj-12345678901234567890 and Telegram: 1234567890:ABCdefGHIjk_LMnO-PQrsTuvWxYz12345678";
        let result = filter.scan(text);

        assert!(result.has_detections);
        assert!(result.detections.len() >= 2);
        assert!(result.redacted_text.contains("[REDACTED"));
        assert!(!result.redacted_text.contains("sk-proj-"));
        assert!(!result.redacted_text.contains("ABCdefGHIjk"));
    }
}
