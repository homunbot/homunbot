//! LLM tool for managing the personal contact book.
//!
//! 10 actions: search, resolve, get, create, update,
//! add_identity, add_relationship, add_event, upcoming, send.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::RwLock;

use super::registry::{Tool, ToolContext, ToolResult};
use crate::config::Config;
use crate::contacts::db::ContactUpdate;
use crate::storage::Database;

pub struct ContactsTool {
    db: Database,
    config: Arc<RwLock<Config>>,
}

impl ContactsTool {
    pub fn new(db: Database, config: Arc<RwLock<Config>>) -> Self {
        Self { db, config }
    }
}

#[async_trait]
impl Tool for ContactsTool {
    fn name(&self) -> &str {
        "contacts"
    }

    fn description(&self) -> &str {
        "Personal contact book. IMPORTANT: when the user asks to send a message to someone, \
         use action='send' with their name to resolve the preferred channel and chat_id, \
         then use send_message with the returned channel and chat_id. \
         Other actions: search, resolve, get, create, update, add_identity, add_relationship, \
         add_event, upcoming."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["search", "resolve", "get", "create", "update",
                             "add_identity", "add_relationship", "add_event", "upcoming", "send"],
                    "description": "Action to perform"
                },
                "query": {
                    "type": "string",
                    "description": "Search query or natural language description (for search/resolve/send)"
                },
                "contact_id": {
                    "type": "integer",
                    "description": "Contact ID (for get/update/add_identity/add_relationship/add_event/send)"
                },
                "name": { "type": "string", "description": "Contact name (for create)" },
                "nickname": { "type": "string" },
                "bio": { "type": "string" },
                "notes": { "type": "string" },
                "birthday": { "type": "string", "description": "ISO date YYYY-MM-DD or MM-DD" },
                "nameday": { "type": "string" },
                "preferred_channel": { "type": "string", "description": "telegram, whatsapp, discord, etc." },
                "response_mode": {
                    "type": "string",
                    "enum": ["automatic", "assisted", "on_demand", "silent"]
                },
                "tone_of_voice": {
                    "type": "string",
                    "description": "How to talk to this person (e.g. formal, informal, friendly, professional, technical)"
                },
                "tags": { "type": "string", "description": "JSON array of tags" },
                "channel": { "type": "string", "description": "Channel type (for add_identity)" },
                "identifier": { "type": "string", "description": "Channel identifier (for add_identity)" },
                "label": { "type": "string", "description": "Label for identity or event" },
                "to_contact_id": { "type": "integer", "description": "Target contact (for add_relationship)" },
                "relationship_type": { "type": "string", "description": "e.g. madre, padre, collega" },
                "bidirectional": { "type": "boolean", "description": "Auto-create reverse relationship" },
                "reverse_type": { "type": "string", "description": "Reverse relationship type" },
                "event_type": {
                    "type": "string",
                    "enum": ["birthday", "nameday", "anniversary", "custom"]
                },
                "date": { "type": "string", "description": "Event date (YYYY-MM-DD or MM-DD)" },
                "recurrence": { "type": "string", "enum": ["yearly", "once", "monthly"] },
                "auto_greet": { "type": "boolean", "description": "Send automated greeting" },
                "days": { "type": "integer", "description": "Days ahead for upcoming events (default 7)" },
                "message": { "type": "string", "description": "Message content for send action" }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let action = args["action"].as_str().unwrap_or("");

        match action {
            "search" => self.do_search(&args).await,
            "resolve" => self.do_resolve(&args).await,
            "get" => self.do_get(&args).await,
            "create" => self.do_create(&args).await,
            "update" => self.do_update(&args).await,
            "add_identity" => self.do_add_identity(&args).await,
            "add_relationship" => self.do_add_relationship(&args).await,
            "add_event" => self.do_add_event(&args).await,
            "upcoming" => self.do_upcoming(&args).await,
            "send" => self.do_send(&args).await,
            _ => Ok(ToolResult {
                output: format!("Unknown action: {action}. Valid: search, resolve, get, create, update, add_identity, add_relationship, add_event, upcoming, send"),
                is_error: true,
            }),
        }
    }
}

