//! Slack channel — polls conversations.history via Web API
//!
//! Implements the Channel trait for Slack integration using Web API polling.
//! Inspired by ZeroClaw's implementation.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::traits::Channel;
use crate::bus::{InboundMessage, MessageMetadata, OutboundMessage};
use crate::config::SlackConfig;

/// Slack channel implementation using Web API polling
pub struct SlackChannel {
    config: SlackConfig,
    client: Client,
}

impl SlackChannel {
    pub fn new(config: SlackConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        Self { config, client }
    }

    /// Check if a Slack user ID is allowed to interact
    fn is_user_allowed(&self, user_id: &str) -> bool {
        if self.config.allow_from.is_empty() {
            return false; // Empty list = deny everyone by default
        }
        self.config
            .allow_from
            .iter()
            .any(|u| u == "*" || u == user_id)
    }

    /// Get the bot's own user ID (to ignore own messages)
    async fn get_bot_user_id(&self) -> Result<String> {
        let resp = self
            .client
            .get("https://slack.com/api/auth.test")
            .bearer_auth(&self.config.token)
            .send()
            .await
            .context("Failed to call auth.test")?;

        let data: SlackResponse = resp
            .json()
            .await
            .context("Failed to parse auth.test response")?;

        if !data.ok {
            anyhow::bail!("Slack auth.test failed: {:?}", data.error);
        }

        data.user_id
            .ok_or_else(|| anyhow::anyhow!("No user_id in auth.test response"))
    }

    /// List accessible channels
    async fn list_accessible_channels(&self) -> Result<Vec<String>> {
        let mut channels = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut params = vec![
                ("exclude_archived", "true"),
                ("limit", "200"),
                ("types", "public_channel,private_channel,mpim,im"),
            ];

            if let Some(ref next) = cursor {
                params.push(("cursor", next));
            }

            let resp = self
                .client
                .get("https://slack.com/api/conversations.list")
                .bearer_auth(&self.config.token)
                .query(&params)
                .send()
                .await
                .context("Failed to call conversations.list")?;

            let data: ConversationsListResponse = resp
                .json()
                .await
                .context("Failed to parse conversations.list response")?;

            if !data.ok {
                tracing::warn!("Slack conversations.list failed: {:?}", data.error);
                break;
            }

            if let Some(chans) = data.channels {
                for ch in chans {
                    // Skip archived channels and channels bot is not a member of
                    if ch.is_archived.unwrap_or(false) {
                        continue;
                    }
                    if !ch.is_member.unwrap_or(true) {
                        continue;
                    }
                    if let Some(id) = ch.id {
                        channels.push(id);
                    }
                }
            }

            cursor = data
                .response_metadata
                .and_then(|m| m.next_cursor)
                .filter(|c| !c.is_empty());

            if cursor.is_none() {
                break;
            }
        }

        channels.sort();
        channels.dedup();
        Ok(channels)
    }

    /// Send a message to Slack
    async fn send_message(&self, channel: &str, text: &str, thread_ts: Option<&str>) -> Result<()> {
        let mut body = serde_json::json!({
            "channel": channel,
            "text": text
        });

        if let Some(ts) = thread_ts {
            body["thread_ts"] = serde_json::json!(ts);
        }

        let resp = self
            .client
            .post("https://slack.com/api/chat.postMessage")
            .bearer_auth(&self.config.token)
            .json(&body)
            .send()
            .await
            .context("Failed to call chat.postMessage")?;

        let data: SlackResponse = resp
            .json()
            .await
            .context("Failed to parse chat.postMessage response")?;

        if !data.ok {
            anyhow::bail!("Slack chat.postMessage failed: {:?}", data.error);
        }

        Ok(())
    }
}

#[async_trait]
impl Channel for SlackChannel {
    fn name(&self) -> &str {
        "slack"
    }

