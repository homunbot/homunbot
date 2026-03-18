pub mod capabilities;
mod cli;
pub mod health;
pub mod slack;
mod traits;

#[cfg(feature = "channel-telegram")]
pub mod telegram;

#[cfg(feature = "channel-discord")]
pub mod discord;

#[cfg(feature = "channel-whatsapp")]
pub mod whatsapp;

#[cfg(feature = "channel-email")]
pub mod email;

pub use cli::CliChannel;
pub use slack::SlackChannel;

#[cfg(feature = "channel-telegram")]
pub use telegram::TelegramChannel;

#[cfg(feature = "channel-discord")]
pub use discord::DiscordChannel;

#[cfg(feature = "channel-whatsapp")]
pub use whatsapp::WhatsAppChannel;

#[cfg(feature = "channel-email")]
pub use email::EmailChannel;

pub use capabilities::{capabilities_for, ChannelCapabilities};
pub use health::ChannelHealthTracker;
pub use traits::Channel;

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Health tracker lifecycle across multiple channels.
    #[test]
    fn health_multi_channel_lifecycle() {
        let tracker = ChannelHealthTracker::new();

        tracker.mark_started("telegram");
        tracker.mark_started("discord");
        tracker.mark_started("slack");

        for _ in 0..5 {
            tracker.record_message("telegram");
            tracker.record_message("discord");
        }
        tracker.record_message("slack");

        assert!(tracker.is_available("telegram"));
        assert!(tracker.is_available("discord"));
        assert!(tracker.is_available("slack"));

        // Simulate crash + restart
        tracker.mark_stopped("discord", Some("WebSocket closed"));
        assert!(!tracker.is_available("discord"));

        tracker.mark_started("discord");
        for _ in 0..5 {
            tracker.record_message("discord");
        }
        assert!(tracker.is_available("discord"));
        assert_eq!(tracker.snapshot("discord").unwrap().restart_count, 1);

        let snaps = tracker.snapshots();
        assert_eq!(snaps.len(), 3);
    }

    /// All known channels have consistent capabilities.
    #[test]
    fn capabilities_coverage_all_channels() {
        let channels = ["cli", "telegram", "discord", "slack", "whatsapp", "email", "web"];

        for name in &channels {
            let caps = capabilities_for(name);
            assert!(caps.inbound_text, "{name} must support inbound text");
            assert!(caps.outbound_text, "{name} must support outbound text");
        }

        // Chat channels support proactive
        for name in &["telegram", "discord", "slack", "whatsapp", "email", "web"] {
            assert!(
                capabilities_for(name).proactive_send,
                "{name} should support proactive"
            );
        }

        // CLI is interactive-only
        assert!(!capabilities_for("cli").proactive_send);

        // Thread-capable channels
        assert!(capabilities_for("discord").thread_scope);
        assert!(capabilities_for("slack").thread_scope);
        assert!(capabilities_for("email").thread_scope);
    }

    /// Health degradation and recovery via sliding window.
    #[test]
    fn health_degradation_recovery() {
        let tracker = ChannelHealthTracker::new();
        tracker.mark_started("slack");

        for _ in 0..4 {
            tracker.record_message("slack");
        }
        for _ in 0..6 {
            tracker.record_error("slack", "rate limited");
        }

        let snap = tracker.snapshot("slack").unwrap();
        assert!(snap.error_rate_recent > 0.5);
        assert!(tracker.is_available("slack")); // degraded != down

        // Recovery
        for _ in 0..20 {
            tracker.record_message("slack");
        }
        assert!(tracker.snapshot("slack").unwrap().error_rate_recent < 0.5);
    }

    /// Email account prefix normalization.
    #[test]
    fn email_prefix_normalization() {
        assert_eq!(capabilities_for("email"), capabilities_for("email:lavoro"));
        assert_eq!(capabilities_for("email"), capabilities_for("email:personal"));
    }

    /// Unknown channels get safe defaults.
    #[test]
    fn unknown_channel_safe_defaults() {
        let caps = capabilities_for("nonexistent");
        assert!(caps.inbound_text);
        assert!(caps.outbound_text);
        assert!(!caps.proactive_send);
        assert!(!caps.group_scope);
    }

    /// Slack config defaults validation.
    #[test]
    fn slack_config_defaults() {
        let config = crate::config::SlackConfig::default();
        assert!(!config.enabled);
        assert!(config.token.is_empty());
        assert!(config.app_token.is_empty());
        assert!(config.default_channel_id.is_empty());
        // mention_required defaults to true via serde(default = "default_true")
        // but Default::default() gives false — serde is what matters at runtime
    }

    /// Slack Socket Mode detection.
    #[test]
    fn slack_socket_mode_toggle() {
        let config = crate::config::SlackConfig::default();
        let ch = SlackChannel::new(config);
        assert!(!ch.has_socket_mode());

        let mut config = crate::config::SlackConfig::default();
        config.app_token = "xapp-1-A111-222-abc".to_string();
        let ch = SlackChannel::new(config);
        assert!(ch.has_socket_mode());
    }

    /// Proactive routing uses default_channel_id when set.
    #[test]
    fn proactive_routing_default_channel_id() {
        let mut ch = crate::config::ChannelsConfig::default();
        ch.slack.enabled = true;
        ch.slack.token = "xoxb-test".to_string();
        ch.slack.channel_id = "C_LISTEN".to_string();
        ch.slack.default_channel_id = "C_PROACTIVE".to_string();

        let targets = ch.active_channels_with_chat_ids();
        let slack = targets.iter().find(|(n, _)| n == "slack");
        assert_eq!(slack.unwrap().1, "C_PROACTIVE");

        // Fallback when default not set
        ch.slack.default_channel_id = String::new();
        let targets = ch.active_channels_with_chat_ids();
        let slack = targets.iter().find(|(n, _)| n == "slack");
        assert_eq!(slack.unwrap().1, "C_LISTEN");
    }
}
