use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::bus::{
    build_outbound_meta, InboundMessage, MessageMetadata, OutboundMessage, StreamMessage,
};
use crate::config::Config;
use crate::scheduler::{CronEvent, CronScheduler, ScheduledKind};
use crate::security::PairingManager;
use crate::session::SessionManager;
use crate::storage::{AutomationUpdate, Database, EmailPendingRow};
use crate::utils::strip_reasoning;
use crate::workflows::engine::WorkflowEngine;
use crate::workflows::WorkflowEvent;
use tokio::sync::RwLock;

use super::debounce::{
    aggregate, get_session_lock, should_skip_debounce, DebounceConfig, DispatchContext,
    MessageDebouncer, PreparedMessage, SessionLocks,
};
use super::email_approval::{ApprovalAction, EmailApprovalHandler};

#[cfg(feature = "web-ui")]
use crate::web::WebServer;

// Conditional channel imports
#[cfg(feature = "channel-telegram")]
use crate::channels::TelegramChannel;

#[cfg(feature = "channel-discord")]
use crate::channels::DiscordChannel;

#[cfg(feature = "channel-whatsapp")]
use crate::channels::WhatsAppChannel;

#[cfg(feature = "channel-email")]
use crate::channels::EmailChannel;

use crate::channels::SlackChannel;
use crate::channels::{Channel, ChannelHealthTracker}; // Import trait to call .start()

use super::AgentLoop;

/// Maximum restart attempts before giving up on a channel.
const MAX_CHANNEL_RESTARTS: u32 = 10;

// Auth is now centralized in agent/auth.rs — no per-channel allow_from merge needed.

/// Shared outbound routing table: channel_name → sender.
/// Wrapped in `Arc<RwLock>` so the channel command handler can add new entries at runtime.
pub type SharedOutboundSenders =
    Arc<RwLock<Vec<(String, mpsc::Sender<OutboundMessage>)>>>;

/// Command to start a channel at runtime (hot-reload after config/pairing).
#[derive(Debug)]
pub enum ChannelCommand {
    /// Start a channel by name (reads fresh config from the shared Arc).
    Start { channel: String },
}

/// A running channel: name + task handle + outbound sender
struct ChannelHandle {
    name: String,
    handle: tokio::task::JoinHandle<()>,
    outbound_tx: mpsc::Sender<OutboundMessage>,
}

