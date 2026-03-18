//! Slack channel — Socket Mode (WebSocket) with HTTP polling fallback.
//!
//! If `app_token` is configured, uses Slack Socket Mode for real-time push events
//! (<100ms latency). Otherwise, falls back to polling `conversations.history` every 3s.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use super::traits::Channel;
use crate::bus::{InboundMessage, MessageMetadata, OutboundMessage};
use crate::config::SlackConfig;

/// Slack channel implementation — Socket Mode (preferred) or HTTP polling (fallback).
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

    /// Whether Socket Mode is available (app_token configured).
    pub fn has_socket_mode(&self) -> bool {
        !self.config.app_token.is_empty()
    }

    /// Check if a Slack user ID is allowed to interact.
    fn is_user_allowed(&self, user_id: &str) -> bool {
        if self.config.allow_from.is_empty() {
            return false;
        }
        self.config
            .allow_from
            .iter()
            .any(|u| u == "*" || u == user_id)
    }

    /// Get the bot's own user ID (to ignore own messages).
    async fn get_bot_user_id(&self) -> Result<String> {
        let resp = self
            .client
            .get("https://slack.com/api/auth.test")
            .bearer_auth(&self.config.token)
            .send()
            .await
            .context("Failed to call auth.test")?;

        let data: SlackApiResponse = resp
            .json()
            .await
            .context("Failed to parse auth.test response")?;

        if !data.ok {
            anyhow::bail!("Slack auth.test failed: {:?}", data.error);
        }

        data.user_id
            .ok_or_else(|| anyhow::anyhow!("No user_id in auth.test response"))
    }

    /// Normalize incoming text: strip bot mention if required, return None if should be skipped.
    fn normalize_content(&self, text: &str, bot_user_id: &str) -> Option<String> {
        let mention_tag = format!("<@{bot_user_id}>");
        let mut content = text.to_string();

        if self.config.mention_required {
            if !content.contains(&mention_tag) {
                return None;
            }
            content = content.replace(&mention_tag, "").trim().to_string();
            if content.is_empty() {
                return None;
            }
        }

        Some(content)
    }

    // ─── Socket Mode ─────────────────────────────────────────────────────

    /// Open a Socket Mode WebSocket URL via `apps.connections.open`.
    async fn open_socket_mode_url(&self) -> Result<String> {
        let resp = self
            .client
            .post("https://slack.com/api/apps.connections.open")
            .bearer_auth(&self.config.app_token)
            .send()
            .await
            .context("Failed to call apps.connections.open")?;

        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse apps.connections.open response")?;

        if body.get("ok") != Some(&serde_json::Value::Bool(true)) {
            let err = body
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown");
            anyhow::bail!("Slack apps.connections.open failed: {err}");
        }

        body.get("url")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow::anyhow!("apps.connections.open did not return url"))
    }

    /// Listen via Socket Mode WebSocket (reconnect loop).
    async fn listen_socket_mode(
        &self,
        inbound_tx: &mpsc::Sender<InboundMessage>,
        bot_user_id: &str,
        scoped_channel: &Option<String>,
    ) -> Result<()> {
        let mut last_ts_by_channel: HashMap<String, String> = HashMap::new();

        loop {
            // 1. Get fresh WebSocket URL
            let ws_url = match self.open_socket_mode_url().await {
                Ok(url) => url,
                Err(e) => {
                    tracing::warn!("Slack Socket Mode: failed to get WS URL: {e}");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
            };

            // 2. Connect
            let (ws_stream, _) = match tokio_tungstenite::connect_async(&ws_url).await {
                Ok(conn) => conn,
                Err(e) => {
                    tracing::warn!("Slack Socket Mode: connect failed: {e}");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
            };
            tracing::info!("Slack Socket Mode: WebSocket connected");

            let (mut write, mut read) = ws_stream.split();

            // 3. Read frames
            while let Some(frame) = read.next().await {
                let text = match frame {
                    Ok(WsMessage::Text(t)) => t,
                    Ok(WsMessage::Ping(payload)) => {
                        if let Err(e) = write.send(WsMessage::Pong(payload)).await {
                            tracing::warn!("Slack Socket Mode: pong failed: {e}");
                            break;
                        }
                        continue;
                    }
                    Ok(WsMessage::Close(_)) => {
                        tracing::warn!("Slack Socket Mode: closed by server");
                        break;
                    }
                    Ok(_) => continue,
                    Err(e) => {
                        tracing::warn!("Slack Socket Mode: read error: {e}");
                        break;
                    }
                };

                // Parse envelope
                let envelope: serde_json::Value = match serde_json::from_str(text.as_ref()) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::debug!("Slack Socket Mode: invalid JSON: {e}");
                        continue;
                    }
                };

                // ACK immediately (must be <3s)
                if let Some(envelope_id) = envelope.get("envelope_id").and_then(|v| v.as_str()) {
                    let ack = serde_json::json!({ "envelope_id": envelope_id });
                    if let Err(e) = write.send(WsMessage::Text(ack.to_string().into())).await {
                        tracing::warn!("Slack Socket Mode: ACK failed: {e}");
                        break;
                    }
                }

                // Handle disconnect event
                let envelope_type = envelope
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if envelope_type == "disconnect" {
                    tracing::warn!("Slack Socket Mode: disconnect event received");
                    break;
                }
                if envelope_type != "events_api" {
                    continue;
                }

                // Extract message event
                let Some(event) = envelope.get("payload").and_then(|p| p.get("event")) else {
                    continue;
                };
                if event.get("type").and_then(|v| v.as_str()) != Some("message") {
                    continue;
                }
                // Skip non-user subtypes (channel_join, message_changed, etc.)
                if event.get("subtype").is_some() {
                    continue;
                }

                // Extract fields
                let channel_id = event
                    .get("channel")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if channel_id.is_empty() {
                    continue;
                }

                // Channel scoping
                if let Some(ref scoped) = scoped_channel {
                    if scoped != channel_id {
                        continue;
                    }
                }

                let user = event
                    .get("user")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if user.is_empty() || user == bot_user_id {
                    continue;
                }
                if !self.is_user_allowed(user) {
                    tracing::debug!("Slack: ignoring unauthorized user: {user}");
                    continue;
                }

                let raw_text = event
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if raw_text.is_empty() {
                    continue;
                }

                let ts = event.get("ts").and_then(|v| v.as_str()).unwrap_or_default();
                if ts.is_empty() {
                    continue;
                }

                // Dedup
                let last_ts = last_ts_by_channel
                    .get(channel_id)
                    .map(String::as_str)
                    .unwrap_or_default();
                if ts <= last_ts {
                    continue;
                }

                // Mention gating
                let Some(content) = self.normalize_content(raw_text, bot_user_id) else {
                    continue;
                };

                last_ts_by_channel.insert(channel_id.to_string(), ts.to_string());

                // Thread context
                let thread_ts = event.get("thread_ts").and_then(|v| v.as_str()).or(Some(ts));
                let metadata = thread_ts.map(|tts| MessageMetadata {
                    thread_id: Some(tts.to_string()),
                    ..Default::default()
                });

                let inbound = InboundMessage {
                    channel: "slack".to_string(),
                    chat_id: channel_id.to_string(),
                    sender_id: user.to_string(),
                    content,
                    timestamp: Utc::now(),
                    metadata,
                };

                tracing::info!(
                    channel = %channel_id,
                    sender = %user,
                    "Slack message received (Socket Mode)"
                );

                if inbound_tx.send(inbound).await.is_err() {
                    tracing::info!("Slack: inbound channel closed");
                    return Ok(());
                }
            }

            tracing::warn!("Slack Socket Mode: reconnecting in 3s...");
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }

    // ─── HTTP Polling Fallback ───────────────────────────────────────────

    /// List accessible channels (for auto-discovery in polling mode).
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

    /// HTTP polling loop (fallback when no app_token).
    async fn listen_polling(
        &self,
        inbound_tx: &mpsc::Sender<InboundMessage>,
        bot_user_id: &str,
        scoped_channel: &Option<String>,
    ) -> Result<()> {
        let mut discovered_channels: Vec<String> = Vec::new();
        let mut last_discovery = Instant::now();
        let mut last_ts_by_channel: HashMap<String, String> = HashMap::new();

        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;

            let target_channels = if let Some(ref channel_id) = scoped_channel {
                vec![channel_id.clone()]
            } else {
                if discovered_channels.is_empty()
                    || last_discovery.elapsed() >= Duration::from_secs(60)
                {
                    match self.list_accessible_channels().await {
                        Ok(channels) => {
                            if channels != discovered_channels {
                                tracing::info!(
                                    "Slack auto-discovery: {} channel(s)",
                                    channels.len()
                                );
                            }
                            discovered_channels = channels;
                        }
                        Err(e) => {
                            tracing::warn!("Slack channel discovery failed: {e}");
                        }
                    }
                    last_discovery = Instant::now();
                }
                discovered_channels.clone()
            };

            if target_channels.is_empty() {
                continue;
            }

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
                        tracing::debug!("Slack poll error for {channel_id}: {e}");
                        continue;
                    }
                };

                let data: ConversationsHistoryResponse = match resp.json().await {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::debug!("Slack parse error for {channel_id}: {e}");
                        continue;
                    }
                };

                if !data.ok {
                    continue;
                }

                if let Some(messages) = data.messages {
                    for msg in messages.iter().rev() {
                        let ts = msg.ts.as_deref().unwrap_or("");
                        let user = msg.user.as_deref().unwrap_or("unknown");
                        let text = msg.text.as_deref().unwrap_or("");
                        let last_ts = last_ts_by_channel
                            .get(&channel_id)
                            .map(String::as_str)
                            .unwrap_or("");

                        if user == bot_user_id {
                            continue;
                        }
                        if msg.bot_id.is_some() {
                            continue;
                        }
                        if !self.is_user_allowed(user) {
                            continue;
                        }
                        if text.is_empty() || ts <= last_ts {
                            continue;
                        }

                        let Some(content) = self.normalize_content(text, bot_user_id) else {
                            continue;
                        };

                        last_ts_by_channel.insert(channel_id.clone(), ts.to_string());

                        let metadata = msg.thread_ts.as_ref().map(|tts| MessageMetadata {
                            thread_id: Some(tts.clone()),
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
                            "Slack message received (polling)"
                        );

                        if inbound_tx.send(inbound).await.is_err() {
                            tracing::info!("Slack: inbound channel closed");
                            return Ok(());
                        }
                    }
                }
            }
        }
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
        let bot_user_id = self.get_bot_user_id().await?;

        let scoped_channel = if self.config.channel_id.is_empty() || self.config.channel_id == "*" {
            None
        } else {
            Some(self.config.channel_id.clone())
        };

        // Proactive messaging check
        let proactive_target = if !self.config.default_channel_id.is_empty() {
            Some(&self.config.default_channel_id)
        } else if scoped_channel.is_some() {
            scoped_channel.as_ref()
        } else {
            None
        };
        if proactive_target.is_none() {
            tracing::warn!(
                "Slack: default_channel_id not set — proactive messaging disabled. \
                 Set [channels.slack] default_channel_id in config.toml to enable."
            );
        }

        if self.has_socket_mode() {
            tracing::info!("Slack starting in Socket Mode (real-time)");
        } else {
            tracing::info!("Slack starting in polling mode (3s interval)");
            if let Some(ref ch) = scoped_channel {
                tracing::info!("Slack listening on #{ch}");
            } else {
                tracing::info!("Slack listening on all accessible channels");
            }
        }

        // Spawn outbound handler (same for both modes — uses Web API)
        let token = self.config.token.clone();
        let client = self.client.clone();
        let _outbound_handle = tokio::spawn(async move {
            while let Some(msg) = outbound_rx.recv().await {
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
                        tracing::warn!("Failed to send Slack message: {e}");
                    }
                }
            }
        });

        // Inbound: Socket Mode or polling
        if self.has_socket_mode() {
            self.listen_socket_mode(&inbound_tx, &bot_user_id, &scoped_channel)
                .await
        } else {
            self.listen_polling(&inbound_tx, &bot_user_id, &scoped_channel)
                .await
        }
    }
}

