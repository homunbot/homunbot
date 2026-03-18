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
pub async fn build_contact_context(
    db: &Database,
    channel: &str,
    sender_id: &str,
) -> Result<Option<String>> {
    let contact = db.find_contact_by_identity(channel, sender_id).await?;
    let contact = match contact {
        Some(c) => c,
        None => return Ok(None),
    };

    let mut lines = Vec::new();
    lines.push(format!("[Contact: {}]", contact.name));

    if let Some(nick) = &contact.nickname {
        lines.push(format!("Nickname: {nick}"));
    }
    if !contact.bio.is_empty() {
        lines.push(format!("Bio: {}", contact.bio));
    }

    // Relationships
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

    // Upcoming events (next 14 days)
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

    Ok(Some(lines.join("\n")))
}