/// Spawn a channel with health monitoring and automatic restart on failure.
///
/// The gateway keeps a stable `outbound_tx` in its routing table. On each
/// restart attempt, a fresh (inner_tx, inner_rx) pair is created for the
/// channel, and a relay task forwards messages from the stable queue to the
/// inner receiver. This way the routing table doesn't change on restart.
fn spawn_monitored_channel<F>(
    name: &str,
    health: &Arc<ChannelHealthTracker>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    channel_factory: F,
) -> ChannelHandle
where
    F: Fn() -> Box<dyn Channel> + Send + 'static,
{
    let ch_name = name.to_string();
    let health = health.clone();
    let (outbound_tx, mut stable_rx) = mpsc::channel::<OutboundMessage>(100);

    let handle = tokio::spawn(async move {
        let retry_config = crate::utils::retry::RetryConfig::patient();
        let mut attempt: u32 = 0;

        loop {
            health.mark_started(&ch_name);
            tracing::info!(channel = %ch_name, attempt, "Channel starting");

            // Create fresh inner channel for this attempt
            let (inner_tx, inner_rx) = mpsc::channel::<OutboundMessage>(100);

            // Relay: forward from stable outbound queue to this attempt's inner_tx.
            // Stops when inner_tx is dropped (channel crashes) or stable_rx is closed.
            let relay_inner_tx = inner_tx.clone();
            let relay_handle = tokio::spawn(async move {
                while let Some(msg) = stable_rx.recv().await {
                    if relay_inner_tx.send(msg).await.is_err() {
                        break; // inner channel gone (crashed), stop relaying
                    }
                }
                stable_rx // return ownership so the next iteration can reuse it
            });

            let channel = channel_factory();
            let inbound = inbound_tx.clone();
            let result = channel.start(inbound, inner_rx).await;

            // Channel exited — abort relay and reclaim stable_rx
            relay_handle.abort();
            match relay_handle.await {
                Ok(rx) => stable_rx = rx,
                Err(_) => {
                    // Relay was aborted, we need a new stable_rx but can't get it back.
                    // This shouldn't happen in practice, but if it does, we stop.
                    tracing::error!(channel = %ch_name, "Lost outbound relay — stopping channel");
                    health.mark_stopped(&ch_name, Some("internal: lost outbound relay"));
                    break;
                }
            }

            match result {
                Ok(()) => {
                    // Clean exit (channel decided to stop)
                    tracing::info!(channel = %ch_name, "Channel exited cleanly");
                    health.mark_stopped(&ch_name, None);
                    break;
                }
                Err(e) => {
                    let err_msg = format!("{e:#}");
                    tracing::error!(channel = %ch_name, attempt, error = %err_msg, "Channel crashed");
                    health.mark_stopped(&ch_name, Some(&err_msg));

                    attempt += 1;
                    if attempt >= MAX_CHANNEL_RESTARTS {
                        tracing::error!(
                            channel = %ch_name,
                            max = MAX_CHANNEL_RESTARTS,
                            "Max restarts exceeded — giving up"
                        );
                        break;
                    }

                    // Exponential backoff before restart
                    let delay = retry_config.delay_for_attempt(attempt);
                    tracing::info!(
                        channel = %ch_name,
                        delay_secs = delay.as_secs(),
                        "Waiting before restart"
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }
    });

    ChannelHandle {
        name: name.to_string(),
        handle,
        outbound_tx,
    }
}

/// Gateway — orchestrates channels, agent loop, cron scheduler, and message routing.
///
/// Architecture:
/// ```text
/// Telegram ─┐
/// Cron ──────┤──→ InboundMessage ──→ Gateway ──→ AgentLoop ──→ OutboundMessage ──→ Channel
/// (future) ──┘
/// ```
pub struct Gateway {
    registry: Arc<super::registry::AgentRegistry>,
    config: Arc<RwLock<Config>>,
    #[allow(dead_code)]
    session_manager: SessionManager,
    cron_scheduler: Arc<CronScheduler>,
    cron_event_rx: mpsc::Receiver<CronEvent>,
    /// Receiver for messages sent by tools (MessageTool) that need routing to channels
    tool_message_rx: Option<mpsc::Receiver<OutboundMessage>>,
    /// Sender for streaming chunks to the web server (forwarded to WebSocket sessions)
    web_stream_tx: Option<mpsc::Sender<StreamMessage>>,
    /// Database handle passed to the web server for memory/vault APIs
    db: Database,
    /// Provider health tracker for circuit breaker metrics (shared with web UI)
    health_tracker: Option<Arc<crate::provider::ProviderHealthTracker>>,
    /// Emergency stop handles — shared between gateway and web UI
    estop_handles: Arc<tokio::sync::RwLock<crate::security::EStopHandles>>,
    /// Workflow engine + event receiver for persistent multi-step tasks
    workflow_engine: Option<Arc<WorkflowEngine>>,
    workflow_event_rx: Option<mpsc::Receiver<WorkflowEvent>>,
    /// Business engine for autonomous business management
    business_engine: Option<Arc<crate::business::engine::BusinessEngine>>,
    /// Channel health tracker for circuit breaker + auto-restart
    channel_health: Arc<ChannelHealthTracker>,
    /// Resolved agent definitions (from config `[agents.*]` or synthesized "default").
    /// Stored for MAG-2 routing and web API introspection.
    agent_definitions: HashMap<String, super::definition::AgentDefinition>,
}

impl Gateway {
    pub fn new(
        registry: Arc<super::registry::AgentRegistry>,
        config: Arc<RwLock<Config>>,
        session_manager: SessionManager,
        cron_scheduler: Arc<CronScheduler>,
        cron_event_rx: mpsc::Receiver<CronEvent>,
        db: Database,
    ) -> Self {
        Self {
            registry,
            config,
            session_manager,
            cron_scheduler,
            cron_event_rx,
            tool_message_rx: None,
            web_stream_tx: None,
            db,
            health_tracker: None,
            estop_handles: Arc::new(tokio::sync::RwLock::new(
                crate::security::EStopHandles::default(),
            )),
            workflow_engine: None,
            workflow_event_rx: None,
            business_engine: None,
            channel_health: Arc::new(ChannelHealthTracker::new()),
            agent_definitions: HashMap::new(),
        }
    }

    /// Get resolved agent definitions (populated after `run()` starts).
    pub fn agent_definitions(&self) -> &HashMap<String, super::definition::AgentDefinition> {
        &self.agent_definitions
    }

    /// Get channel health tracker (for sharing with web server).
    pub fn channel_health(&self) -> Arc<ChannelHealthTracker> {
        self.channel_health.clone()
    }

    /// Set the receiver for tool-originated messages (from MessageTool)
    pub fn set_tool_message_rx(&mut self, rx: mpsc::Receiver<OutboundMessage>) {
        self.tool_message_rx = Some(rx);
    }

    /// Set the provider health tracker for circuit breaker + web UI metrics.
    pub fn set_health_tracker(&mut self, tracker: Arc<crate::provider::ProviderHealthTracker>) {
        self.health_tracker = Some(tracker);
    }

    /// Set the workflow engine and its event receiver.
    pub fn set_workflow_engine(
        &mut self,
        engine: Arc<WorkflowEngine>,
        event_rx: mpsc::Receiver<WorkflowEvent>,
    ) {
        self.workflow_engine = Some(engine);
        self.workflow_event_rx = Some(event_rx);
    }

    /// Set the business engine for autonomous business management.
    pub fn set_business_engine(&mut self, engine: Arc<crate::business::engine::BusinessEngine>) {
        self.business_engine = Some(engine);
    }

    /// Get the estop handles Arc (for populating from main.rs after gateway creation).
    pub fn estop_handles(&self) -> Arc<tokio::sync::RwLock<crate::security::EStopHandles>> {
        self.estop_handles.clone()
    }

    /// Start the gateway — runs all channels + cron + agent loop.
    /// Blocks until Ctrl+C.
    pub async fn run(mut self) -> Result<()> {
        // Snapshot config for channel startup (one-time operation).
        // The Arc is passed to WebServer so web UI changes propagate to the agent.
        let config = self.config.read().await.clone();

        // Resolve agent definitions from config (MAG-1).
        self.agent_definitions = super::definition::AgentDefinition::resolve_all(&config);
        tracing::info!(
            count = self.agent_definitions.len(),
            agents = ?self.agent_definitions.keys().collect::<Vec<_>>(),
            "Agent definitions resolved"
        );

        let (inbound_tx, mut inbound_rx) = mpsc::channel::<InboundMessage>(100);
        let (channel_cmd_tx, mut channel_cmd_rx) = mpsc::channel::<ChannelCommand>(10);
        let mut channels: Vec<ChannelHandle> = Vec::new();

        // --- Start Telegram channel ---
        #[cfg(feature = "channel-telegram")]
        if config.channels.telegram.enabled {
            let mut tg_config = config.channels.telegram.clone();
            // Resolve token from encrypted storage if marker is present
            if tg_config.token == "***ENCRYPTED***" || tg_config.token.is_empty() {
                if let Ok(secrets) = crate::storage::global_secrets() {
                    let key = crate::storage::SecretKey::channel_token("telegram");
                    if let Ok(Some(real_token)) = secrets.get(&key) {
                        tg_config.token = real_token;
                    }
                }
            }

            // Skip if no valid token
            if tg_config.token.is_empty() || tg_config.token == "***ENCRYPTED***" {
                tracing::error!("Telegram enabled but no token found - skipping channel");
            } else {
                let ch = spawn_monitored_channel(
                    "telegram",
                    &self.channel_health,
                    inbound_tx.clone(),
                    move || Box::new(TelegramChannel::new(tg_config.clone())),
                );
                channels.push(ch);
                tracing::info!("Telegram channel started (monitored)");
            }
        }

        // --- Start Discord channel ---
        #[cfg(feature = "channel-discord")]
        if config.channels.discord.enabled {
            let mut dc_config = config.channels.discord.clone();
            // Resolve token from encrypted storage if marker is present
            if dc_config.token == "***ENCRYPTED***" || dc_config.token.is_empty() {
                if let Ok(secrets) = crate::storage::global_secrets() {
                    let key = crate::storage::SecretKey::channel_token("discord");
                    if let Ok(Some(real_token)) = secrets.get(&key) {
                        dc_config.token = real_token;
                    }
                }
            }

            // Skip if no valid token
            if dc_config.token.is_empty() || dc_config.token == "***ENCRYPTED***" {
                tracing::error!("Discord enabled but no token found - skipping channel");
            } else {
                let ch =
                    spawn_monitored_channel("discord", &self.channel_health, inbound_tx.clone(), {
                        let health = self.channel_health.clone();
                        move || {
                            Box::new(
                                DiscordChannel::new(dc_config.clone()).with_health(health.clone()),
                            )
                        }
                    });
                channels.push(ch);
                tracing::info!("Discord channel started (monitored)");
            }
        }

        // --- Start WhatsApp channel ---
        #[cfg(feature = "channel-whatsapp")]
        if config.channels.whatsapp.enabled {
            let wa_config = config.channels.whatsapp.clone();
            let ch = spawn_monitored_channel(
                "whatsapp",
                &self.channel_health,
                inbound_tx.clone(),
                move || Box::new(WhatsAppChannel::new(wa_config.clone())),
            );
            channels.push(ch);
            tracing::info!("WhatsApp channel started (monitored)");
        }

        // --- Start Slack channel ---
        if config.channels.slack.enabled {
            let mut slack_config = config.channels.slack.clone();

            // Resolve tokens from encrypted storage if marker is present
            if slack_config.token == "***ENCRYPTED***" || slack_config.token.is_empty() {
                if let Ok(secrets) = crate::storage::global_secrets() {
                    let key = crate::storage::SecretKey::channel_token("slack");
                    if let Ok(Some(real_token)) = secrets.get(&key) {
                        slack_config.token = real_token;
                    }
                }
            }
            if slack_config.app_token == "***ENCRYPTED***" || slack_config.app_token.is_empty() {
                if let Ok(secrets) = crate::storage::global_secrets() {
                    let key = crate::storage::SecretKey::channel_token("slack_app");
                    if let Ok(Some(real_token)) = secrets.get(&key) {
                        slack_config.app_token = real_token;
                    }
                }
            }

            // Skip if no valid token
            if slack_config.token.is_empty() || slack_config.token == "***ENCRYPTED***" {
                tracing::error!("Slack enabled but no token found - skipping channel");
            } else {
                let ch = spawn_monitored_channel(
                    "slack",
                    &self.channel_health,
                    inbound_tx.clone(),
                    move || Box::new(SlackChannel::new(slack_config.clone())),
                );
                channels.push(ch);
                tracing::info!("Slack channel started (monitored)");
            }
        }

        // --- Start Email channel (multi-account) ---
        #[cfg(feature = "channel-email")]
        {
            // Migrate legacy [channels.email] → [channels.emails.default]
            let mut channels_config = config.channels.clone();
            channels_config.migrate_legacy_email();

            let active_accounts = channels_config.active_email_accounts();
            if !active_accounts.is_empty() {
                let accounts: std::collections::HashMap<String, crate::config::EmailAccountConfig> =
                    active_accounts
                        .into_iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();

                let ch = spawn_monitored_channel(
                    "email",
                    &self.channel_health,
                    inbound_tx.clone(),
                    move || Box::new(EmailChannel::new(accounts.clone())),
                );
                channels.push(ch);
                tracing::info!("Email channel started (monitored, multi-account)");
            }
        }

        // --- Start MCP channels ---
        for (name, mcp_cfg) in &config.channels.mcp {
            if !mcp_cfg.enabled {
                continue;
            }
            let channel_name = format!("mcp:{name}");
            let cfg = mcp_cfg.clone();
            let ch_name = channel_name.clone();
            let ch = spawn_monitored_channel(
                &channel_name,
                &self.channel_health,
                inbound_tx.clone(),
                move || Box::new(crate::channels::McpChannel::new(ch_name.clone(), cfg.clone())),
            );
            channels.push(ch);
            tracing::info!(name = %name, "MCP channel registered");
        }

        // --- Start Web UI server ---
        #[cfg(feature = "web-ui")]
        if config.channels.web.enabled {
            let shared_config = self.config.clone(); // Arc — shared with agent for hot-reload
            let web_inbound_tx = inbound_tx.clone();
            let port = config.channels.web.port;
            let (web_outbound_tx, web_outbound_rx) = mpsc::channel::<OutboundMessage>(100);

            // Channel for streaming text chunks from the agent to WebSocket clients
            let (stream_tx, stream_rx) = mpsc::channel::<StreamMessage>(256);
            self.web_stream_tx = Some(stream_tx);

            let web_db = self.db.clone();
            let web_health_tracker = self.health_tracker.clone();
            let web_channel_health = self.channel_health.clone();
            let web_channel_cmd_tx = channel_cmd_tx.clone();
            let web_workflow_engine = self.workflow_engine.clone();
            let web_business_engine = self.business_engine.clone();
            let web_estop_handles = self.estop_handles.clone();
            let default_agent = self.registry.default_agent();
            let web_tool_registry = default_agent.tool_registry_handle();
            // Share the memory searcher with the web server for hybrid search API
            #[cfg(feature = "embeddings")]
            let web_memory_searcher = default_agent.memory_searcher_handle();
            #[cfg(feature = "embeddings")]
            let web_rag_engine = default_agent.rag_engine_handle();

            let handle = tokio::spawn(async move {
                let mut server =
                    WebServer::new(shared_config, web_inbound_tx, web_outbound_rx, web_db);
                server.set_stream_rx(stream_rx);
                if let Some(tracker) = web_health_tracker {
                    server.set_health_tracker(tracker);
                }
                server.set_channel_health(web_channel_health);
                if let Some(wf_engine) = web_workflow_engine {
                    server.set_workflow_engine(wf_engine);
                }
                if let Some(biz_engine) = web_business_engine {
                    server.set_business_engine(biz_engine);
                }
                server.set_estop_handles(web_estop_handles);
                server.set_tool_registry(web_tool_registry);
                server.set_channel_cmd_tx(web_channel_cmd_tx);
                #[cfg(feature = "embeddings")]
                if let Some(searcher) = web_memory_searcher {
                    server.set_memory_searcher(searcher);
                }
                #[cfg(feature = "embeddings")]
                if let Some(rag) = web_rag_engine {
                    server.set_rag_engine(rag);
                }
                if let Err(e) = server.start().await {
                    tracing::error!(error = %e, "Web UI server error");
                }
            });

            channels.push(ChannelHandle {
                name: "web".to_string(),
                handle,
                outbound_tx: web_outbound_tx,
            });
            self.channel_health.mark_started("web");
            tracing::info!(port = port, "Web UI started at http://localhost:{}", port);
        }

        // --- Start Cron scheduler (created externally, started here) ---
        let _cron_handle = self.cron_scheduler.clone().start().await?;
        let mut cron_event_rx = self.cron_event_rx;
        tracing::info!("Cron scheduler started");

        // --- Run memory cleanup if enabled ---
        if config.memory.auto_cleanup {
            let mem_config = &config.memory;
            tracing::info!(
                conversation_days = mem_config.conversation_retention_days,
                history_days = mem_config.history_retention_days,
                "Running automatic memory cleanup"
            );
            match self
                .db
                .run_memory_cleanup(
                    mem_config.conversation_retention_days,
                    mem_config.history_retention_days,
                )
                .await
            {
                Ok(result) => {
                    if result.messages_deleted > 0 || result.chunks_deleted > 0 {
                        tracing::info!(
                            messages = result.messages_deleted,
                            chunks = result.chunks_deleted,
                            "Memory cleanup completed"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Memory cleanup failed (non-fatal)");
                }
            }
            match crate::web::api::cleanup_chat_upload_dirs(
                &self.db,
                mem_config.conversation_retention_days,
            )
            .await
            {
                Ok(stats) => {
                    if stats.files_deleted > 0 || stats.directories_deleted > 0 {
                        tracing::info!(
                            files = stats.files_deleted,
                            directories = stats.directories_deleted,
                            bytes = stats.bytes_deleted,
                            "Chat upload cleanup completed"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Chat upload cleanup failed (non-fatal)");
                }
            }
        }

        if channels.is_empty() {
            println!("No channels enabled. Set [channels.telegram] enabled = true or [channels.web] enabled = true in ~/.homun/config.toml");
            return Ok(());
        }

        // Keep one sender for scheduler/system events, then drop our main copy.
        let scheduler_inbound_tx = inbound_tx.clone();
        let cmd_inbound_tx = inbound_tx.clone(); // For channel command handler
        drop(inbound_tx);

        let active = channels.len();
        let web_url = if config.channels.web.enabled {
            format!(" Web UI: http://localhost:{}", config.channels.web.port)
        } else {
            String::new()
        };
        tracing::info!(channels = active, "Gateway running");
        println!("🧪 Homun gateway running ({active} channel(s) + cron).{web_url}");
        println!("Press Ctrl+C to stop.");

        // Build outbound routing table: channel_name → sender (shared for hot-reload)
        let outbound_senders: SharedOutboundSenders = Arc::new(RwLock::new(
            channels
                .iter()
                .map(|ch| (ch.name.clone(), ch.outbound_tx.clone()))
                .collect(),
        ));

        // --- Build pairing config: channel_name → (pairing_required, allow_from set) ---
        let pairing_manager = Arc::new(PairingManager::new(self.db.clone()));
        let mut pairing_config: HashMap<String, (bool, HashSet<String>)> = HashMap::new();
        {
            let ch = &config.channels;
            pairing_config.insert(
                "telegram".into(),
                (
                    ch.telegram.pairing_required,
                    ch.telegram.allow_from.iter().cloned().collect(),
                ),
            );
            pairing_config.insert(
                "discord".into(),
                (
                    ch.discord.pairing_required,
                    ch.discord.allow_from.iter().cloned().collect(),
                ),
            );
            pairing_config.insert(
                "whatsapp".into(),
                (
                    ch.whatsapp.pairing_required,
                    ch.whatsapp.allow_from.iter().cloned().collect(),
                ),
            );
            pairing_config.insert(
                "slack".into(),
                (
                    ch.slack.pairing_required,
                    ch.slack.allow_from.iter().cloned().collect(),
                ),
            );
            // Multi-account email pairing
            let mut email_channels_cfg = ch.clone();
            email_channels_cfg.migrate_legacy_email();
            for (name, acc) in &email_channels_cfg.emails {
                pairing_config.insert(
                    format!("email:{name}"),
                    (
                        acc.pairing_required,
                        acc.allow_from.iter().cloned().collect(),
                    ),
                );
            }
            // Legacy fallback
            if email_channels_cfg.emails.is_empty() {
                pairing_config.insert(
                    "email".into(),
                    (
                        ch.email.pairing_required,
                        ch.email.allow_from.iter().cloned().collect(),
                    ),
                );
            }
            // MCP channels
            for (name, mcp_cfg) in &ch.mcp {
                if mcp_cfg.enabled {
                    pairing_config.insert(
                        format!("mcp:{name}"),
                        (
                            mcp_cfg.pairing_required,
                            mcp_cfg.allow_from.iter().cloned().collect(),
                        ),
                    );
                }
            }
        }

        // Merge contact identities into pairing allow_from sets
        for (ch_name, (_pairing, allow_set)) in &mut pairing_config {
            let channel_key = if ch_name.starts_with("email:") { "email" } else { ch_name.as_str() };
            if let Ok(ids) = self.db.contact_identifiers_for_channel(channel_key).await {
                allow_set.extend(ids);
            }
        }

        // --- Safety: warn if enabled channels have empty allow_from and no pairing ---
        for (ch_name, (pairing_required, allow_set)) in &pairing_config {
            if ch_name == "web" {
                continue; // web uses session auth, not allow_from
            }
            // Only warn for channels that are actually enabled
            let enabled = match ch_name.as_str() {
                "telegram" => config.channels.telegram.enabled,
                "discord" => config.channels.discord.enabled,
                "slack" => config.channels.slack.enabled,
                "whatsapp" => config.channels.whatsapp.enabled,
                _ if ch_name.starts_with("email:") => true, // already in active accounts
                "email" => config.channels.email.enabled,
                _ if ch_name.starts_with("mcp:") => true, // already filtered by enabled
                _ => false,
            };
            if enabled && allow_set.is_empty() && !pairing_required {
                tracing::warn!(
                    channel = %ch_name,
                    "Channel has NO allow_from and pairing is disabled — \
                     bot will not respond to anyone. Add allowed users or enable pairing."
                );
            }
        }

        // Spawn periodic cleanup for expired pairing requests
        let cleanup_pm = pairing_manager.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                cleanup_pm.cleanup_expired().await;
            }
        });

        // --- Build approval notify routing table ---
        // Maps channel_name → (notify_channel, notify_chat_id) for assisted mode
        let mut approval_routes: HashMap<String, (String, String)> = HashMap::new();
        {
            // Email accounts
            let mut email_cfg = config.channels.clone();
            email_cfg.migrate_legacy_email();
            for (name, acc) in &email_cfg.emails {
                if let (Some(ch), Some(cid)) = (&acc.notify_channel, &acc.notify_chat_id) {
                    approval_routes.insert(format!("email:{name}"), (ch.clone(), cid.clone()));
                }
            }
            // Chat channels (via unified ChannelBehavior)
            for ch_name in &["telegram", "whatsapp", "discord", "slack"] {
                if let Some(b) = config.channels.behavior_for(ch_name) {
                    if let (Some(nc), Some(ncid)) = (b.notify_channel(), b.notify_chat_id()) {
                        approval_routes.insert(ch_name.to_string(), (nc.to_string(), ncid.to_string()));
                    }
                }
            }
        }

        // --- Email approval handler ---
        let approval_handler = EmailApprovalHandler::new(self.db.clone(), &approval_routes);

        // --- RAG engine handle for file ingestion ---
        #[cfg(feature = "embeddings")]
        let routing_rag_engine = self.registry.default_agent().rag_engine_handle();

        // --- Debounce + per-session lock infrastructure ---
        let debounce_config = DebounceConfig::from_agent_config(
            config.agent.debounce_window_ms,
            config.agent.debounce_max_batch,
        );
        let session_locks: SessionLocks = Arc::new(std::sync::Mutex::new(HashMap::new()));

        // --- Main message routing loop ---
        let registry = self.registry.clone();
        let senders_for_routing = outbound_senders.clone();
        let web_stream_tx = self.web_stream_tx.take();
        let web_stream_tx_for_wf = web_stream_tx.clone();
        let routing_db = self.db.clone();
        let routing_config = self.config.clone();

        let channel_health = self.channel_health.clone();
        // Track known (channel, chat_id) pairs from inbound messages + contacts.
        // Outbound messages to unknown pairs are blocked as a safety measure.
        let mut seed: HashSet<(String, String)> = HashSet::new();
        // Pre-seed from contact identities so proactive messages to known contacts work.
        if let Ok(identities) = self.db.all_contact_identities().await {
            for ident in &identities {
                let chat_id = if ident.channel == "whatsapp" && !ident.identifier.contains('@') {
                    // Normalize phone to WhatsApp JID
                    let num = ident.identifier.replace(['+', ' ', '-'], "");
                    format!("{num}@s.whatsapp.net")
                } else {
                    ident.identifier.clone()
                };
                seed.insert((ident.channel.clone(), chat_id));
            }
            if !seed.is_empty() {
                tracing::info!(count = seed.len(), "Pre-seeded known chat IDs from contacts");
            }
        }
        let known_chat_ids: Arc<std::sync::Mutex<HashSet<(String, String)>>> =
            Arc::new(std::sync::Mutex::new(seed));
        let known_chat_ids_for_routing = known_chat_ids.clone();

        let routing_loop = tokio::spawn(async move {
            let approval_handler = std::sync::Arc::new(approval_handler);
            let mut inbound_rate_limiter = InboundRateLimiter::new();

            // Debounce pipeline: routing loop → debounce_tx → debouncer → dispatch
            let (debounce_tx, debounce_rx) = mpsc::channel::<PreparedMessage>(100);
            {
                let registry = registry.clone();
                let routing_cfg = routing_config.clone();
                let senders = senders_for_routing.clone();
                let stream_tx = web_stream_tx.clone();
                let db = routing_db.clone();
                let locks = session_locks.clone();
                let known = known_chat_ids.clone();

                tokio::spawn(async move {
                    MessageDebouncer::new(debounce_config, debounce_rx)
                        .run(move |prepared| {
                            let registry = registry.clone();
                            let routing_cfg = routing_cfg.clone();
                            let senders = senders.clone();
                            let stream_tx = stream_tx.clone();
                            let db = db.clone();
                            let locks = locks.clone();
                            let known = known.clone();

                            tokio::spawn(async move {
                                // Route to the right agent (MAG-2 config + MAG-3 LLM)
                                let cfg = routing_cfg.read().await;
                                let agent = registry.route(
                                    prepared.ctx.contact.as_ref(),
                                    &prepared.channel_name,
                                    &cfg,
                                    &prepared.session_key,
                                    &prepared.inbound.content,
                                ).await.clone();
                                drop(cfg);
                                dispatch_to_agent(
                                    prepared, agent, senders, stream_tx, db, locks, known,
                                )
                                .await;
                            });
                        })
                        .await;
                });
            }
            #[allow(unused_mut)] // `inbound` is mutated inside #[cfg(feature = "embeddings")]
            while let Some(mut inbound) = inbound_rx.recv().await {
                // Track successful inbound message in channel health
                channel_health.record_message(&inbound.channel);

                // Register known (channel, chat_id) pair for outbound validation
                if let Ok(mut known) = known_chat_ids.lock() {
                    known.insert((inbound.channel.clone(), inbound.chat_id.clone()));
                }

                // Per-sender rate limiting (skip for system/cron messages)
                let is_system_msg = inbound
                    .metadata
                    .as_ref()
                    .map(|m| m.is_system)
                    .unwrap_or(false);
                if !is_system_msg && !inbound_rate_limiter.check(&inbound.channel, &inbound.chat_id)
                {
                    tracing::warn!(
                        channel = %inbound.channel,
                        sender = %inbound.chat_id,
                        limit = INBOUND_RATE_LIMIT,
                        window_secs = INBOUND_RATE_WINDOW_SECS,
                        "Inbound rate limit exceeded — dropping message"
                    );
                    continue;
                }

                let session_key = inbound
                    .metadata
                    .as_ref()
                    .filter(|m| m.is_system && m.scheduler_kind.as_deref() == Some("automation"))
                    .map(|m| {
                        if let Some(run_id) = m.automation_run_id.as_ref() {
                            format!(
                                "automation:{}:{run_id}",
                                m.scheduler_job_id.as_deref().unwrap_or("unknown")
                            )
                        } else {
                            format!(
                                "automation:{}",
                                m.scheduler_job_id.as_deref().unwrap_or("unknown")
                            )
                        }
                    })
                    .unwrap_or_else(|| inbound.session_key());
                let channel_name = inbound.channel.clone();
                let chat_id = inbound.chat_id.clone();
                let is_system = inbound
                    .metadata
                    .as_ref()
                    .map(|m| m.is_system)
                    .unwrap_or(false);

                tracing::info!(
                    channel = %channel_name,
                    session = %session_key,
                    "Processing inbound message"
                );

                // Skip /new — already handled by channel
                if inbound.content == "/new" || inbound.content == "/reset" {
                    continue;
                }

                // --- Unified authorization check ---
                if !is_system {
                    if let Some((pairing_required, allow_from)) = pairing_config.get(&channel_name)
                    {
                        match crate::agent::auth::check_authorization(
                            &routing_db,
                            &channel_name,
                            &inbound.sender_id,
                            allow_from,
                            *pairing_required,
                        )
                        .await
                        {
                            crate::agent::auth::AuthDecision::Authorized => {}
                            crate::agent::auth::AuthDecision::NeedsPairing => {
                                // Hand off to OTP pairing flow
                                match pairing_manager
                                    .check_sender(
                                        &channel_name,
                                        &inbound.sender_id,
                                        None,
                                        &inbound.content,
                                        true,
                                        allow_from,
                                    )
                                    .await
                                {
                                    Ok(Some(response)) => {
                                        let outbound = OutboundMessage {
                                            channel: channel_name.clone(),
                                            chat_id: chat_id.clone(),
                                            content: response,
                                            metadata: None,
                                        };
                                        route_outbound(
                                            outbound,
                                            &senders_for_routing,
                                            &known_chat_ids,
                                        )
                                        .await;
                                        continue;
                                    }
                                    Ok(None) => {} // Sender verified via user_identities
                                    Err(e) => {
                                        tracing::error!(error = %e, "Pairing check failed");
                                    }
                                }
                            }
                            crate::agent::auth::AuthDecision::Rejected => {
                                tracing::warn!(
                                    channel = %channel_name,
                                    sender = %inbound.sender_id,
                                    "Unauthorized sender — rejecting (fail-closed)"
                                );
                                continue;
                            }
                        }
                    }
                }

                // --- Email approval interception (pre-agent) ---
                // If this message comes from a notify channel and there are pending
                // drafts, handle it as an approval/reject/modify command.
                if !is_system {
                    let approval_action = approval_handler
                        .check_message(&channel_name, &chat_id, &inbound.content)
                        .await;

                    match approval_action {
                        ApprovalAction::Approve { pending } => {
                            // Send the draft as an email reply
                            let email_ch = format!("email:{}", pending.account_name);
                            let subject =
                                format!("Re: {}", pending.subject.as_deref().unwrap_or(""));
                            let draft_body = pending.draft_response.as_deref().unwrap_or_default();

                            // Build reply with subject + Message-ID for threading
                            let mut email_content = format!("Subject: {subject}\n");
                            if let Some(ref mid) = pending.message_id {
                                email_content.push_str(&format!("In-Reply-To: {mid}\n"));
                            }
                            email_content.push('\n');
                            email_content.push_str(draft_body);

                            let email_msg = OutboundMessage {
                                channel: email_ch,
                                chat_id: pending.from_address.clone(),
                                content: email_content,
                                metadata: None,
                            };
                            route_outbound(email_msg, &senders_for_routing, &known_chat_ids).await;

                            // Update status
                            let _ = routing_db
                                .update_email_pending_status(&pending.id, "sent")
                                .await;

                            // Confirm on the notify channel
                            let confirm = OutboundMessage {
                                channel: channel_name.clone(),
                                chat_id: chat_id.clone(),
                                content: format!("✅ Email inviata a {}", pending.from_address),
                                metadata: None,
                            };
                            route_outbound(confirm, &senders_for_routing, &known_chat_ids).await;

                            // Show next pending draft if any
                            show_next_pending(
                                &routing_db,
                                &senders_for_routing,
                                &channel_name,
                                &chat_id,
                                &known_chat_ids,
                            )
                            .await;
                            continue;
                        }
                        ApprovalAction::Reject { pending_id } => {
                            let _ = routing_db
                                .update_email_pending_status(&pending_id, "rejected")
                                .await;

                            let confirm = OutboundMessage {
                                channel: channel_name.clone(),
                                chat_id: chat_id.clone(),
                                content: "❌ Bozza scartata".to_string(),
                                metadata: None,
                            };
                            route_outbound(confirm, &senders_for_routing, &known_chat_ids).await;

                            show_next_pending(
                                &routing_db,
                                &senders_for_routing,
                                &channel_name,
                                &chat_id,
                                &known_chat_ids,
                            )
                            .await;
                            continue;
                        }
                        ApprovalAction::ListPending { drafts } => {
                            for (i, d) in drafts.iter().enumerate() {
                                let msg = EmailApprovalHandler::format_draft_notification(
                                    d,
                                    i + 1,
                                    drafts.len(),
                                );
                                let out = OutboundMessage {
                                    channel: channel_name.clone(),
                                    chat_id: chat_id.clone(),
                                    content: msg,
                                    metadata: None,
                                };
                                route_outbound(out, &senders_for_routing, &known_chat_ids).await;
                            }
                            continue;
                        }
                        ApprovalAction::Modify { pending, feedback } => {
                            // Build injected context for the agent to regenerate the draft
                            let injected = EmailApprovalHandler::build_modification_context(
                                &pending, &feedback,
                            );
                            let modify_agent = registry.default_agent().clone();
                            let modify_senders = senders_for_routing.clone();
                            let modify_known = known_chat_ids.clone();
                            let modify_db = routing_db.clone();
                            let modify_channel = channel_name.clone();
                            let modify_chat_id = chat_id.clone();
                            let pending_id = pending.id.clone();

                            tokio::spawn(async move {
                                let response = match modify_agent
                                    .process_message(
                                        &injected,
                                        &format!("email-modify:{pending_id}"),
                                        &modify_channel,
                                        &modify_chat_id,
                                    )
                                    .await
                                {
                                    Ok(text) => strip_reasoning(&text),
                                    Err(e) => {
                                        tracing::error!(error = %e, "Agent error (email modify)");
                                        let err_msg = OutboundMessage {
                                            channel: modify_channel,
                                            chat_id: modify_chat_id,
                                            content: format!("❌ Errore nella rigenerazione: {e}"),
                                            metadata: None,
                                        };
                                        route_outbound(err_msg, &modify_senders, &modify_known)
                                            .await;
                                        return;
                                    }
                                };

                                // Save the new draft
                                let _ = modify_db
                                    .update_email_pending_draft(&pending_id, &response)
                                    .await;

                                // Load updated record and show formatted
                                if let Ok(Some(updated)) =
                                    modify_db.load_email_pending_by_id(&pending_id).await
                                {
                                    let notify_key = format!("{modify_channel}:{modify_chat_id}");
                                    let total = modify_db
                                        .load_pending_for_notify(&notify_key)
                                        .await
                                        .map(|v| v.len())
                                        .unwrap_or(1);
                                    let msg = EmailApprovalHandler::format_draft_notification(
                                        &updated, 1, total,
                                    );
                                    let out = OutboundMessage {
                                        channel: modify_channel,
                                        chat_id: modify_chat_id,
                                        content: msg,
                                        metadata: None,
                                    };
                                    route_outbound(out, &modify_senders, &modify_known).await;
                                }
                            });
                            continue;
                        }
                        ApprovalAction::NotApplicable => {
                            // Not an approval command — continue to normal processing
                        }
                    }
                }

                // --- Chat channel approval interception (generic pending_responses) ---
                // Check if this message is an approve/reject for a chat channel draft.
                if !is_system {
                    use super::email_approval::{parse_command_and_index, Command};
                    if let Ok(pending_list) = routing_db
                        .list_pending_responses_for_notify(&channel_name, &chat_id)
                        .await
                    {
                        if !pending_list.is_empty() {
                            let lower = inbound.content.trim().to_lowercase();
                            let (cmd, idx) = parse_command_and_index(&lower);
                            let target = idx
                                .and_then(|i| pending_list.get(i))
                                .or_else(|| pending_list.first());

                            match cmd {
                                Command::Approve => {
                                    if let Some(p) = target {
                                        // Send the draft to the original channel
                                        let draft = p.draft_response.clone().unwrap_or_default();
                                        let out = OutboundMessage {
                                            channel: p.channel.clone(),
                                            chat_id: p.chat_id.clone(),
                                            content: draft,
                                            metadata: None,
                                        };
                                        route_outbound(out, &senders_for_routing, &known_chat_ids)
                                            .await;
                                        let _ = routing_db
                                            .update_pending_response_status(p.id, "approved")
                                            .await;
                                        let confirm = OutboundMessage {
                                            channel: channel_name.clone(),
                                            chat_id: chat_id.clone(),
                                            content: format!(
                                                "✅ Message sent to {} on {}",
                                                p.chat_id, p.channel
                                            ),
                                            metadata: None,
                                        };
                                        route_outbound(
                                            confirm,
                                            &senders_for_routing,
                                            &known_chat_ids,
                                        )
                                        .await;
                                    }
                                    continue;
                                }
                                Command::Reject => {
                                    if let Some(p) = target {
                                        let _ = routing_db
                                            .update_pending_response_status(p.id, "rejected")
                                            .await;
                                        let confirm = OutboundMessage {
                                            channel: channel_name.clone(),
                                            chat_id: chat_id.clone(),
                                            content: "❌ Draft discarded".to_string(),
                                            metadata: None,
                                        };
                                        route_outbound(
                                            confirm,
                                            &senders_for_routing,
                                            &known_chat_ids,
                                        )
                                        .await;
                                    }
                                    continue;
                                }
                                Command::List => {
                                    for (i, p) in pending_list.iter().enumerate() {
                                        let preview = p
                                            .draft_response
                                            .as_deref()
                                            .unwrap_or("[no draft]");
                                        let preview = if preview.len() > 300 {
                                            &preview[..300]
                                        } else {
                                            preview
                                        };
                                        let out = OutboundMessage {
                                            channel: channel_name.clone(),
                                            chat_id: chat_id.clone(),
                                            content: format!(
                                                "[{}/{}] **{}** → `{}`:\n{}",
                                                i + 1,
                                                pending_list.len(),
                                                p.channel,
                                                p.chat_id,
                                                preview
                                            ),
                                            metadata: None,
                                        };
                                        route_outbound(
                                            out,
                                            &senders_for_routing,
                                            &known_chat_ids,
                                        )
                                        .await;
                                    }
                                    continue;
                                }
                                Command::Modify => {
                                    // For chat channels, treat modify as normal message
                                    // (let it flow through to the agent)
                                }
                            }
                        }
                    }
                }

                // --- RAG file ingestion ---
                // If the message has an attachment_path, ingest it into the knowledge base
                // and notify the user.  When the user sends a file without a caption we
                // skip the agent loop (the confirmation is enough).  When a caption is
                // present we rewrite the message to hint the agent to use the knowledge tool.
                #[cfg(feature = "embeddings")]
                {
                    let mut rag_skip_agent = false;
                    if let Some(ref path) = inbound
                        .metadata
                        .as_ref()
                        .and_then(|m| m.attachment_path.clone())
                    {
                        if let Some(ref rag_mutex) = routing_rag_engine {
                            let file_path = std::path::PathBuf::from(path);
                            let file_name = file_path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| path.clone());
                            let mut rag = rag_mutex.lock().await;
                            match rag.ingest_file(&file_path, "telegram").await {
                                Ok(Some(source_id)) => {
                                    tracing::info!(source_id, file = %file_name, "RAG ingested Telegram file");
                                    let confirm = OutboundMessage {
                                        channel: channel_name.clone(),
                                        chat_id: chat_id.clone(),
                                        content: format!(
                                            "📄 Indexed \"{file_name}\" into knowledge base."
                                        ),
                                        metadata: None,
                                    };
                                    route_outbound(confirm, &senders_for_routing, &known_chat_ids)
                                        .await;

                                    // If message content is just the filename (no user caption),
                                    // skip the agent loop — the confirmation is sufficient.
                                    let content_trimmed = inbound.content.trim();
                                    if content_trimmed == file_name
                                        || content_trimmed == "[document]"
                                    {
                                        rag_skip_agent = true;
                                    } else {
                                        // User wrote a caption — rewrite content with a hint so the
                                        // agent knows to use the knowledge tool.
                                        inbound.content = format!(
                                            "[The file \"{file_name}\" has been indexed in the knowledge base (source_id={source_id}). \
                                             Use the knowledge tool with action=\"search\" to retrieve its content.]\n\n{}",
                                            inbound.content
                                        );
                                    }
                                }
                                Ok(None) => {
                                    // Duplicate file
                                    let confirm = OutboundMessage {
                                        channel: channel_name.clone(),
                                        chat_id: chat_id.clone(),
                                        content: format!("📄 \"{file_name}\" already in knowledge base (duplicate)."),
                                        metadata: None,
                                    };
                                    route_outbound(confirm, &senders_for_routing, &known_chat_ids)
                                        .await;
                                    let content_trimmed = inbound.content.trim();
                                    if content_trimmed == file_name
                                        || content_trimmed == "[document]"
                                    {
                                        rag_skip_agent = true;
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, file = %file_name, "RAG ingestion failed");
                                    let confirm = OutboundMessage {
                                        channel: channel_name.clone(),
                                        chat_id: chat_id.clone(),
                                        content: format!("❌ Failed to index \"{file_name}\": {e}"),
                                        metadata: None,
                                    };
                                    route_outbound(confirm, &senders_for_routing, &known_chat_ids)
                                        .await;
                                    let content_trimmed = inbound.content.trim();
                                    if content_trimmed == file_name
                                        || content_trimmed == "[document]"
                                    {
                                        rag_skip_agent = true;
                                    }
                                }
                            }
                            // Clean up the temp file
                            let _ = tokio::fs::remove_file(&file_path).await;
                        }
                    }
                    if rag_skip_agent {
                        continue;
                    }
                }

                // --- Contact resolution + response mode routing (CTB-3) ---
                let resolved_contact = routing_db
                    .find_contact_by_identity(&channel_name, &inbound.sender_id)
                    .await
                    .ok()
                    .flatten();
                let (contact_id, contact_response_mode) = {
                    if let Some(c) = &resolved_contact {
                        let mode = if c.response_mode != "automatic" && !c.response_mode.is_empty()
                        {
                            c.response_mode.clone()
                        } else {
                            // Channel default from config (via unified ChannelBehavior)
                            let cfg = routing_config.read().await;
                            let ch_mode = cfg.channels.behavior_for(&channel_name)
                                .map(|b| b.response_mode())
                                .unwrap_or("automatic");
                            if ch_mode.is_empty() {
                                "automatic".to_string()
                            } else {
                                ch_mode.to_string()
                            }
                        };
                        (Some(c.id), Some(mode))
                    } else {
                        (None, None)
                    }
                };

                // Apply response mode: silent and on_demand skip agent processing
                let effective_mode = contact_response_mode.as_deref().unwrap_or("automatic");
                match effective_mode {
                    "silent" => {
                        tracing::info!(channel = %channel_name, sender = %inbound.sender_id, "Contact silent mode: dropping message");
                        continue;
                    }
                    "on_demand" => {
                        tracing::info!(channel = %channel_name, sender = %inbound.sender_id, "Contact on_demand mode: saving pending");
                        let _ = routing_db
                            .insert_pending_response(
                                contact_id,
                                &channel_name,
                                &chat_id,
                                &inbound.content,
                                None,
                            )
                            .await;
                        continue;
                    }
                    _ => {} // automatic and assisted proceed
                }

                // --- Assisted mode: route agent response to notify channel for approval ---
                // Works for any channel (email, telegram, whatsapp, discord, slack).
                let approval_notify = if effective_mode == "assisted" {
                    approval_routes.get(&channel_name).cloned()
                } else if channel_name.starts_with("email:") {
                    // Email also checks metadata.requires_approval (set by email channel)
                    let requires_approval = inbound
                        .metadata
                        .as_ref()
                        .map(|m| m.requires_approval)
                        .unwrap_or(false);
                    if requires_approval {
                        approval_routes.get(&channel_name).cloned()
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Extract email metadata for draft storage (before inbound is moved)
                let email_meta = if approval_notify.is_some() {
                    inbound.metadata.as_ref().map(|m| {
                        (
                            m.email_account.clone().unwrap_or_default(),
                            m.email_subject.clone(),
                            m.email_message_id.clone(),
                        )
                    })
                } else {
                    None
                };
                let email_from = if approval_notify.is_some() {
                    Some(inbound.sender_id.clone())
                } else {
                    None
                };
                let email_body_preview = if approval_notify.is_some() {
                    let body = &inbound.content;
                    Some(body.chars().take(500).collect::<String>())
                } else {
                    None
                };
                // Build dispatch context + send through debounce pipeline
                let inbound_metadata = inbound.metadata.clone();
                let is_automation_context = inbound_metadata
                    .as_ref()
                    .and_then(|m| m.scheduler_kind.as_deref())
                    == Some("automation");
                let automation_run_id = inbound_metadata
                    .as_ref()
                    .and_then(|m| m.automation_run_id.clone());
                let automation_id = automation_run_id
                    .as_ref()
                    .and_then(|_| infer_automation_id(&inbound));
                let base_suppress_outbound =
                    should_suppress_system_outbound(inbound_metadata.as_ref(), &channel_name);
                let blocked_tools: &'static [&'static str] = if is_automation_context {
                    &["create_automation", "cron"]
                } else {
                    &[]
                };
                let thinking_override = inbound_metadata.as_ref().and_then(|m| m.thinking_override);

                let prepared = PreparedMessage {
                    inbound,
                    session_key: session_key.clone(),
                    channel_name: channel_name.clone(),
                    chat_id: chat_id.clone(),
                    ctx: DispatchContext {
                        is_system,
                        is_automation: is_automation_context,
                        approval_notify,
                        email_meta,
                        email_from,
                        email_body_preview,
                        automation_run_id,
                        automation_id,
                        suppress_outbound: base_suppress_outbound,
                        blocked_tools,
                        thinking_override,
                        inbound_metadata,
                        contact_id,
                        contact_response_mode,
                        contact: resolved_contact,
                    },
                };

                let _ = debounce_tx.send(prepared).await;
            }
        });

        // --- Cron event loop: route scheduler events into the shared inbound queue ---
        let cron_db = self.db.clone();
        let cron_loop = tokio::spawn(async move {
            while let Some(event) = cron_event_rx.recv().await {
                let kind = event.kind;
                let kind_name = match kind {
                    ScheduledKind::Cron => "cron",
                    ScheduledKind::Automation => "automation",
                };
                tracing::info!(
                    kind = kind_name,
                    job_id = %event.job_id,
                    job_name = %event.job_name,
                    "Queueing scheduler event"
                );

                let (channel, chat_id) = event
                    .deliver_to
                    .as_deref()
                    .and_then(|d| d.rsplit_once(':'))
                    .map(|(c, id)| (c.trim().to_string(), id.trim().to_string()))
                    .filter(|(c, id)| !c.is_empty() && !id.is_empty())
                    .unwrap_or_else(|| match kind {
                        ScheduledKind::Automation => ("cli".to_string(), "default".to_string()),
                        ScheduledKind::Cron => ("cron".to_string(), event.job_id.clone()),
                    });

                let inbound = InboundMessage {
                    channel,
                    sender_id: format!("system:{kind_name}"),
                    chat_id,
                    content: event.message,
                    timestamp: chrono::Utc::now(),
                    metadata: Some(MessageMetadata {
                        is_system: true,
                        scheduler_kind: Some(kind_name.to_string()),
                        scheduler_job_id: Some(event.job_id.clone()),
                        automation_run_id: event.automation_run_id.clone(),
                        ..Default::default()
                    }),
                };

                if let Err(e) = scheduler_inbound_tx.send(inbound).await {
                    tracing::error!(error = %e, kind = kind_name, "Failed to enqueue scheduler event");

                    if kind == ScheduledKind::Automation {
                        if let Some(run_id) = event.automation_run_id {
                            let result_msg = "Failed to enqueue automation run into inbound queue";
                            let _ = cron_db
                                .complete_automation_run(&run_id, "error", Some(result_msg))
                                .await;
                            let _ = cron_db
                                .update_automation(
                                    &event.job_id,
                                    AutomationUpdate {
                                        status: Some("error".to_string()),
                                        last_result: Some(Some(result_msg.to_string())),
                                        touch_last_run: true,
                                        ..Default::default()
                                    },
                                )
                                .await;
                        }
                    }
                }
            }
        });

        // --- Tool message loop: forward messages from MessageTool to channels ---
        let tool_msg_loop = if let Some(mut tool_rx) = self.tool_message_rx {
            let senders_for_tools = outbound_senders.clone();
            let known_for_tools = known_chat_ids_for_routing.clone();
            Some(tokio::spawn(async move {
                while let Some(outbound) = tool_rx.recv().await {
                    tracing::info!(
                        channel = %outbound.channel,
                        chat_id = %outbound.chat_id,
                        "Routing tool-originated message"
                    );
                    // Tool-originated messages (send_message tool) are trusted —
                    // pre-register the chat_id so route_outbound won't block them.
                    if let Ok(mut known) = known_for_tools.lock() {
                        known.insert((outbound.channel.clone(), outbound.chat_id.clone()));
                    }
                    route_outbound(outbound, &senders_for_tools, &known_for_tools).await;
                }
            }))
        } else {
            None
        };

        // --- Workflow event loop: forward workflow notifications to channels ---
        let workflow_loop = if let (Some(engine), Some(mut wf_rx)) =
            (self.workflow_engine.take(), self.workflow_event_rx.take())
        {
            let senders_for_wf = outbound_senders.clone();
            let known_for_wf = known_chat_ids_for_routing.clone();
            // Resume workflows that were interrupted by previous shutdown
            let engine_for_resume = engine.clone();
            tokio::spawn(async move {
                match engine_for_resume.resume_on_startup().await {
                    Ok(n) if n > 0 => {
                        tracing::info!(count = n, "Resumed workflows from previous session");
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to resume workflows on startup");
                    }
                    _ => {}
                }
            });

            let stream_tx_wf = web_stream_tx_for_wf.clone();
            Some(tokio::spawn(async move {
                while let Some(event) = wf_rx.recv().await {
                    let notification = event.format_notification();
                    if let Some(deliver_to) = event.deliver_to() {
                        if let Some((channel, chat_id)) = deliver_to.rsplit_once(':') {
                            // For web channel: also send structured progress event
                            if channel == "web" {
                                if let Some(ref stx) = stream_tx_wf {
                                    let progress = event.to_progress_json();
                                    let _ = stx
                                        .send(crate::bus::StreamMessage {
                                            chat_id: chat_id.to_string(),
                                            delta: progress.to_string(),
                                            done: false,
                                            event_type: Some("workflow_progress".to_string()),
                                            tool_call_data: None,
                                        })
                                        .await;
                                }
                            }

                            // Send text notification to all channels
                            let outbound = OutboundMessage {
                                channel: channel.to_string(),
                                chat_id: chat_id.to_string(),
                                content: notification,
                                metadata: None,
                            };
                            route_outbound(outbound, &senders_for_wf, &known_for_wf).await;
                        }
                    }
                }
            }))
        } else {
            None
        };

        // --- Channel command handler: hot-start channels at runtime ---
        let cmd_config = self.config.clone();
        let cmd_health = self.channel_health.clone();
        let cmd_senders = outbound_senders.clone();
        let cmd_db = self.db.clone();
        let channel_cmd_loop = tokio::spawn(async move {
            while let Some(cmd) = channel_cmd_rx.recv().await {
                match cmd {
                    ChannelCommand::Start { channel } => {
                        // Check if already running
                        if cmd_health.is_available(&channel) {
                            tracing::info!(channel = %channel, "Channel already running, skipping start");
                            continue;
                        }

                        let config = cmd_config.read().await.clone();
                        let handle = start_channel_by_name(
                            &channel,
                            &config,
                            &cmd_health,
                            cmd_inbound_tx.clone(),
                            Some(&cmd_db),
                        );
                        if let Some(ch) = handle {
                            tracing::info!(channel = %channel, "Hot-started channel via command");
                            cmd_senders.write().await.push((ch.name.clone(), ch.outbound_tx));
                            // Keep the JoinHandle alive by leaking it (it self-manages via
                            // spawn_monitored_channel's restart loop).
                            std::mem::forget(ch.handle);
                        } else {
                            tracing::warn!(channel = %channel, "Could not start channel (not enabled or missing config)");
                        }
                    }
                }
            }
        });

        // Wait for shutdown signal (Ctrl+C or SIGTERM).
        // First signal triggers graceful shutdown, second forces immediate exit.
        shutdown_signal().await;
        tracing::info!("Shutdown signal received — stopping gracefully...");
        println!("\n🧪 Shutting down gracefully (press Ctrl+C again to force)...");

        // 1. Signal the agent loop to stop current operation
        crate::agent::stop::request_stop();

        // 2. Stop accepting new messages
        routing_loop.abort();
        cron_loop.abort();
        channel_cmd_loop.abort();
        if let Some(handle) = tool_msg_loop {
            handle.abort();
        }
        if let Some(handle) = workflow_loop {
            handle.abort();
        }

        // 3. Grace period — wait up to 30s for in-flight work to complete.
        //    A second Ctrl+C forces immediate exit.
        const GRACE_SECS: u64 = 30;
        let force_shutdown = Arc::new(AtomicBool::new(false));
        let force_flag = force_shutdown.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                force_flag.store(true, Ordering::SeqCst);
                println!("Forcing shutdown...");
            }
        });

        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(GRACE_SECS);
        let mut last_progress = GRACE_SECS;
        while tokio::time::Instant::now() < deadline {
            if force_shutdown.load(Ordering::SeqCst) {
                break;
            }
            let remaining = deadline
                .saturating_duration_since(tokio::time::Instant::now())
                .as_secs();
            // Print progress every 5 seconds
            if remaining < last_progress && remaining % 5 == 0 {
                println!("🧪 Shutting down... {remaining}s remaining");
                last_progress = remaining;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
        }

        // 4. Stop all channels
        for ch in channels {
            ch.handle.abort();
            tracing::info!(channel = %ch.name, "Channel stopped");
        }

        // 5. Close database pool (drain in-flight queries)
        self.db.pool().close().await;
        tracing::info!("Database pool closed");

        // 6. Remove PID file so `homun stop` knows we're gone
        let pid_file = crate::config::Config::data_dir().join("homun.pid");
        let _ = std::fs::remove_file(&pid_file);

        tracing::info!("Gateway shutdown complete");
        println!("Goodbye! 🧪");

        Ok(())
    }
}

/// Wait for either Ctrl+C (SIGINT) or SIGTERM.
/// On non-Unix platforms, only Ctrl+C is supported.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await.ok();
    }
}

