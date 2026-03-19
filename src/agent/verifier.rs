//! Action Verifier — DISABLED.
//!
//! Originally designed to detect hallucinations where the LLM claims to have
//! done something without calling the tool. However, pattern-based detection
//! causes false positives when the LLM states facts (e.g., "è già salvato nel file")
//! and doesn't scale across languages.
//!
//! Now relies on system prompt instructions to guide the LLM.

/// Result of verification: always Verified (verification disabled).
#[derive(Debug)]
pub enum VerificationResult {
    /// Response is verified.
    Verified,
}

/// Verify actions — DISABLED, always returns Verified.
///
/// The system prompt already instructs the LLM:
/// "NEVER say 'done', 'saved', 'fatto', 'aggiunto' WITHOUT calling the tool first."
///
/// We trust the LLM to follow these instructions rather than using
/// language-specific pattern matching that causes false positives.
pub fn verify_actions(_response: &str, _tools_used: &[String]) -> VerificationResult {
    VerificationResult::Verified
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verification is now DISABLED — all responses return Verified.
    // We trust the LLM to follow system prompt instructions.

    #[test]
    fn test_always_verified_no_claims() {
        let response = "The weather today is sunny with a high of 25 degrees.";
        let result = verify_actions(response, &[]);
        assert!(matches!(result, VerificationResult::Verified));
    }

    #[test]
    fn test_always_verified_with_tool() {
        let response = "I saved the changes to USER.md";
        let result = verify_actions(response, &["write_file".to_string()]);
        assert!(matches!(result, VerificationResult::Verified));
    }

    #[test]
    fn test_always_verified_fact_statement() {
        // This was the bug case — "è già salvato" was triggering verification
        let response = "Il tuo gruppo sanguigno è già salvato nel file USER.md";
        let result = verify_actions(response, &[]);
        assert!(matches!(result, VerificationResult::Verified));
    }

    #[test]
    fn test_always_verified_italian() {
        let response = "Ho salvato le informazioni su USER.md";
        let result = verify_actions(response, &[]);
        assert!(matches!(result, VerificationResult::Verified));
    }

    #[test]
    fn test_always_verified_any_language() {
        // Works for any language — no pattern matching needed
        let response = "Es ist bereits gespeichert";
        let result = verify_actions(response, &[]);
        assert!(matches!(result, VerificationResult::Verified));
    }
}
