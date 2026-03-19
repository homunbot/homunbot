//! Email approval handler for the Assisted mode flow.
//!
//! When an email arrives in Assisted mode, the agent generates a draft and stores it
//! in `email_pending`. The draft is sent to a notify channel (e.g. Telegram).
//! The user can then approve, reject, or request modifications via text commands.
//!
//! Commands (case-insensitive, with optional index number):
//!   Approve: ok, invia, send, sì, yes, manda, approva
//!   Reject:  rifiuta, reject, no, scarta, annulla
//!   List:    lista, pending, mostra, show
//!   Modify:  any other text is treated as feedback for the agent

use std::collections::HashMap;

use crate::storage::{Database, EmailPendingRow};

/// Result of checking an inbound message against pending email approvals.
#[derive(Debug)]
pub enum ApprovalAction {
    /// Not a notify channel or no pending drafts — proceed normally.
    NotApplicable,
    /// User approved a draft for sending.
    Approve { pending: EmailPendingRow },
    /// User rejected a draft.
    Reject { pending_id: String },
    /// User requested the list of pending drafts.
    ListPending { drafts: Vec<EmailPendingRow> },
    /// User sent feedback — the draft should be regenerated.
    Modify {
        pending: EmailPendingRow,
        feedback: String,
    },
}

/// Handles email approval commands on notify channels.
pub struct EmailApprovalHandler {
    db: Database,
    /// Reverse map: (notify_channel, notify_chat_id) → list of email account names
    reverse_notify: HashMap<(String, String), Vec<String>>,
}

impl EmailApprovalHandler {
    /// Build the handler from the email notify routes table.
    ///
    /// `email_notify_routes` maps `"email:<account>"` → `(notify_channel, notify_chat_id)`.
    pub fn new(db: Database, email_notify_routes: &HashMap<String, (String, String)>) -> Self {
        let mut reverse_notify: HashMap<(String, String), Vec<String>> = HashMap::new();
        for (email_key, (ch, cid)) in email_notify_routes {
            // email_key is like "email:lavoro" — extract account name
            let account = email_key
                .strip_prefix("email:")
                .unwrap_or(email_key)
                .to_string();
            reverse_notify
                .entry((ch.clone(), cid.clone()))
                .or_default()
                .push(account);
        }

        Self { db, reverse_notify }
    }

    /// Check if an inbound message is an email approval command.
    ///
    /// Returns `NotApplicable` if the channel/chat_id pair isn't a notify target
    /// or if there are no pending drafts.
    pub async fn check_message(
        &self,
        channel: &str,
        chat_id: &str,
        content: &str,
    ) -> ApprovalAction {
        // 1. Is this channel+chat_id a notify target?
        let key = (channel.to_string(), chat_id.to_string());
        if !self.reverse_notify.contains_key(&key) {
            return ApprovalAction::NotApplicable;
        }

        // 2. Build notify_session_key and load pending drafts
        let notify_key = format!("{channel}:{chat_id}");
        let pending = match self.db.load_pending_for_notify(&notify_key).await {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(error = %e, "Failed to load pending emails");
                return ApprovalAction::NotApplicable;
            }
        };

        if pending.is_empty() {
            return ApprovalAction::NotApplicable;
        }

        // 3. Parse the command
        let trimmed = content.trim();
        let lower = trimmed.to_lowercase();
        let (command, index) = parse_command_and_index(&lower);

