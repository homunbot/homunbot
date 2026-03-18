//! Static capability declarations for each channel.
//!
//! Each channel declares what it supports (attachments, threads, proactive send, etc.)
//! so the agent loop can adapt behavior and the LLM knows what each channel can do.

/// Static capability declaration for a messaging channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelCapabilities {
    /// Can receive text messages from users.
    pub inbound_text: bool,
    /// Can receive file/media attachments from users.
    pub inbound_attachments: bool,
    /// Can send text messages to users.
    pub outbound_text: bool,
    /// Can send file/media attachments to users.
    pub outbound_attachments: bool,
    /// Can send messages without a prior inbound trigger (notifications, briefings).
    pub proactive_send: bool,
    /// Supports group/channel/space conversations.
    pub group_scope: bool,
    /// Supports private DM conversations.
    pub dm_scope: bool,
    /// Supports thread/topic/reply binding.
    pub thread_scope: bool,
    /// Can filter messages by @mention in groups.
    pub mention_policy: bool,
    /// Can show typing indicator while processing.
    pub typing_state: bool,
    /// Supports rich text formatting (markdown, HTML, mrkdwn).
    pub markdown_support: bool,
}

impl ChannelCapabilities {
    /// Compact human-readable summary of enabled capabilities.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.inbound_text {
            parts.push("text in");
        }
        if self.outbound_text {
            parts.push("text out");
        }
        if self.inbound_attachments {
            parts.push("attachments in");
        }
        if self.outbound_attachments {
            parts.push("attachments out");
        }
        if self.proactive_send {
            parts.push("proactive");
        }
        if self.group_scope {
            parts.push("groups");
        }
        if self.dm_scope {
            parts.push("DM");
        }
        if self.thread_scope {
            parts.push("threads");
        }
        if self.typing_state {
            parts.push("typing");
        }
        if self.markdown_support {
            parts.push("markdown");
        }
        if parts.is_empty() {
            return "none".to_string();
        }
        parts.join(", ")
    }
}

/// Returns the static capabilities for a channel by name.
///
/// Unknown channels get a safe default (text only, no extras).
pub fn capabilities_for(channel_name: &str) -> ChannelCapabilities {
    // Strip email account prefix (e.g. "email:lavoro" → "email")
    let base_name = channel_name.split(':').next().unwrap_or(channel_name);

    match base_name {
        "cli" => ChannelCapabilities {
            inbound_text: true,
            inbound_attachments: false,
            outbound_text: true,
            outbound_attachments: false,
            proactive_send: false,
            group_scope: false,
            dm_scope: true,
            thread_scope: false,
            mention_policy: false,
            typing_state: false,
            markdown_support: false,
        },
        "telegram" => ChannelCapabilities {
            inbound_text: true,
            inbound_attachments: true,
            outbound_text: true,
            outbound_attachments: true,
            proactive_send: true,
            group_scope: true,
            dm_scope: true,
            thread_scope: false,
            mention_policy: true,
            typing_state: true,
            markdown_support: true,
        },
        "discord" => ChannelCapabilities {
            inbound_text: true,
            inbound_attachments: true,
            outbound_text: true,
            outbound_attachments: true,
            proactive_send: true,
            group_scope: true,
            dm_scope: true,
            thread_scope: true,
            mention_policy: true,
            typing_state: true,
            markdown_support: true,
        },
        "slack" => ChannelCapabilities {
            inbound_text: true,
            inbound_attachments: false,
            outbound_text: true,
            outbound_attachments: false,
            proactive_send: true,
            group_scope: true,
            dm_scope: true,
            thread_scope: true,
            mention_policy: true,
            typing_state: false,
            markdown_support: true,
        },
        "whatsapp" => ChannelCapabilities {
            inbound_text: true,
            inbound_attachments: true,
            outbound_text: true,
            outbound_attachments: true,
            proactive_send: true,
            group_scope: true,
            dm_scope: true,
            thread_scope: false,
            mention_policy: true,
            typing_state: false,
            markdown_support: false,
        },
        "email" => ChannelCapabilities {
            inbound_text: true,
            inbound_attachments: true,
            outbound_text: true,
            outbound_attachments: true,
            proactive_send: true,
            group_scope: false,
            dm_scope: true,
            thread_scope: true,
            mention_policy: false,
            typing_state: false,
            markdown_support: true,
        },
        "web" => ChannelCapabilities {
            inbound_text: true,
            inbound_attachments: true,
            outbound_text: true,
            outbound_attachments: false,
            proactive_send: true,
            group_scope: false,
            dm_scope: true,
            thread_scope: false,
            mention_policy: false,
            typing_state: true,
            markdown_support: true,
        },
        // Unknown channel: safe default — text only
        _ => ChannelCapabilities {
            inbound_text: true,
            inbound_attachments: false,
            outbound_text: true,
            outbound_attachments: false,
            proactive_send: false,
            group_scope: false,
            dm_scope: false,
            thread_scope: false,
            mention_policy: false,
            typing_state: false,
            markdown_support: false,
        },
    }
}

/// Builds a system prompt block describing capabilities for active channels.
pub fn build_capabilities_prompt(channels: &[&str]) -> String {
    if channels.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n### Channel Capabilities\n");
    out.push_str("Each channel supports different features. Adapt your messages accordingly:\n");
    for &name in channels {
        let caps = capabilities_for(name);
        out.push_str(&format!("- **{name}**: {}\n", caps.summary()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telegram_capabilities() {
        let caps = capabilities_for("telegram");
        assert!(caps.inbound_attachments);
        assert!(caps.proactive_send);
        assert!(caps.typing_state);
        assert!(caps.markdown_support);
        assert!(caps.group_scope);
        assert!(!caps.thread_scope);
    }

    #[test]
    fn test_cli_capabilities() {
        let caps = capabilities_for("cli");
        assert!(caps.inbound_text);
        assert!(caps.outbound_text);
        assert!(caps.dm_scope);
        assert!(!caps.inbound_attachments);
        assert!(!caps.proactive_send);
        assert!(!caps.group_scope);
        assert!(!caps.markdown_support);
        assert!(!caps.typing_state);
    }

    #[test]
    fn test_email_account_prefix_stripped() {
        let caps = capabilities_for("email:lavoro");
        assert_eq!(caps, capabilities_for("email"));
        assert!(caps.thread_scope);
        assert!(caps.proactive_send);
        assert!(!caps.group_scope);
    }

    #[test]
    fn test_unknown_channel_defaults() {
        let caps = capabilities_for("signal");
        assert!(caps.inbound_text);
        assert!(caps.outbound_text);
        assert!(!caps.proactive_send);
        assert!(!caps.inbound_attachments);
        assert!(!caps.markdown_support);
    }

    #[test]
    fn test_summary_format() {
        let caps = capabilities_for("cli");
        let summary = caps.summary();
        assert!(summary.contains("text in"));
        assert!(summary.contains("text out"));
        assert!(summary.contains("DM"));
        assert!(!summary.contains("proactive"));
        assert!(!summary.contains("markdown"));
    }

    #[test]
    fn test_build_capabilities_prompt() {
        let prompt = build_capabilities_prompt(&["telegram", "cli"]);
        assert!(prompt.contains("### Channel Capabilities"));
        assert!(prompt.contains("**telegram**"));
        assert!(prompt.contains("**cli**"));
        assert!(prompt.contains("proactive"));
    }

    #[test]
    fn test_build_capabilities_prompt_empty() {
        let prompt = build_capabilities_prompt(&[]);
        assert!(prompt.is_empty());
    }
}