/// Dispatch a debounced/aggregated message to the agent loop.
///
/// Acquires the per-session lock so only one agent call runs per session
/// at a time.  Handles streaming (web) vs. non-streaming paths, email
/// approval routing, and automation run tracking.
async fn dispatch_to_agent(
    prepared: PreparedMessage,
    agent: Arc<AgentLoop>,
    senders: SharedOutboundSenders,
    stream_tx: Option<mpsc::Sender<StreamMessage>>,
    task_db: Database,
    session_locks: SessionLocks,
    known_chat_ids: KnownChatIds,
) {
    // Per-session serialisation: wait if another batch is still processing.
    let lock = get_session_lock(&session_locks, &prepared.session_key);
    let _guard = lock.lock().await;

    let PreparedMessage {
        inbound,
        session_key,
        channel_name,
        chat_id,
        ctx,
    } = prepared;

    let blocked_tools = ctx.blocked_tools;
    let thinking_override = ctx.thinking_override;

    // For the web channel, use streaming mode
    let (response, processing_error) = if channel_name == "web" {
        if let Some(bus_stream_tx) = stream_tx {
            let chat_id_for_stream = chat_id.clone();
            let (chunk_tx, mut chunk_rx) = mpsc::channel::<crate::provider::StreamChunk>(128);

            let bridge = tokio::spawn(async move {
                while let Some(chunk) = chunk_rx.recv().await {
                    let _ = bus_stream_tx
                        .send(StreamMessage {
                            chat_id: chat_id_for_stream.clone(),
                            delta: chunk.delta,
                            done: chunk.done,
                            event_type: chunk.event_type,
                            tool_call_data: chunk.tool_call_data,
                        })
                        .await;
                }
            });

            let result = agent
                .process_message_streaming_with_options(
                    &inbound.content,
                    &session_key,
                    &channel_name,
                    &chat_id,
                    chunk_tx,
                    blocked_tools,
                    thinking_override,
                )
                .await;

            bridge.abort();
            match result {
                Ok(text) => (text, None),
                Err(e) => {
                    tracing::error!(error = ?e, "Agent error (streaming)");
                    (
                        format!("Sorry, I encountered an error: {e}"),
                        Some(e.to_string()),
                    )
                }
            }
        } else {
            match agent
                .process_message_with_blocked_tools(
                    &inbound.content,
                    &session_key,
                    &channel_name,
                    &chat_id,
                    blocked_tools,
                )
                .await
            {
                Ok(text) => (text, None),
                Err(e) => {
                    tracing::error!(error = ?e, "Agent error");
                    (
                        format!("Sorry, I encountered an error: {e}"),
                        Some(e.to_string()),
                    )
                }
            }
        }
    } else {
        match agent
            .process_message_with_blocked_tools(
                &inbound.content,
                &session_key,
                &channel_name,
                &chat_id,
                blocked_tools,
            )
            .await
        {
            Ok(text) => (text, None),
            Err(e) => {
                tracing::error!(error = %e, "Agent error");
                (
                    format!("Sorry, I encountered an error: {e}"),
                    Some(e.to_string()),
                )
            }
        }
    };

    tracing::info!(
        channel = %channel_name,
        response_len = response.len(),
        "Agent response ready, routing to channel"
    );

    // Strip reasoning/thinking blocks for non-web channels
    let content = if channel_name == "web" {
        response
    } else {
        strip_reasoning(&response)
    };
    let run_output = content.clone();

    // If assisted mode, save draft + format notification for approval
    let outbound = if let Some((notify_ch, notify_cid)) = ctx.approval_notify {
        tracing::info!(
            source_channel = %channel_name,
            notify_channel = %notify_ch,
            "Saving draft and routing to notify channel for approval"
        );

        if channel_name.starts_with("email:") {
            // Email: use email_pending table (has email-specific fields)
            let notify_key = format!("{notify_ch}:{notify_cid}");
            let pending_id = uuid::Uuid::new_v4().to_string();
            let (account_name, subject, message_id) = ctx.email_meta.unwrap_or_default();
            let from_address = ctx.email_from.unwrap_or_default();
            let body_preview = ctx.email_body_preview;

            let row = EmailPendingRow {
                id: pending_id,
                account_name,
                from_address,
                subject,
                body_preview,
                message_id,
                draft_response: Some(content),
                status: "pending".to_string(),
                notify_session_key: Some(notify_key.clone()),
                created_at: String::new(),
                updated_at: None,
            };

            if let Err(e) = task_db.insert_email_pending(&row).await {
                tracing::error!(error = %e, "Failed to save email draft");
            }

            let total = task_db
                .load_pending_for_notify(&notify_key)
                .await
                .map(|v| v.len())
                .unwrap_or(1);

            let formatted = EmailApprovalHandler::format_draft_notification(&row, total, total);

            OutboundMessage {
                channel: notify_ch,
                chat_id: notify_cid,
                content: formatted,
                metadata: None,
            }
        } else {
            // Chat channels: use generic pending_responses table
            let preview = if content.len() > 500 { &content[..500] } else { &content };
            let _ = task_db
                .insert_pending_response_with_notify(
                    ctx.contact_id,
                    &channel_name,
                    &chat_id,
                    &inbound.content,
                    Some(&content),
                    Some(&notify_ch),
                    Some(&notify_cid),
                )
                .await;

            let formatted = format!(
                "📨 **{ch}** draft for `{sender}`:\n\n{draft}\n\n\
                 Reply **ok** to send, **rifiuta** to discard, or type feedback to modify.",
                ch = channel_name,
                sender = chat_id,
                draft = preview,
            );

            OutboundMessage {
                channel: notify_ch,
                chat_id: notify_cid,
                content: formatted,
                metadata: None,
            }
        }
    } else {
        OutboundMessage {
            channel: channel_name.clone(),
            chat_id: chat_id.clone(),
            content,
            metadata: build_outbound_meta(inbound.metadata.as_ref()),
        }
    };

    let mut suppress_outbound = ctx.suppress_outbound;
    let mut trigger_note: Option<String> = None;

    if processing_error.is_none() {
        if let (Some(run_id), Some(automation_id)) = (
            ctx.automation_run_id.as_deref(),
            ctx.automation_id.as_deref(),
        ) {
            if let Ok(Some(automation)) = task_db.load_automation(automation_id).await {
                let previous_result = task_db
                    .load_last_successful_automation_result(automation_id, Some(run_id))
                    .await
                    .ok()
                    .flatten();
                let (should_notify, note) = evaluate_automation_trigger(
                    &automation.trigger_kind,
                    automation.trigger_value.as_deref(),
                    previous_result.as_deref(),
                    &run_output,
                );
                if !should_notify {
                    suppress_outbound = true;
                    trigger_note = note;
                }
            }
        }
    }

    if !suppress_outbound {
        route_outbound(outbound, &senders, &known_chat_ids).await;
    }

    if let Some(run_id) = ctx.automation_run_id {
        let run_result = match processing_error.as_deref() {
            Some(e) => format!("Run failed: {e}"),
            None => run_output.clone(),
        };
        let run_status = if processing_error.is_some() {
            "error"
        } else {
            "success"
        };
        let automation_status = if processing_error.is_some() {
            "error"
        } else {
            "active"
        };

        if let Err(e) = task_db
            .complete_automation_run(&run_id, run_status, Some(&run_result))
            .await
        {
            tracing::error!(
                error = %e,
                run_id = %run_id,
                "Failed to complete automation run"
            );
        }

        if let Some(automation_id) = ctx.automation_id {
            let latest_result = if processing_error.is_some() {
                truncate_for_status(&run_result, 500)
            } else if let Some(note) = trigger_note.as_deref() {
                format!("{note} | output: {}", truncate_for_status(&run_result, 300))
            } else {
                truncate_for_status(&run_result, 500)
            };
            if let Err(e) = task_db
                .update_automation(
                    &automation_id,
                    AutomationUpdate {
                        status: Some(automation_status.to_string()),
                        last_result: Some(Some(latest_result)),
                        touch_last_run: true,
                        ..Default::default()
                    },
                )
                .await
            {
                tracing::error!(
                    error = %e,
                    automation_id = %automation_id,
                    "Failed to update automation status after run"
                );
            }
        }
    }
}