        match command {
            Command::Approve => match select_by_index(&pending, index) {
                Some(d) => ApprovalAction::Approve { pending: d.clone() },
                None => ApprovalAction::NotApplicable,
            },
            Command::Reject => match select_by_index(&pending, index) {
                Some(d) => ApprovalAction::Reject {
                    pending_id: d.id.clone(),
                },
                None => ApprovalAction::NotApplicable,
            },
            Command::List => ApprovalAction::ListPending { drafts: pending },
            Command::Modify => match select_by_index(&pending, index) {
                Some(d) => ApprovalAction::Modify {
                    pending: d.clone(),
                    feedback: trimmed.to_string(),
                },
                None => ApprovalAction::NotApplicable,
            },
        }
    }

    /// Format a draft notification for the notify channel.
    pub fn format_draft_notification(
        pending: &EmailPendingRow,
        index: usize,
        total: usize,
    ) -> String {
        let counter = if total > 1 {
            format!(" [{}/{}]", index, total)
        } else {
            String::new()
        };

        let subject = pending.subject.as_deref().unwrap_or("(nessun oggetto)");

        let draft = pending
            .draft_response
            .as_deref()
            .unwrap_or("(bozza non ancora generata)");

        format!(
            "📧 Email da approvare{counter}\n\
             Da: {from}\n\
             Oggetto: {subject}\n\
             \n\
             --- Bozza ---\n\
             {draft}\n\
             \n\
             ---\n\
             ✅ \"ok\" per inviare · ✏️ scrivi modifiche · ❌ \"rifiuta\" per scartare",
            from = pending.from_address,
        )
    }

    /// Build the context prompt for the agent to regenerate a draft.
    pub fn build_modification_context(pending: &EmailPendingRow, feedback: &str) -> String {
        let subject = pending.subject.as_deref().unwrap_or("(nessun oggetto)");
        let body = pending.body_preview.as_deref().unwrap_or("");
        let draft = pending
            .draft_response
            .as_deref()
            .unwrap_or("(nessuna bozza)");

        format!(
            "Stai gestendo un'email in modalità Assisted. L'utente ha chiesto modifiche alla bozza.\n\
             \n\
             EMAIL ORIGINALE:\n\
             Da: {from}\n\
             Oggetto: {subject}\n\
             {body}\n\
             \n\
             BOZZA ATTUALE:\n\
             {draft}\n\
             \n\
             RICHIESTA DELL'UTENTE:\n\
             {feedback}\n\
             \n\
             Riscrivi SOLO la bozza di risposta email, senza commenti o spiegazioni aggiuntive.",
            from = pending.from_address,
        )
    }
}

// ---------------------------------------------------------------------------
// Command parsing
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub(crate) enum Command {
    Approve,
    Reject,
    List,
    Modify,
}

/// Parse command keyword + optional numeric index from the message.
///
/// Examples: "ok" → (Approve, None), "ok 2" → (Approve, Some(1)), "rifiuta 3" → (Reject, Some(2))
/// The index is 0-based internally (user types 1-based).
pub(crate) fn parse_command_and_index(lower: &str) -> (Command, Option<usize>) {
    let parts: Vec<&str> = lower.split_whitespace().collect();
    if parts.is_empty() {
        return (Command::Modify, None);
    }

    let keyword = parts[0];
    let index = parts
        .get(1)
        .and_then(|s| s.parse::<usize>().ok())
        .map(|n| n.saturating_sub(1)); // Convert 1-based to 0-based

    let approve_keywords = ["ok", "invia", "send", "sì", "si", "yes", "manda", "approva"];
    let reject_keywords = ["rifiuta", "reject", "no", "scarta", "annulla"];
    let list_keywords = ["lista", "pending", "mostra", "show"];

    if approve_keywords.contains(&keyword) {
        (Command::Approve, index)
    } else if reject_keywords.contains(&keyword) {
        (Command::Reject, index)
    } else if list_keywords.contains(&keyword) {
        (Command::List, None)
    } else {
        (Command::Modify, None)
    }
}

