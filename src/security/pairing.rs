//! DM Pairing — OTP-based authentication for unknown channel senders.
//!
//! When `pairing_required = true` on a channel, senders not in `allow_from`
//! and not already linked via `user_identities` receive a 6-digit OTP code.
//! Once verified, they are registered as trusted users.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use anyhow::Result;
use rand::Rng;
use tokio::sync::RwLock;

use crate::storage::Database;
use crate::user::UserManager;

/// How long a pairing code remains valid.
const CODE_TTL_SECS: u64 = 300; // 5 minutes

/// Maximum verification attempts before the code is invalidated.
const MAX_ATTEMPTS: u32 = 3;

struct PairingRequest {
    code: String,
    display_name: Option<String>,
    created_at: Instant,
    attempts: u32,
}

/// Manages OTP-based pairing for channel senders.
pub struct PairingManager {
    user_manager: UserManager,
    pending: RwLock<HashMap<String, PairingRequest>>, // key: "channel:platform_id"
}

impl PairingManager {
    pub fn new(db: Database) -> Self {
        Self {
            user_manager: UserManager::new(db),
            pending: RwLock::new(HashMap::new()),
        }
    }

    /// Check if a sender needs pairing. Returns:
    /// - `Ok(None)` — sender is trusted, proceed with message normally
    /// - `Ok(Some(response))` — sender needs pairing, send this text back instead
    pub async fn check_sender(
        &self,
        channel: &str,
        sender_id: &str,
        display_name: Option<&str>,
        message: &str,
        pairing_required: bool,
        allow_from: &HashSet<String>,
    ) -> Result<Option<String>> {
        // Pairing disabled — let the channel's own allow_from logic handle it
        if !pairing_required {
            return Ok(None);
        }

        // Pre-approved via allow_from
        if allow_from.contains(sender_id) {
            return Ok(None);
        }

        // Already linked in user_identities
        if self
            .user_manager
            .lookup_by_channel(channel, sender_id)
            .await?
            .is_some()
        {
            return Ok(None);
        }

        let key = format!("{channel}:{sender_id}");
        let trimmed = message.trim();

        // Check if the message looks like a 6-digit code
        let is_code = trimmed.len() == 6 && trimmed.chars().all(|c| c.is_ascii_digit());

        if is_code {
            return self
                .verify_code(&key, channel, sender_id, display_name, trimmed)
                .await;
        }

        // Generate a new code (or return existing if still valid)
        self.issue_code(&key, display_name).await
    }

    /// Verify a submitted code.
    async fn verify_code(
        &self,
        key: &str,
        channel: &str,
        sender_id: &str,
        display_name: Option<&str>,
        code: &str,
    ) -> Result<Option<String>> {
        let mut pending = self.pending.write().await;

        let request = match pending.get_mut(key) {
            Some(r) => r,
            None => {
                // No pending request — issue a fresh code
                drop(pending);
                return self.issue_code(key, display_name).await;
            }
        };

        // Check expiry
        if request.created_at.elapsed().as_secs() > CODE_TTL_SECS {
            pending.remove(key);
            drop(pending);
            return self.issue_code(key, display_name).await;
        }

        // Check code
        if request.code == code {
            let name = request
                .display_name
                .clone()
                .unwrap_or_else(|| sender_id.to_string());
            pending.remove(key);
            drop(pending);

            // Create user and link identity
            let user = self.user_manager.create_user(&name).await?;
            self.user_manager
                .link_identity(&user.id, channel, sender_id, display_name)
                .await?;

            tracing::info!(
                channel = %channel,
                sender_id = %sender_id,
                user_id = %user.id,
                "Pairing successful"
            );

            Ok(Some(format!(
                "Pairing successful! Welcome, {name}. You can now chat with me."
            )))
        } else {
            request.attempts += 1;
            if request.attempts >= MAX_ATTEMPTS {
                pending.remove(key);
                drop(pending);
                return self.issue_code(key, display_name).await;
            }

            let remaining = MAX_ATTEMPTS - request.attempts;
            Ok(Some(format!(
                "Invalid code. {remaining} attempt(s) remaining. Please try again."
            )))
        }
    }

    /// Issue a new pairing code (or return existing valid one).
    async fn issue_code(&self, key: &str, display_name: Option<&str>) -> Result<Option<String>> {
        let mut pending = self.pending.write().await;

        // Return existing code if still valid
        if let Entry::Occupied(entry) = pending.entry(key.to_string()) {
            if entry.get().created_at.elapsed().as_secs() <= CODE_TTL_SECS {
                let code = &entry.get().code;
                return Ok(Some(format!(
                    "Please enter your pairing code to continue: {code}\n\
                     The code expires in 5 minutes."
                )));
            }
            entry.remove();
        }

        let code = generate_code();
        pending.insert(
            key.to_string(),
            PairingRequest {
                code: code.clone(),
                display_name: display_name.map(String::from),
                created_at: Instant::now(),
                attempts: 0,
            },
        );

        tracing::info!(key = %key, "Pairing code issued");

        Ok(Some(format!(
            "Welcome! To use this bot, please enter this pairing code: {code}\n\
             The code expires in 5 minutes."
        )))
    }

    /// Remove expired pairing requests.
    pub async fn cleanup_expired(&self) {
        let mut pending = self.pending.write().await;
        pending.retain(|_, req| req.created_at.elapsed().as_secs() <= CODE_TTL_SECS);
    }
}

/// Generate a random 6-digit code.
fn generate_code() -> String {
    let code: u32 = rand::thread_rng().gen_range(100_000..1_000_000);
    code.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_code_format() {
        for _ in 0..100 {
            let code = generate_code();
            assert_eq!(code.len(), 6);
            assert!(code.chars().all(|c| c.is_ascii_digit()));
            let n: u32 = code.parse().unwrap();
            assert!(n >= 100_000 && n < 1_000_000);
        }
    }
}