impl ContactsTool {
    async fn do_search(&self, args: &Value) -> Result<ToolResult> {
        let query = args["query"].as_str();
        let contacts = self.db.list_contacts(query).await?;
        let output = if contacts.is_empty() {
            "No contacts found.".to_string()
        } else {
            contacts
                .iter()
                .map(|c| {
                    format!(
                        "#{} {} {} [{}] mode={}",
                        c.id,
                        c.name,
                        c.nickname
                            .as_deref()
                            .map(|n| format!("({n})"))
                            .unwrap_or_default(),
                        c.preferred_channel.as_deref().unwrap_or("?"),
                        c.response_mode,
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        Ok(ToolResult {
            output,
            is_error: false,
        })
    }

    async fn do_resolve(&self, args: &Value) -> Result<ToolResult> {
        let query = args["query"].as_str().unwrap_or("");
        if query.is_empty() {
            return Ok(ToolResult {
                output: "Missing 'query' for resolve".to_string(),
                is_error: true,
            });
        }
        let config = self.config.read().await.clone();
        match crate::contacts::resolver::resolve_contact(&self.db, &config, query).await? {
            Some(result) => Ok(ToolResult {
                output: format!(
                    "Resolved: #{} {} (confidence: {:.0}%)\nPath: {}",
                    result.contact.id,
                    result.contact.name,
                    result.confidence * 100.0,
                    result.resolution_path,
                ),
                is_error: false,
            }),
            None => Ok(ToolResult {
                output: format!("Could not resolve '{query}' to any contact."),
                is_error: false,
            }),
        }
    }

    async fn do_get(&self, args: &Value) -> Result<ToolResult> {
        let id = args["contact_id"].as_i64().unwrap_or(0);
        if id == 0 {
            return Ok(ToolResult {
                output: "Missing contact_id".into(),
                is_error: true,
            });
        }
        let contact = self.db.load_contact(id).await?;
        match contact {
            Some(c) => {
                let identities = self.db.list_contact_identities(id).await?;
                let relationships = self.db.list_contact_relationships(id).await?;
                let events = self.db.list_contact_events(id).await?;
                let tone = if c.tone_of_voice.is_empty() {
                    "-".to_string()
                } else {
                    c.tone_of_voice.clone()
                };
                let output = format!(
                    "Contact #{}: {}\nNickname: {}\nBio: {}\nNotes: {}\nBirthday: {}\n\
                     Channel: {}\nMode: {}\nTone: {}\nTags: {}\n\
                     Identities: {}\nRelationships: {}\nEvents: {}",
                    c.id,
                    c.name,
                    c.nickname.as_deref().unwrap_or("-"),
                    c.bio,
                    c.notes,
                    c.birthday.as_deref().unwrap_or("-"),
                    c.preferred_channel.as_deref().unwrap_or("-"),
                    c.response_mode,
                    tone,
                    c.tags,
                    format_identities(&identities),
                    format_relationships(&relationships),
                    format_events(&events),
                );
                Ok(ToolResult {
                    output,
                    is_error: false,
                })
            }
            None => Ok(ToolResult {
                output: format!("Contact #{id} not found"),
                is_error: true,
            }),
        }
    }

    async fn do_create(&self, args: &Value) -> Result<ToolResult> {
        let name = args["name"].as_str().unwrap_or("");
        if name.is_empty() {
            return Ok(ToolResult {
                output: "Missing 'name' for create".into(),
                is_error: true,
            });
        }
        let id = self
            .db
            .insert_contact(
                name,
                args["nickname"].as_str(),
                args["bio"].as_str(),
                args["notes"].as_str(),
                args["birthday"].as_str(),
                args["nameday"].as_str(),
                args["preferred_channel"].as_str(),
                args["response_mode"].as_str(),
                args["tags"].as_str(),
                args["tone_of_voice"].as_str(),
            )
            .await?;
        Ok(ToolResult {
            output: format!("Created contact #{id}: {name}"),
            is_error: false,
        })
    }

    async fn do_update(&self, args: &Value) -> Result<ToolResult> {
        let id = args["contact_id"].as_i64().unwrap_or(0);
        if id == 0 {
            return Ok(ToolResult {
                output: "Missing contact_id".into(),
                is_error: true,
            });
        }
        let upd = ContactUpdate {
            name: args["name"].as_str().map(|s| s.to_string()),
            nickname: args["nickname"].as_str().map(|s| s.to_string()),
            bio: args["bio"].as_str().map(|s| s.to_string()),
            notes: args["notes"].as_str().map(|s| s.to_string()),
            birthday: args["birthday"].as_str().map(|s| s.to_string()),
            nameday: args["nameday"].as_str().map(|s| s.to_string()),
            preferred_channel: args["preferred_channel"].as_str().map(|s| s.to_string()),
            response_mode: args["response_mode"].as_str().map(|s| s.to_string()),
            tone_of_voice: args["tone_of_voice"].as_str().map(|s| s.to_string()),
            tags: args["tags"].as_str().map(|s| s.to_string()),
            avatar_url: args["avatar_url"].as_str().map(|s| s.to_string()),
            persona_override: args["persona_override"].as_str().map(|s| s.to_string()),
            persona_instructions: args["persona_instructions"].as_str().map(|s| s.to_string()),
            agent_override: args["agent_override"].as_str().map(|s| s.to_string()),
            profile_id: args["profile_id"].as_i64(),
        };
        let updated = self.db.update_contact(id, &upd).await?;
        Ok(ToolResult {
            output: if updated {
                format!("Updated contact #{id}")
            } else {
                format!("Contact #{id} not found or no fields to update")
            },
            is_error: !updated,
        })
    }

    async fn do_add_identity(&self, args: &Value) -> Result<ToolResult> {
        let contact_id = args["contact_id"].as_i64().unwrap_or(0);
        let channel = args["channel"].as_str().unwrap_or("");
        let identifier = args["identifier"].as_str().unwrap_or("");
        if contact_id == 0 || channel.is_empty() || identifier.is_empty() {
            return Ok(ToolResult {
                output: "Missing contact_id, channel, or identifier".into(),
                is_error: true,
            });
        }
        let id = self
            .db
            .insert_contact_identity(contact_id, channel, identifier, args["label"].as_str())
            .await?;
        Ok(ToolResult {
            output: format!("Added identity #{id}: {channel}:{identifier}"),
            is_error: false,
        })
    }

    async fn do_add_relationship(&self, args: &Value) -> Result<ToolResult> {
        let from_id = args["contact_id"].as_i64().unwrap_or(0);
        let to_id = args["to_contact_id"].as_i64().unwrap_or(0);
        let rel_type = args["relationship_type"].as_str().unwrap_or("");
        if from_id == 0 || to_id == 0 || rel_type.is_empty() {
            return Ok(ToolResult {
                output: "Missing contact_id, to_contact_id, or relationship_type".into(),
                is_error: true,
            });
        }
        let bidir = args["bidirectional"].as_bool().unwrap_or(false);
        let id = self
            .db
            .insert_contact_relationship(
                from_id,
                to_id,
                rel_type,
                bidir,
                args["reverse_type"].as_str(),
                args["notes"].as_str(),
            )
            .await?;
        Ok(ToolResult {
            output: format!("Added relationship #{id}: {rel_type}"),
            is_error: false,
        })
    }

    async fn do_add_event(&self, args: &Value) -> Result<ToolResult> {
        let contact_id = args["contact_id"].as_i64().unwrap_or(0);
        let event_type = args["event_type"].as_str().unwrap_or("");
        let date = args["date"].as_str().unwrap_or("");
        if contact_id == 0 || event_type.is_empty() || date.is_empty() {
            return Ok(ToolResult {
                output: "Missing contact_id, event_type, or date".into(),
                is_error: true,
            });
        }
        let id = self
            .db
            .insert_contact_event(
                contact_id,
                event_type,
                date,
                args["recurrence"].as_str(),
                args["label"].as_str(),
                args["auto_greet"].as_bool().unwrap_or(false),
                args["notify_days_before"].as_i64().map(|n| n as i32),
            )
            .await?;
        Ok(ToolResult {
            output: format!("Added event #{id}: {event_type} on {date}"),
            is_error: false,
        })
    }

    async fn do_upcoming(&self, args: &Value) -> Result<ToolResult> {
        let days = args["days"].as_i64().unwrap_or(7) as i32;
        let events = self.db.load_upcoming_contact_events(days).await?;
        if events.is_empty() {
            return Ok(ToolResult {
                output: format!("No events in the next {days} days."),
                is_error: false,
            });
        }
        let lines: Vec<String> = events
            .iter()
            .map(|ue| {
                format!(
                    "- {} {} ({}) [{}]",
                    ue.event.date,
                    ue.contact_name,
                    ue.event.event_type,
                    ue.event.label.as_deref().unwrap_or(""),
                )
            })
            .collect();
        Ok(ToolResult {
            output: format!("Upcoming events (next {days} days):\n{}", lines.join("\n")),
            is_error: false,
        })
    }

    async fn do_send(&self, args: &Value) -> Result<ToolResult> {
        let message = args["message"].as_str().unwrap_or("");
        if message.is_empty() {
            return Ok(ToolResult {
                output: "Missing 'message'".into(),
                is_error: true,
            });
        }

        // Resolve the contact (by ID or query)
        let contact = if let Some(id) = args["contact_id"].as_i64() {
            self.db.load_contact(id).await?
        } else if let Some(query) = args["query"].as_str() {
            let config = self.config.read().await.clone();
            crate::contacts::resolver::resolve_contact(&self.db, &config, query)
                .await?
                .map(|r| r.contact)
        } else {
            return Ok(ToolResult {
                output: "Provide contact_id or query to identify the recipient".into(),
                is_error: true,
            });
        };

        match contact {
            Some(c) => {
                let channel = c.preferred_channel.as_deref().unwrap_or("unknown");
                // Find the identity for the preferred channel
                let identities = self
                    .db
                    .list_contact_identities(c.id)
                    .await
                    .unwrap_or_default();
                let identity = identities.iter().find(|i| i.channel == channel);

                if let Some(ident) = identity {
                    let chat_id = if channel == "whatsapp" && !ident.identifier.contains('@') {
                        let num = ident.identifier.replace(['+', ' ', '-'], "");
                        format!("{num}@s.whatsapp.net")
                    } else {
                        ident.identifier.clone()
                    };
                    Ok(ToolResult {
                        output: format!(
                            "Ready to send to {name} via {channel}. \
                             Use send_message with channel=\"{channel}\" chat_id=\"{chat_id}\" \
                             and your message content.",
                            name = c.name,
                        ),
                        is_error: false,
                    })
                } else {
                    let available = identities
                        .iter()
                        .map(|i| format!("{}:{}", i.channel, i.identifier))
                        .collect::<Vec<_>>()
                        .join(", ");
                    Ok(ToolResult {
                        output: format!(
                            "Contact {} has no {channel} identity. Available: {avail}. \
                             Set preferred_channel or add a {channel} identity first.",
                            c.name,
                            avail = if available.is_empty() {
                                "none".into()
                            } else {
                                available
                            },
                        ),
                        is_error: true,
                    })
                }
            }
            None => Ok(ToolResult {
                output: "Contact not found. Create it first or provide a valid contact_id.".into(),
                is_error: true,
            }),
        }
    }
}

fn format_identities(ids: &[crate::contacts::ContactIdentity]) -> String {
    if ids.is_empty() {
        return "none".to_string();
    }
    ids.iter()
        .map(|i| format!("{}:{}", i.channel, i.identifier))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_relationships(rels: &[crate::contacts::ContactRelationship]) -> String {
    if rels.is_empty() {
        return "none".to_string();
    }
    rels.iter()
        .map(|r| format!("{} →#{}", r.relationship_type, r.to_contact_id))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_events(events: &[crate::contacts::ContactEvent]) -> String {
    if events.is_empty() {
        return "none".to_string();
    }
    events
        .iter()
        .map(|e| format!("{}: {} ({})", e.event_type, e.date, e.recurrence))
        .collect::<Vec<_>>()
        .join(", ")
}
