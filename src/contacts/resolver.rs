//! Contact resolver — NLP + graph traversal.
//!
//! Resolves natural language descriptions like "la mamma di Felicia"
//! to a specific Contact via direct name matching or LLM-assisted
//! relationship graph traversal.

use anyhow::{Context, Result};

use crate::config::Config;
use crate::provider::one_shot::{OneShotRequest, OneShotResponse};
use crate::storage::Database;

use super::Contact;

/// Result of a contact resolution.
#[derive(Debug)]
pub struct ResolveResult {
    pub contact: Contact,
    pub confidence: f32,
    pub resolution_path: String,
}

/// Relationship keywords that trigger LLM resolution.
const RELATIONSHIP_KEYWORDS: &[&str] = &[
    "madre", "padre", "mamma", "papà", "papa", "figlio", "figlia",
    "fratello", "sorella", "marito", "moglie", "partner", "collega",
    "capo", "amico", "amica", "zio", "zia", "nonno", "nonna",
    "cugino", "cugina", "nipote",
    // Prepositions that indicate relational queries
    " di ", " del ", " della ", " dello ", " dei ", " delle ",
    "mother", "father", "son", "daughter", "wife", "husband",
    "brother", "sister", "friend", "colleague", "boss",
];

/// Check if a query contains relationship language.
fn has_relationship_keywords(query: &str) -> bool {
    let lower = query.to_lowercase();
    RELATIONSHIP_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

/// Resolve a natural language description to a contact.
///
/// **Fast path**: direct name/nickname match (no LLM call).
/// **Slow path**: if the description contains relationship keywords,
/// uses `llm_one_shot()` to parse the intent against the full contact
/// + relationship graph.
pub async fn resolve_contact(
    db: &Database,
    config: &Config,
    description: &str,
) -> Result<Option<ResolveResult>> {
    // Fast path: direct name search
    if !has_relationship_keywords(description) {
        let matches = db.list_contacts(Some(description)).await?;
        if matches.len() == 1 {
            return Ok(Some(ResolveResult {
                resolution_path: format!("Direct match: {}", matches[0].name),
                confidence: 1.0,
                contact: matches[0].clone(),
            }));
        }
        if matches.is_empty() {
            return Ok(None);
        }
        // Multiple matches — fall through to LLM for disambiguation
    }

    // Slow path: LLM resolution with relationship graph
    let contacts = db.list_contacts(None).await?;
    if contacts.is_empty() {
        return Ok(None);
    }

    // Build contact list with relationships for LLM context
    let mut context_lines = Vec::new();
    for c in &contacts {
        let rels = db.list_contact_relationships(c.id).await.unwrap_or_default();
        let rel_str = if rels.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = rels
                .iter()
                .map(|r| {
                    let other_id = if r.from_contact_id == c.id {
                        r.to_contact_id
                    } else {
                        r.from_contact_id
                    };
                    let other_name = contacts
                        .iter()
                        .find(|cc| cc.id == other_id)
                        .map(|cc| cc.name.as_str())
                        .unwrap_or("?");
                    if r.from_contact_id == c.id {
                        format!("{} of {}", r.relationship_type, other_name)
                    } else {
                        r.reverse_type
                            .as_deref()
                            .map(|rt| format!("{} of {}", rt, other_name))
                            .unwrap_or_else(|| format!("related to {}", other_name))
                    }
                })
                .collect();
            format!(" | Relationships: {}", parts.join(", "))
        };
        context_lines.push(format!(
            "- ID:{} {} (nickname: {}){}",
            c.id,
            c.name,
            c.nickname.as_deref().unwrap_or("-"),
            rel_str,
        ));
    }

    let system = "You are a contact book resolver. Given a query and a list of contacts with \
        their relationships, find the matching contact. Return ONLY a JSON object: \
        {\"contact_id\": <number>, \"confidence\": <0.0-1.0>, \"path\": \"<explanation>\"}. \
        If no match found, return {\"contact_id\": null, \"confidence\": 0, \"path\": \"no match\"}.";

    let user = format!(
        "Query: \"{}\"\n\nContacts:\n{}",
        description,
        context_lines.join("\n")
    );

    let resp: OneShotResponse = crate::provider::one_shot::llm_one_shot(
        config,
        OneShotRequest {
            system_prompt: system.to_string(),
            user_message: user,
            temperature: 0.1,
            max_tokens: 256,
            timeout_secs: 15,
            ..Default::default()
        },
    )
    .await
    .context("LLM contact resolution failed")?;

    // Parse LLM response
    let content = resp.content.trim();
    // Extract JSON from possible markdown fences
    let json_str = content
        .strip_prefix("```json")
        .or_else(|| content.strip_prefix("```"))
        .and_then(|s| s.strip_suffix("```"))
        .unwrap_or(content)
        .trim();

    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
        let contact_id = parsed["contact_id"].as_i64();
        let confidence = parsed["confidence"].as_f64().unwrap_or(0.0) as f32;
        let path = parsed["path"].as_str().unwrap_or("").to_string();

        if let Some(cid) = contact_id {
            if confidence >= 0.5 {
                if let Some(contact) = contacts.into_iter().find(|c| c.id == cid) {
                    return Ok(Some(ResolveResult {
                        contact,
                        confidence,
                        resolution_path: path,
                    }));
                }
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relationship_keyword_detection() {
        assert!(has_relationship_keywords("la mamma di Felicia"));
        assert!(has_relationship_keywords("padre di Marco"));
        assert!(has_relationship_keywords("the mother of John"));
        assert!(!has_relationship_keywords("Marco Rossi"));
        assert!(!has_relationship_keywords("find someone"));
    }
}
