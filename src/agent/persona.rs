//! Persona resolution — determines how the agent presents itself.
//!
//! Priority chain: Contact.persona_override > Channel.persona > "bot" (global default).
//! Same pattern for tone_of_voice.

use crate::contacts::Contact;

/// Resolved persona for a specific conversation.
#[derive(Debug, Clone)]
pub struct ResolvedPersona {
    /// "bot", "owner", "company", or "custom".
    pub persona_type: String,
    /// Effective tone of voice (contact > channel > "").
    pub tone_of_voice: String,
    /// System prompt prefix injected before the identity section.
    pub prompt_prefix: String,
}

/// Resolve the effective persona for a conversation.
///
/// # Arguments
/// * `contact` — the resolved contact (if any)
/// * `channel_persona` — persona from channel config (e.g. "owner")
/// * `channel_tone` — default tone from channel config
/// * `user_name` — the owner's name (from USER.md or config) for "owner" persona
pub fn resolve_persona(
    contact: Option<&Contact>,
    channel_persona: &str,
    channel_tone: &str,
    user_name: &str,
) -> ResolvedPersona {
    // Priority chain for persona type
    let persona_type = contact
        .and_then(|c| c.persona_override.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if channel_persona.is_empty() {
                "bot"
            } else {
                channel_persona
            }
        })
        .to_string();

    // Priority chain for tone
    let tone_of_voice = contact
        .map(|c| c.tone_of_voice.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if channel_tone.is_empty() {
                ""
            } else {
                channel_tone
            }
        })
        .to_string();

    // Custom instructions from contact (for "custom" persona)
    let custom_instructions = contact
        .map(|c| c.persona_instructions.as_str())
        .unwrap_or("");

    // Build prompt prefix
    let prompt_prefix = build_prompt_prefix(&persona_type, user_name, custom_instructions);

    ResolvedPersona {
        persona_type,
        tone_of_voice,
        prompt_prefix,
    }
}

/// Build the system prompt prefix for a given persona type.
fn build_prompt_prefix(persona_type: &str, user_name: &str, custom_instructions: &str) -> String {
    match persona_type {
        "owner" => {
            let name_part = if user_name.is_empty() {
                "the owner".to_string()
            } else {
                user_name.to_string()
            };
            format!(
                "You are responding on behalf of {name_part}. \
                 Write in first person as if you ARE them, not their assistant. \
                 Do not mention that you are an AI or an assistant."
            )
        }
        "company" => {
            "You are representing the company. \
             Respond professionally using 'we' and 'our team'. \
             Maintain a corporate tone."
                .to_string()
        }
        "custom" if !custom_instructions.is_empty() => custom_instructions.to_string(),
        // "bot" or any unrecognized value — default behavior (SOUL.md identity)
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_contact(
        persona_override: Option<&str>,
        tone: &str,
        instructions: &str,
    ) -> Contact {
        Contact {
            id: 1,
            name: "Test".to_string(),
            nickname: None,
            bio: String::new(),
            notes: String::new(),
            birthday: None,
            nameday: None,
            preferred_channel: None,
            response_mode: "automatic".to_string(),
            tone_of_voice: tone.to_string(),
            tags: "[]".to_string(),
            avatar_url: None,
            created_at: String::new(),
            updated_at: String::new(),
            persona_override: persona_override.map(|s| s.to_string()),
            persona_instructions: instructions.to_string(),
            agent_override: None,
        }
    }

    #[test]
    fn default_persona_is_bot() {
        let p = resolve_persona(None, "", "", "");
        assert_eq!(p.persona_type, "bot");
        assert!(p.prompt_prefix.is_empty());
    }

    #[test]
    fn channel_persona_used_when_no_contact() {
        let p = resolve_persona(None, "owner", "", "Fabio");
        assert_eq!(p.persona_type, "owner");
        assert!(p.prompt_prefix.contains("Fabio"));
    }

    #[test]
    fn contact_overrides_channel() {
        let contact = make_contact(Some("company"), "", "");
        let p = resolve_persona(Some(&contact), "owner", "", "");
        assert_eq!(p.persona_type, "company");
    }

    #[test]
    fn tone_priority_contact_over_channel() {
        let contact = make_contact(None, "informal", "");
        let p = resolve_persona(Some(&contact), "bot", "formal", "");
        assert_eq!(p.tone_of_voice, "informal");
    }

    #[test]
    fn tone_falls_back_to_channel() {
        let contact = make_contact(None, "", "");
        let p = resolve_persona(Some(&contact), "bot", "formal", "");
        assert_eq!(p.tone_of_voice, "formal");
    }

    #[test]
    fn custom_persona_uses_instructions() {
        let contact = make_contact(Some("custom"), "", "Respond only in haiku.");
        let p = resolve_persona(Some(&contact), "bot", "", "");
        assert_eq!(p.persona_type, "custom");
        assert_eq!(p.prompt_prefix, "Respond only in haiku.");
    }
}