fn should_suppress_system_outbound(metadata: Option<&MessageMetadata>, channel: &str) -> bool {
    if channel != "cron" {
        return false;
    }
    let Some(meta) = metadata else {
        return false;
    };
    meta.is_system && meta.scheduler_kind.as_deref() == Some("cron")
}

fn truncate_for_status(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let clipped: String = text.chars().take(max_chars).collect();
    format!("{clipped}...")
}

fn evaluate_automation_trigger(
    trigger_kind: &str,
    trigger_value: Option<&str>,
    previous_result: Option<&str>,
    current_result: &str,
) -> (bool, Option<String>) {
    match trigger_kind.trim().to_ascii_lowercase().as_str() {
        "always" => (true, None),
        "on_change" | "changed" => {
            let Some(previous) = previous_result else {
                return (
                    true,
                    Some("No previous successful run; notifying".to_string()),
                );
            };
            let previous_norm = normalize_for_compare(previous);
            let current_norm = normalize_for_compare(current_result);
            if previous_norm == current_norm {
                (
                    false,
                    Some("Trigger on_change not matched (result unchanged)".to_string()),
                )
            } else {
                (true, None)
            }
        }
        "contains" => {
            let needle = trigger_value.unwrap_or("").trim();
            if needle.is_empty() {
                return (
                    true,
                    Some("Trigger contains misconfigured; defaulting to notify".to_string()),
                );
            }
            let haystack = current_result.to_ascii_lowercase();
            if haystack.contains(&needle.to_ascii_lowercase()) {
                (true, None)
            } else {
                (
                    false,
                    Some(format!("Trigger contains not matched ('{needle}')")),
                )
            }
        }
        other => (
            true,
            Some(format!("Unknown trigger '{other}'; defaulting to notify")),
        ),
    }
}