    async fn start(
        &self,
        inbound_tx: mpsc::Sender<InboundMessage>,
        mut outbound_rx: mpsc::Receiver<OutboundMessage>,
    ) -> Result<()> {
        // Get bot user ID
        let bot_user_id = match self.get_bot_user_id().await {
            Ok(id) => id,
            Err(e) => {
                tracing::error!("Failed to get Slack bot user ID: {}", e);
                return Err(e);
            }
        };

        // Determine which channels to monitor
        let scoped_channel = if self.config.channel_id.is_empty() || self.config.channel_id == "*" {
            None
        } else {
            Some(self.config.channel_id.clone())
        };

        let mut discovered_channels: Vec<String> = Vec::new();
        let mut last_discovery = Instant::now();
        let mut last_ts_by_channel: HashMap<String, String> = HashMap::new();

        if let Some(ref channel_id) = scoped_channel {
            tracing::info!("Slack channel listening on #{}...", channel_id);
        } else {
            tracing::info!("Slack listening on all accessible channels (auto-discovery)");
        }

        // Spawn outbound message handler
        let token = self.config.token.clone();
        let client = self.client.clone();
        let _outbound_handle = tokio::spawn(async move {
            while let Some(msg) = outbound_rx.recv().await {
                // Extract thread_ts from outbound metadata for threaded replies
                let thread_ts = msg.metadata.as_ref().and_then(|m| m.thread_id.as_deref());
                match send_slack_message(&client, &token, &msg.chat_id, &msg.content, thread_ts)
                    .await
                {
                    Ok(()) => {
                        tracing::debug!(
                            channel = %msg.chat_id,
                            threaded = thread_ts.is_some(),
                            "Sent Slack message"
                        );
                    }
                    Err(e) => {
                        tracing::warn!("Failed to send Slack message: {}", e);
                    }
                }
            }
        });

        // Main polling loop
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;

            // Determine target channels
            let target_channels = if let Some(ref channel_id) = scoped_channel {
                vec![channel_id.clone()]
            } else {
                // Auto-discover channels every 60 seconds
                if discovered_channels.is_empty()
                    || last_discovery.elapsed() >= Duration::from_secs(60)
                {
                    match self.list_accessible_channels().await {
                        Ok(channels) => {
                            if channels != discovered_channels {
                                tracing::info!(
                                    "Slack auto-discovery: listening on {} channel(s)",
                                    channels.len()
                                );
                            }
                            discovered_channels = channels;
                        }
                        Err(e) => {
                            tracing::warn!("Slack channel discovery failed: {}", e);
                        }
                    }
                    last_discovery = Instant::now();
                }
                discovered_channels.clone()
            };

            if target_channels.is_empty() {
                continue;
            }

            // Poll each channel
            for channel_id in target_channels {
                let mut params = vec![("channel", channel_id.clone()), ("limit", "10".to_string())];

                if let Some(last_ts) = last_ts_by_channel.get(&channel_id) {
                    if !last_ts.is_empty() {
                        params.push(("oldest", last_ts.clone()));
                    }
                }

                let resp = match self
                    .client
                    .get("https://slack.com/api/conversations.history")
                    .bearer_auth(&self.config.token)
                    .query(&params)
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::debug!("Slack poll error for {}: {}", channel_id, e);
                        continue;
                    }
                };