/// Select a pending draft by 0-based index, defaulting to the first one (FIFO).
fn select_by_index(pending: &[EmailPendingRow], index: Option<usize>) -> Option<&EmailPendingRow> {
    match index {
        Some(i) => pending.get(i),
        None => pending.first(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_approve() {
        assert_eq!(parse_command_and_index("ok"), (Command::Approve, None));
        assert_eq!(parse_command_and_index("ok 2"), (Command::Approve, Some(1)));
        assert_eq!(parse_command_and_index("invia"), (Command::Approve, None));
        assert_eq!(parse_command_and_index("send"), (Command::Approve, None));
        assert_eq!(parse_command_and_index("sì"), (Command::Approve, None));
        assert_eq!(parse_command_and_index("yes"), (Command::Approve, None));
        assert_eq!(parse_command_and_index("manda"), (Command::Approve, None));
        assert_eq!(parse_command_and_index("approva"), (Command::Approve, None));
    }

    #[test]
    fn test_parse_reject() {
        assert_eq!(parse_command_and_index("rifiuta"), (Command::Reject, None));
        assert_eq!(
            parse_command_and_index("rifiuta 1"),
            (Command::Reject, Some(0))
        );
        assert_eq!(parse_command_and_index("reject"), (Command::Reject, None));
        assert_eq!(parse_command_and_index("no"), (Command::Reject, None));
        assert_eq!(parse_command_and_index("scarta"), (Command::Reject, None));
        assert_eq!(parse_command_and_index("annulla"), (Command::Reject, None));
    }

    #[test]
    fn test_parse_list() {
        assert_eq!(parse_command_and_index("lista"), (Command::List, None));
        assert_eq!(parse_command_and_index("pending"), (Command::List, None));
        assert_eq!(parse_command_and_index("mostra"), (Command::List, None));
        assert_eq!(parse_command_and_index("show"), (Command::List, None));
    }

    #[test]
    fn test_parse_modify() {
        assert_eq!(
            parse_command_and_index("sii più formale"),
            (Command::Modify, None)
        );
        assert_eq!(
            parse_command_and_index("rispondi in inglese"),
            (Command::Modify, None)
        );
        assert_eq!(parse_command_and_index(""), (Command::Modify, None));
    }

    #[test]
    fn test_format_draft_notification_single() {
        let row = EmailPendingRow {
            id: "abc".into(),
            account_name: "lavoro".into(),
            from_address: "mario@example.com".into(),
            subject: Some("Re: Progetto Q4".into()),
            body_preview: Some("Ciao...".into()),
            message_id: None,
            draft_response: Some("Grazie per la mail.".into()),
            status: "pending".into(),
            notify_session_key: None,
            created_at: "2024-01-01".into(),
            updated_at: None,
        };
        let formatted = EmailApprovalHandler::format_draft_notification(&row, 1, 1);
        assert!(formatted.contains("📧 Email da approvare"));
        assert!(!formatted.contains("[1/1]")); // No counter for single
        assert!(formatted.contains("mario@example.com"));
        assert!(formatted.contains("Re: Progetto Q4"));
        assert!(formatted.contains("Grazie per la mail."));
    }

    #[test]
    fn test_format_draft_notification_multiple() {
        let row = EmailPendingRow {
            id: "abc".into(),
            account_name: "lavoro".into(),
            from_address: "mario@example.com".into(),
            subject: Some("Re: Budget".into()),
            body_preview: None,
            message_id: None,
            draft_response: Some("Ok.".into()),
            status: "pending".into(),
            notify_session_key: None,
            created_at: "2024-01-01".into(),
            updated_at: None,
        };
        let formatted = EmailApprovalHandler::format_draft_notification(&row, 1, 3);
        assert!(formatted.contains("[1/3]"));
    }

    #[test]
    fn test_build_modification_context() {
        let row = EmailPendingRow {
            id: "abc".into(),
            account_name: "lavoro".into(),
            from_address: "mario@example.com".into(),
            subject: Some("Re: Meeting".into()),
            body_preview: Some("Quando ci vediamo?".into()),
            message_id: None,
            draft_response: Some("Domani alle 10.".into()),
            status: "pending".into(),
            notify_session_key: None,
            created_at: "2024-01-01".into(),
            updated_at: None,
        };

        let ctx = EmailApprovalHandler::build_modification_context(&row, "sii più formale");
        assert!(ctx.contains("mario@example.com"));
        assert!(ctx.contains("Re: Meeting"));
        assert!(ctx.contains("Quando ci vediamo?"));
        assert!(ctx.contains("Domani alle 10."));
        assert!(ctx.contains("sii più formale"));
        assert!(ctx.contains("Riscrivi SOLO"));
    }
}