fn normalize_for_compare(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn infer_automation_id(inbound: &InboundMessage) -> Option<String> {
    if let Some(id) = inbound
        .metadata
        .as_ref()
        .and_then(|m| m.scheduler_job_id.clone())
    {
        return Some(id);
    }
    inbound
        .sender_id
        .strip_prefix("automation:")
        .map(|s| s.to_string())
}

/// Start a channel by name from the current config.
/// Returns `None` if the channel is not enabled or config is incomplete.
fn start_channel_by_name(
    name: &str,
    config: &Config,
    health: &Arc<ChannelHealthTracker>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    #[allow(unused_variables)]
    db: Option<&Database>,
) -> Option<ChannelHandle> {
    /// Resolve an encrypted token from the vault.
    fn resolve_token(channel_key: &str, token: &str) -> String {
        if token == "***ENCRYPTED***" || token.is_empty() {
            if let Ok(secrets) = crate::storage::global_secrets() {
                let key = crate::storage::SecretKey::channel_token(channel_key);
                if let Ok(Some(real)) = secrets.get(&key) {
                    return real;
                }
            }
            String::new()
        } else {
            token.to_string()
        }
    }

    match name {
        #[cfg(feature = "channel-telegram")]
        "telegram" if config.channels.telegram.enabled => {
            let mut cfg = config.channels.telegram.clone();
            cfg.token = resolve_token("telegram", &cfg.token);
            if cfg.token.is_empty() || cfg.token == "***ENCRYPTED***" {
                return None;
            }
            Some(spawn_monitored_channel(
                "telegram", health, inbound_tx,
                move || Box::new(TelegramChannel::new(cfg.clone())),
            ))
        }
        #[cfg(feature = "channel-discord")]
        "discord" if config.channels.discord.enabled => {
            let mut cfg = config.channels.discord.clone();
            cfg.token = resolve_token("discord", &cfg.token);
            if cfg.token.is_empty() || cfg.token == "***ENCRYPTED***" {
                return None;
            }
            let health_for_dc = health.clone();
            Some(spawn_monitored_channel(
                "discord", health, inbound_tx,
                move || Box::new(DiscordChannel::new(cfg.clone()).with_health(health_for_dc.clone())),
            ))
        }
        #[cfg(feature = "channel-whatsapp")]
        "whatsapp" if config.channels.whatsapp.enabled => {
            let cfg = config.channels.whatsapp.clone();
            Some(spawn_monitored_channel(
                "whatsapp", health, inbound_tx,
                move || Box::new(WhatsAppChannel::new(cfg.clone())),
            ))
        }
        "slack" if config.channels.slack.enabled => {
            let mut cfg = config.channels.slack.clone();
            cfg.token = resolve_token("slack", &cfg.token);
            cfg.app_token = resolve_token("slack_app", &cfg.app_token);
            if cfg.token.is_empty() || cfg.token == "***ENCRYPTED***" {
                return None;
            }
            Some(spawn_monitored_channel(
                "slack", health, inbound_tx,
                move || Box::new(SlackChannel::new(cfg.clone())),
            ))
        }
        name if name.starts_with("mcp:") => {
            let mcp_name = name.strip_prefix("mcp:").unwrap_or("").to_string();
            let cfg = config.channels.mcp.get(&mcp_name)?;
            if !cfg.enabled { return None; }
            let cfg = cfg.clone();
            let ch_name = name.to_string();
            Some(spawn_monitored_channel(
                name, health, inbound_tx,
                move || Box::new(crate::channels::McpChannel::new(ch_name.clone(), cfg.clone())),
            ))
        }
        _ => None,
    }
}

/// After approving/rejecting a draft, show the next pending one if any.
async fn show_next_pending(
    db: &Database,
    senders: &SharedOutboundSenders,
    channel: &str,
    chat_id: &str,
    known: &KnownChatIds,
) {
    let notify_key = format!("{channel}:{chat_id}");
    if let Ok(remaining) = db.load_pending_for_notify(&notify_key).await {
        if let Some(next) = remaining.first() {
            let msg = EmailApprovalHandler::format_draft_notification(next, 1, remaining.len());
            let out = OutboundMessage {
                channel: channel.to_string(),
                chat_id: chat_id.to_string(),
                content: msg,
                metadata: None,
            };
            route_outbound(out, senders, known).await;
        }
    }
}

/// Route an outbound message to the correct channel.
///
/// Supports prefixed channel names: `email:lavoro` is routed to the `email` sender
/// (the email channel handles per-account dispatch internally).
/// Known (channel, chat_id) pairs from inbound messages.
type KnownChatIds = Arc<std::sync::Mutex<HashSet<(String, String)>>>;

/// Per-sender inbound rate limiter. Sliding window of message timestamps.
const INBOUND_RATE_LIMIT: usize = 10; // max messages per window
const INBOUND_RATE_WINDOW_SECS: u64 = 60; // window duration

struct InboundRateLimiter {
    windows: HashMap<(String, String), std::collections::VecDeque<std::time::Instant>>,
}

impl InboundRateLimiter {
    fn new() -> Self {
        Self {
            windows: HashMap::new(),
        }
    }

    /// Returns true if the message is allowed, false if rate-limited.
    fn check(&mut self, channel: &str, sender: &str) -> bool {
        let now = std::time::Instant::now();
        let cutoff = now - std::time::Duration::from_secs(INBOUND_RATE_WINDOW_SECS);
        let key = (channel.to_string(), sender.to_string());
        let window = self.windows.entry(key).or_default();

        // Remove expired entries
        while window.front().map(|t| *t < cutoff).unwrap_or(false) {
            window.pop_front();
        }

        if window.len() >= INBOUND_RATE_LIMIT {
            return false; // rate limited
        }

        window.push_back(now);
        true
    }
}

async fn route_outbound(
    outbound: OutboundMessage,
    senders: &SharedOutboundSenders,
    known: &KnownChatIds,
) {
    let channel_name = outbound.channel.clone();

    // Safety: verify this (channel, chat_id) was seen from an inbound message.
    // System messages (cron, automations) use synthetic chat_ids — always allow those.
    let is_system = outbound.chat_id.starts_with("cron:")
        || outbound.chat_id.starts_with("automation:")
        || outbound.chat_id.starts_with("workflow:")
        || outbound.chat_id.starts_with("heartbeat:")
        || channel_name == "web";
    if !is_system {
        let is_known = known
            .lock()
            .map(|k| k.contains(&(channel_name.clone(), outbound.chat_id.clone())))
            .unwrap_or(true); // if lock poisoned, allow (fail-open)
        if !is_known {
            tracing::warn!(
                channel = %channel_name,
                chat_id = %outbound.chat_id,
                "Blocked outbound to unknown chat_id (never seen from inbound)"
            );
            return;
        }
    }

    // For prefixed channels like "email:lavoro", the sender is registered as "email"
    let sender_key = if channel_name.contains(':') {
        channel_name
            .split(':')
            .next()
            .unwrap_or(&channel_name)
            .to_string()
    } else {
        channel_name.clone()
    };

    let senders_guard = senders.read().await;
    let mut routed = false;
    for (name, tx) in senders_guard.iter() {
        if *name == sender_key || *name == channel_name {
            if let Err(e) = tx.send(outbound).await {
                tracing::error!(
                    channel = %name,
                    error = %e,
                    "Failed to route outbound message"
                );
            } else {
                tracing::info!(channel = %name, "Outbound message routed");
            }
            routed = true;
            break;
        }
    }
    if !routed {
        tracing::error!(channel = %channel_name, "No sender found for channel");
    }
}