// ─── Outbound Helper ─────────────────────────────────────────────────────

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

    let data: SlackApiResponse = resp
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
struct SlackApiResponse {
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
    #[allow(dead_code)]
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

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> SlackConfig {
        SlackConfig::default()
    }

    #[test]
    fn slack_channel_name() {
        let ch = SlackChannel::new(make_config());
        assert_eq!(ch.name(), "slack");
    }

    #[test]
    fn empty_allowlist_denies_everyone() {
        let ch = SlackChannel::new(make_config());
        assert!(!ch.is_user_allowed("U12345"));
        assert!(!ch.is_user_allowed("anyone"));
    }

    #[test]
    fn wildcard_allows_everyone() {
        let mut config = make_config();
        config.allow_from = vec!["*".to_string()];
        let ch = SlackChannel::new(config);
        assert!(ch.is_user_allowed("U12345"));
        assert!(ch.is_user_allowed("anyone"));
    }

    #[test]
    fn specific_allowlist_filters() {
        let mut config = make_config();
        config.allow_from = vec!["U111".to_string(), "U222".to_string()];
        let ch = SlackChannel::new(config);
        assert!(ch.is_user_allowed("U111"));
        assert!(ch.is_user_allowed("U222"));
        assert!(!ch.is_user_allowed("U333"));
    }

    #[test]
    fn socket_mode_detection() {
        let ch = SlackChannel::new(make_config());
        assert!(!ch.has_socket_mode());

        let mut config = make_config();
        config.app_token = "xapp-1-A111-222-abc".to_string();
        let ch = SlackChannel::new(config);
        assert!(ch.has_socket_mode());
    }

    #[test]
    fn mention_gating() {
        let mut config = make_config();
        config.mention_required = true;
        let ch = SlackChannel::new(config);

        // Without mention → None
        assert!(ch.normalize_content("hello world", "U_BOT").is_none());

        // With mention → stripped
        let result = ch.normalize_content("hey <@U_BOT> do something", "U_BOT");
        assert_eq!(result, Some("hey  do something".to_string()));

        // Only mention → None (empty after strip)
        assert!(ch.normalize_content("<@U_BOT>", "U_BOT").is_none());
    }

    #[test]
    fn mention_not_required() {
        let mut config = make_config();
        config.mention_required = false;
        let ch = SlackChannel::new(config);

        assert_eq!(
            ch.normalize_content("hello", "U_BOT"),
            Some("hello".to_string())
        );
    }
}