                let data: ConversationsHistoryResponse = match resp.json().await {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::debug!("Slack parse error for {}: {}", channel_id, e);
                        continue;
                    }
                };

                if !data.ok {
                    continue;
                }

                if let Some(messages) = data.messages {
                    // Messages come newest-first, reverse to process oldest first
                    for msg in messages.iter().rev() {
                        let ts = msg.ts.as_deref().unwrap_or("");
                        let user = msg.user.as_deref().unwrap_or("unknown");
                        let text = msg.text.as_deref().unwrap_or("");
                        let last_ts = last_ts_by_channel
                            .get(&channel_id)
                            .map(String::as_str)
                            .unwrap_or("");

                        // Skip bot's own messages
                        if user == bot_user_id {
                            continue;
                        }

                        // Skip bot messages (from other bots)
                        if msg.bot_id.is_some() {
                            continue;
                        }

                        // User validation
                        if !self.is_user_allowed(user) {
                            tracing::debug!(
                                "Slack: ignoring message from unauthorized user: {}",
                                user
                            );
                            continue;
                        }

                        // Skip empty or already-seen
                        if text.is_empty() || ts <= last_ts {
                            continue;
                        }

                        // Mention gating: in channels, only respond when @mentioned
                        let mention_tag = format!("<@{bot_user_id}>");
                        let mut content = text.to_string();
                        if self.config.mention_required {
                            if !content.contains(&mention_tag) {
                                continue; // Not addressed to us
                            }
                            // Strip mention from content
                            content = content.replace(&mention_tag, "").trim().to_string();
                            if content.is_empty() {
                                continue;
                            }
                        }

                        // Update last seen timestamp
                        last_ts_by_channel.insert(channel_id.clone(), ts.to_string());

                        // Build inbound message (thread context for reply routing)
                        let metadata = msg.thread_ts.as_ref().map(|ts| MessageMetadata {
                            thread_id: Some(ts.clone()),
                            ..Default::default()
                        });

                        let inbound = InboundMessage {
                            channel: "slack".to_string(),
                            chat_id: channel_id.clone(),
                            sender_id: user.to_string(),
                            content,
                            timestamp: Utc::now(),
                            metadata,
                        };

                        tracing::info!(
                            channel = %channel_id,
                            sender = %user,
                            "Slack message received"
                        );

                        if inbound_tx.send(inbound).await.is_err() {
                            tracing::info!("Slack channel: inbound channel closed");
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}

/// Send a Slack message (helper for outbound handler)
async fn send_slack_message(
    client: &Client,
    token: &str,
    channel: &str,
    text: &str,
    thread_ts: Option<&str>,
) -> Result<()> {
    let mut body = serde_json::json!({
        "channel": channel,
        "text": text
    });

    if let Some(ts) = thread_ts {
        body["thread_ts"] = serde_json::json!(ts);
    }

    let resp = client
        .post("https://slack.com/api/chat.postMessage")
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .context("Failed to send Slack message")?;

    let data: SlackResponse = resp
        .json()
        .await
        .context("Failed to parse Slack response")?;

    if !data.ok {
        anyhow::bail!("Slack API error: {:?}", data.error);
    }

    Ok(())
}

// ─── Slack API Types ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SlackResponse {
    ok: bool,
    error: Option<String>,
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConversationsListResponse {
    ok: bool,
    error: Option<String>,
    channels: Option<Vec<SlackChannelInfo>>,
    response_metadata: Option<ResponseMetadata>,
}

#[derive(Debug, Deserialize)]
struct SlackChannelInfo {
    id: Option<String>,
    is_archived: Option<bool>,
    is_member: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ResponseMetadata {
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConversationsHistoryResponse {
    ok: bool,
    error: Option<String>,
    messages: Option<Vec<SlackMessage>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct SlackMessage {
    ts: Option<String>,
    user: Option<String>,
    text: Option<String>,
    thread_ts: Option<String>,
    bot_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slack_channel_name() {
        let config = SlackConfig::default();
        let ch = SlackChannel::new(config);
        assert_eq!(ch.name(), "slack");
    }

    #[test]
    fn empty_allowlist_denies_everyone() {
        let config = SlackConfig::default();
        let ch = SlackChannel::new(config);
        assert!(!ch.is_user_allowed("U12345"));
        assert!(!ch.is_user_allowed("anyone"));
    }

    #[test]
    fn wildcard_allows_everyone() {
        let mut config = SlackConfig::default();
        config.allow_from = vec!["*".to_string()];
        let ch = SlackChannel::new(config);
        assert!(ch.is_user_allowed("U12345"));
        assert!(ch.is_user_allowed("anyone"));
    }

    #[test]
    fn specific_allowlist_filters() {
        let mut config = SlackConfig::default();
        config.allow_from = vec!["U111".to_string(), "U222".to_string()];
        let ch = SlackChannel::new(config);
        assert!(ch.is_user_allowed("U111"));
        assert!(ch.is_user_allowed("U222"));
        assert!(!ch.is_user_allowed("U333"));
    }
}
