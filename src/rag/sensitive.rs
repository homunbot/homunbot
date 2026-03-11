//! Sensitive data detection for RAG chunks.
//!
//! Detects patterns like API keys, tokens, passwords, private keys, IBAN, etc.
//! Chunks containing sensitive data are marked for redaction.

use regex::Regex;
use std::sync::LazyLock;

/// Compiled regex patterns for sensitive content detection.
static SENSITIVE_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // API keys and tokens (named patterns)
        r"(?i)(api[_\-]?key|api[_\-]?secret|access[_\-]?token|auth[_\-]?token|bearer)\s*[:=]\s*\S{10,}",
        // OpenAI / Anthropic / GitHub / GitLab / AWS key formats
        r"sk-[a-zA-Z0-9]{20,}",
        r"sk-ant-[a-zA-Z0-9\-]{20,}",
        r"ghp_[a-zA-Z0-9]{36,}",
        r"gho_[a-zA-Z0-9]{36,}",
        r"glpat-[a-zA-Z0-9\-]{20,}",
        r"AKIA[0-9A-Z]{16}",
        r"xoxb-[0-9a-zA-Z\-]{20,}",
        // Passwords
        r"(?i)(password|passwd|pwd)\s*[:=]\s*\S{6,}",
        // Private keys
        r"-----BEGIN\s+(RSA\s+|EC\s+|DSA\s+|OPENSSH\s+)?PRIVATE KEY-----",
        // JWT tokens
        r"eyJ[a-zA-Z0-9_\-]{10,}\.eyJ[a-zA-Z0-9_\-]{10,}\.[a-zA-Z0-9_\-]+",
        // Connection strings with credentials
        r"(?i)(mysql|postgres|postgresql|mongodb|redis|amqp)://\S+:\S+@",
        // IBAN
        r"\b[A-Z]{2}\d{2}\s?[A-Z0-9]{4}\s?[A-Z0-9]{4}\s?[A-Z0-9]{4}(?:\s?[A-Z0-9]{4}){0,4}(?:\s?[A-Z0-9]{1,4})?\b",
        // Credit card numbers (basic 16-digit patterns)
        r"\b\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{4}\b",
    ]
    .iter()
    .filter_map(|p| Regex::new(p).ok())
    .collect()
});

/// File names that suggest sensitive content.
static SENSITIVE_NAME_PATTERNS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(password|secret|token|key|recovery|credential|private|\.pem$|\.key$)")
        .expect("valid regex")
});

/// Check if text content contains sensitive data patterns.
pub fn is_sensitive(text: &str) -> bool {
    SENSITIVE_PATTERNS.iter().any(|re| re.is_match(text))
}

/// Check if a filename suggests sensitive content.
pub fn is_sensitive_filename(name: &str) -> bool {
    SENSITIVE_NAME_PATTERNS.is_match(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detects_api_key() {
        assert!(is_sensitive(
            "api_key: sk-abc123456789012345678901234567890"
        ));
        assert!(is_sensitive(
            "OPENAI_API_KEY=sk-proj-abcdef1234567890abcdef12"
        ));
        assert!(is_sensitive(
            "token = ghp_1234567890abcdef1234567890abcdef12345678"
        ));
    }

    #[test]
    fn test_detects_private_key() {
        assert!(is_sensitive("-----BEGIN RSA PRIVATE KEY-----\nMIIE..."));
        assert!(is_sensitive("-----BEGIN PRIVATE KEY-----\ndata"));
    }

    #[test]
    fn test_detects_password() {
        assert!(is_sensitive("password: mySecretPass123"));
        assert!(is_sensitive("DB_PASSWORD=supersecret"));
    }

    #[test]
    fn test_detects_connection_string() {
        assert!(is_sensitive("postgres://admin:secret@db.host:5432/mydb"));
    }

    #[test]
    fn test_normal_text_not_sensitive() {
        assert!(!is_sensitive(
            "This is a normal document about programming."
        ));
        assert!(!is_sensitive("The key to success is hard work."));
        assert!(!is_sensitive("Meeting notes from today's session."));
    }

    #[test]
    fn test_sensitive_filename() {
        assert!(is_sensitive_filename("passwords.txt"));
        assert!(is_sensitive_filename("api-secret.env"));
        assert!(is_sensitive_filename("recovery-key.md"));
        assert!(is_sensitive_filename("server.key"));
        assert!(is_sensitive_filename("cert.pem"));
        assert!(!is_sensitive_filename("readme.md"));
        assert!(!is_sensitive_filename("config.toml"));
    }
}
