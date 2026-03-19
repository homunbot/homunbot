//! Contact Book — domain types.
//!
//! Personal CRM with multi-channel identities, social graph,
//! response policies, and relational events.

pub mod context;
pub mod db;
pub mod events;
pub mod resolver;

use serde::{Deserialize, Serialize};

// ── Response mode ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseMode {
    Automatic,
    Assisted,
    OnDemand,
    Silent,
}

impl ResponseMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Assisted => "assisted",
            Self::OnDemand => "on_demand",
            Self::Silent => "silent",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "assisted" => Self::Assisted,
            "on_demand" => Self::OnDemand,
            "silent" => Self::Silent,
            _ => Self::Automatic,
        }
    }
}

// ── Domain types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Contact {
    pub id: i64,
    pub name: String,
    pub nickname: Option<String>,
    pub bio: String,
    pub notes: String,
    pub birthday: Option<String>,
    pub nameday: Option<String>,
    pub preferred_channel: Option<String>,
    pub response_mode: String,
    pub tone_of_voice: String,
    pub tags: String,
    pub avatar_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Per-contact persona override: bot, owner, company, custom. NULL = use channel default.
    pub persona_override: Option<String>,
    /// Custom persona instructions (used when persona_override = "custom").
    pub persona_instructions: String,
    /// Per-contact agent routing override (MAG-2). NULL = use channel default.
    pub agent_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ContactIdentity {
    pub id: i64,
    pub contact_id: i64,
    pub channel: String,
    pub identifier: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ContactRelationship {
    pub id: i64,
    pub from_contact_id: i64,
    pub to_contact_id: i64,
    pub relationship_type: String,
    pub bidirectional: i32,
    pub reverse_type: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ContactEvent {
    pub id: i64,
    pub contact_id: i64,
    pub event_type: String,
    pub date: String,
    pub recurrence: String,
    pub label: Option<String>,
    pub auto_greet: i32,
    pub greet_template: Option<String>,
    pub notify_days_before: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PendingResponse {
    pub id: i64,
    pub contact_id: Option<i64>,
    pub channel: String,
    pub chat_id: String,
    pub inbound_content: String,
    pub draft_response: Option<String>,
    pub status: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    /// Channel where the draft notification was sent for approval.
    pub notify_channel: Option<String>,
    /// Chat ID on the notify channel.
    pub notify_chat_id: Option<String>,
}

/// Upcoming event with contact name for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpcomingEvent {
    #[serde(flatten)]
    pub event: ContactEvent,
    pub contact_name: String,
}
