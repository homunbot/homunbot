//! Contact context builder for system prompt injection.
//!
//! When a known contact sends a message, builds a structured text block
//! with their profile, relationships, and upcoming events for the agent.

use anyhow::Result;

use crate::storage::Database;

/// Build a contact context section for the system prompt.
///
/// Returns `None` if the sender is not found in the contact book.
/// Returns a formatted text block like:
/// ```text
/// [Contact: Marco Rossi]
/// Bio: CTO di AcmeCorp
/// Relationships: married to Laura Bianchi
/// Preferred channel: telegram
/// Response mode: automatic
/// Upcoming: birthday in 3 days (March 21)
/// ```
/// Convenience wrapper: looks up the contact by identity, then delegates to
/// [`build_contact_context_from`]. Returns `None` if the sender is unknown.
pub async fn build_contact_context(
    db: &Database,
    channel: &str,
    sender_id: &str,
) -> Result<Option<String>> {
    let contact = db.find_contact_by_identity(channel, sender_id).await?;
    match contact {
        Some(c) => Ok(Some(build_contact_context_from(db, &c).await?)),
        None => Ok(None),
    }
}

/// Build contact context from a pre-resolved `Contact` (avoids duplicate DB lookup).
pub async fn build_contact_context_from(
    db: &Database,
    contact: &crate::contacts::Contact,
) -> Result<String> {
    let mut lines = Vec::new();
    lines.push(format!("[Contact: {}]", contact.name));

    if let Some(nick) = &contact.nickname {
        lines.push(format!("Nickname: {nick}"));
    }
    if !contact.bio.is_empty() {
        lines.push(format!("Bio: {}", contact.bio));
    }

    let relationships = db.list_contact_relationships(contact.id).await?;
    if !relationships.is_empty() {
        let mut rel_parts = Vec::new();
        for r in &relationships {
            let other_id = if r.from_contact_id == contact.id {
                r.to_contact_id
            } else {
                r.from_contact_id
            };
            let other_name = db
                .load_contact(other_id)
                .await
                .ok()
                .flatten()
                .map(|c| c.name)
                .unwrap_or_else(|| format!("#{other_id}"));
            let rel_type = if r.from_contact_id == contact.id {
                &r.relationship_type
            } else {
                r.reverse_type.as_deref().unwrap_or(&r.relationship_type)
            };
            rel_parts.push(format!("{rel_type} of {other_name}"));
        }
        lines.push(format!("Relationships: {}", rel_parts.join(", ")));
    }

    if let Some(ch) = &contact.preferred_channel {
        lines.push(format!("Preferred channel: {ch}"));
    }
    lines.push(format!("Response mode: {}", contact.response_mode));

    if !contact.tone_of_voice.is_empty() {
        lines.push(format!("Tone of voice: {}", contact.tone_of_voice));
    }

    let events = db.list_contact_events(contact.id).await?;
    if !events.is_empty() {
        let event_strs: Vec<String> = events
            .iter()
            .map(|e| {
                let label = e.label.as_deref().unwrap_or(&e.event_type);
                format!("{label} ({}, {})", e.date, e.recurrence)
            })
            .collect();
        lines.push(format!("Events: {}", event_strs.join(", ")));
    }

    if contact.tags != "[]" && !contact.tags.is_empty() {
        lines.push(format!("Tags: {}", contact.tags));
    }
    if !contact.notes.is_empty() {
        lines.push(format!("Notes: {}", contact.notes));
    }

    Ok(lines.join("\n"))
}

/// Build a context hint for unknown senders (not in the contact book).
///
/// Returned as a prompt hint so the agent knows the sender is unrecognized
/// and can offer to create/associate a contact during conversation.
pub fn build_unknown_sender_context(channel: &str, sender_id: &str) -> String {
    format!(
        "[Unknown sender: {channel}:{sender_id}]\n\
         This person is not in your contact book. If you learn who they are \
         during the conversation, use the contacts tool to create a new contact \
         and add their {channel} identity ({sender_id})."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unknown_sender_context() {
        let ctx = build_unknown_sender_context("telegram", "12345");
        assert!(ctx.contains("[Unknown sender: telegram:12345]"));
        assert!(ctx.contains("not in your contact book"));
        assert!(ctx.contains("contacts tool"));
    }

    #[test]
    fn test_unknown_sender_context_whatsapp() {
        let ctx = build_unknown_sender_context("whatsapp", "+393331234567@s.whatsapp.net");
        assert!(ctx.contains("whatsapp"));
        assert!(ctx.contains("+393331234567@s.whatsapp.net"));
    }
}
