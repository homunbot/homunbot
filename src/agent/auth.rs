//! Unified channel authorization.
//!
//! All inbound messages pass through [`check_authorization`] in the gateway
//! routing loop. Individual channels are transport-only — they forward
//! everything and let the gateway decide.

use std::collections::HashSet;

use crate::storage::Database;

/// Result of the authorization check for an inbound message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthDecision {
    /// Sender is authorized — proceed with message processing.
    Authorized,
    /// Sender is unknown but pairing is enabled — hand off to OTP flow.
    NeedsPairing,
    /// Sender is unauthorized and pairing is disabled — drop the message.
    Rejected,
}

/// Check whether a sender is authorized to interact with the agent.
///
/// Pipeline:
/// 1. Is sender in the `allow_from` set? (includes contact identities merged at startup)
/// 2. Live DB lookup — catches contacts added after gateway startup.
/// 3. If still unknown: pairing enabled → [`AuthDecision::NeedsPairing`], else → [`AuthDecision::Rejected`].
pub async fn check_authorization(
    db: &Database,
    channel: &str,
    sender_id: &str,
    allow_from: &HashSet<String>,
    pairing_required: bool,
) -> AuthDecision {
    // 1. Static allow_from (includes contacts merged at startup)
    if allow_from.contains(sender_id) {
        return AuthDecision::Authorized;
    }

    // 1b. Email domain matching: allow_from entries like "@example.com" or "example.com"
    if channel.starts_with("email") {
        let sender_lower = sender_id.to_lowercase();
        for entry in allow_from {
            if entry == "*" {
                return AuthDecision::Authorized;
            }
            if entry.starts_with('@') && sender_lower.ends_with(&entry.to_lowercase()) {
                return AuthDecision::Authorized;
            }
            if !entry.contains('@') {
                let domain_suffix = format!("@{}", entry.to_lowercase());
                if sender_lower.ends_with(&domain_suffix) {
                    return AuthDecision::Authorized;
                }
            }
        }
    }

    // 2. Live contact DB lookup (handles contacts added after startup)
    let channel_key = if channel.starts_with("email:") {
        "email"
    } else {
        channel
    };
    if let Ok(Some(_)) = db.find_contact_by_identity(channel_key, sender_id).await {
        return AuthDecision::Authorized;
    }

    // 3. Unknown sender
    if pairing_required {
        AuthDecision::NeedsPairing
    } else {
        AuthDecision::Rejected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_allow_set(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    async fn test_db() -> (Database, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path).await.unwrap();
        (db, dir)
    }

    #[tokio::test]
    async fn authorized_via_allow_from() {
        let (db, _dir) = test_db().await;
        let allow = make_allow_set(&["user123"]);
        let result = check_authorization(&db, "telegram", "user123", &allow, false).await;
        assert_eq!(result, AuthDecision::Authorized);
    }

    #[tokio::test]
    async fn rejected_when_unknown_no_pairing() {
        let (db, _dir) = test_db().await;
        let allow = make_allow_set(&["other"]);
        let result = check_authorization(&db, "telegram", "stranger", &allow, false).await;
        assert_eq!(result, AuthDecision::Rejected);
    }

    #[tokio::test]
    async fn needs_pairing_when_unknown_with_pairing() {
        let (db, _dir) = test_db().await;
        let allow = make_allow_set(&["other"]);
        let result = check_authorization(&db, "telegram", "stranger", &allow, true).await;
        assert_eq!(result, AuthDecision::NeedsPairing);
    }

    #[tokio::test]
    async fn authorized_via_contact_db() {
        let (db, _dir) = test_db().await;
        let contact_id = db
            .insert_contact("Test User", None, None, None, None, None, None, None, None, None)
            .await
            .unwrap();
        db.insert_contact_identity(contact_id, "telegram", "tg_user_42", None)
            .await
            .unwrap();

        let allow = make_allow_set(&[]); // empty allow_from
        let result = check_authorization(&db, "telegram", "tg_user_42", &allow, false).await;
        assert_eq!(result, AuthDecision::Authorized);
    }

    #[tokio::test]
    async fn email_channel_uses_email_key_for_lookup() {
        let (db, _dir) = test_db().await;
        let contact_id = db
            .insert_contact("Email User", None, None, None, None, None, None, None, None, None)
            .await
            .unwrap();
        db.insert_contact_identity(contact_id, "email", "user@example.com", None)
            .await
            .unwrap();

        let allow = make_allow_set(&[]);
        // Channel name is "email:work" but DB stores "email"
        let result =
            check_authorization(&db, "email:work", "user@example.com", &allow, false).await;
        assert_eq!(result, AuthDecision::Authorized);
    }
}
