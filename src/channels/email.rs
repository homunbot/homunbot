//! Email channel — IMAP IDLE for instant push notifications, SMTP for outbound
//!
//! Implements the Channel trait for email integration.
//! Inspired by ZeroClaw's implementation, adapted for Homun architecture.

#![allow(clippy::too_many_lines)]

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

#[cfg(feature = "channel-email")]
use async_imap::extensions::idle::IdleResponse;
#[cfg(feature = "channel-email")]
use async_imap::types::Fetch;
#[cfg(feature = "channel-email")]
use async_imap::Session;
#[cfg(feature = "channel-email")]
use futures::TryStreamExt;
#[cfg(feature = "channel-email")]
use lettre::message::SinglePart;
#[cfg(feature = "channel-email")]
use lettre::transport::smtp::authentication::Credentials;
#[cfg(feature = "channel-email")]
use lettre::{Message, SmtpTransport, Transport};
#[cfg(feature = "channel-email")]
use mail_parser::MessageParser;
#[cfg(feature = "channel-email")]
use rustls_pki_types::DnsName;
#[cfg(feature = "channel-email")]
use tokio::net::TcpStream;
#[cfg(feature = "channel-email")]
use tokio_rustls::client::TlsStream;
#[cfg(feature = "channel-email")]
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
#[cfg(feature = "channel-email")]
use tokio_rustls::TlsConnector;

use super::traits::Channel;
use crate::bus::{InboundMessage, OutboundMessage};
use crate::config::EmailConfig;

/// Type alias for IMAP session with TLS
#[cfg(feature = "channel-email")]
type ImapSession = Session<TlsStream<TcpStream>>;

/// Email channel — IMAP IDLE for instant push notifications, SMTP for outbound
pub struct EmailChannel {
    config: EmailConfig,
    seen_messages: Arc<Mutex<HashSet<String>>>,
}

impl EmailChannel {
    pub fn new(config: EmailConfig) -> Self {
        Self {
            config,
            seen_messages: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Check if a sender email is in the allowlist
    pub fn is_sender_allowed(&self, email: &str) -> bool {
        if self.config.allow_from.is_empty() {
            return false; // Empty = deny all
        }
        if self.config.allow_from.iter().any(|a| a == "*") {
            return true; // Wildcard = allow all
        }
        let email_lower = email.to_lowercase();
        self.config.allow_from.iter().any(|allowed| {
            if allowed.starts_with('@') {
                // Domain match with @ prefix: "@example.com"
                email_lower.ends_with(&allowed.to_lowercase())
            } else if allowed.contains('@') {
                // Full email address match
                allowed.eq_ignore_ascii_case(email)
            } else {
                // Domain match without @ prefix: "example.com"
                email_lower.ends_with(&format!("@{}", allowed.to_lowercase()))
            }
        })
    }

    /// Strip HTML tags from content (basic)
    pub fn strip_html(html: &str) -> String {
        let mut result = String::new();
        let mut in_tag = false;
        for ch in html.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => result.push(ch),
                _ => {}
            }
        }
        let mut normalized = String::with_capacity(result.len());
        for word in result.split_whitespace() {
            if !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push_str(word);
        }
        normalized
    }

    /// Resolve password from vault if needed
    fn resolve_password(&self) -> String {
        if self.config.password == "***ENCRYPTED***" {
            if let Ok(secrets) = crate::storage::global_secrets() {
                let key = crate::storage::SecretKey::channel_token("email");
                if let Ok(Some(password)) = secrets.get(&key) {
                    return password;
                }
            }
            // Fallback to config password
            warn!("Email password marked as encrypted but not found in vault");
        }
        self.config.password.clone()
    }
}

#[async_trait]
impl Channel for EmailChannel {
    fn name(&self) -> &str {
        "email"
    }

    async fn start(
        &self,
        inbound_tx: mpsc::Sender<InboundMessage>,
        mut outbound_rx: mpsc::Receiver<OutboundMessage>,
    ) -> Result<()> {
        let password = self.resolve_password();

        if !self.config.is_configured() && password.is_empty() {
            anyhow::bail!("Email channel not configured");
        }

        info!(
            "Starting email channel: {} (IMAP: {}:{}, SMTP: {}:{})",
            self.config.from_address,
            self.config.imap_host,
            self.config.imap_port,
            self.config.smtp_host,
            self.config.smtp_port
        );

        // Clone what we need for the tasks
        let config = self.config.clone();
        let seen_messages = self.seen_messages.clone();
        let inbound_tx_clone = inbound_tx.clone();
        let password_imap = password.clone();
        let password_smtp = password.clone();

        // Spawn IMAP listener task
        let imap_handle = tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);
            let max_backoff = Duration::from_secs(60);

            loop {
                match run_imap_session(&config, &password_imap, &inbound_tx_clone, &seen_messages)
                    .await
                {
                    Ok(()) => {
                        // Clean exit
                        info!("IMAP session ended cleanly");
                        return Ok::<(), anyhow::Error>(());
                    }
                    Err(e) => {
                        error!(
                            "IMAP session error: {}. Reconnecting in {:?}...",
                            e, backoff
                        );
                        sleep(backoff).await;
                        backoff = std::cmp::min(backoff * 2, max_backoff);
                    }
                }
            }
        });

        // Spawn SMTP sender task
        let smtp_config = self.config.clone();
        let smtp_handle = tokio::spawn(async move {
            while let Some(msg) = outbound_rx.recv().await {
                if msg.channel != "email" {
                    continue;
                }
                if let Err(e) = send_email(&smtp_config, &password_smtp, &msg).await {
                    error!("Failed to send email: {}", e);
                }
            }
            Ok::<(), anyhow::Error>(())
        });

        // Wait for either task to complete
        tokio::select! {
            result = imap_handle => {
                warn!("IMAP task finished: {:?}", result);
            }
            result = smtp_handle => {
                warn!("SMTP task finished: {:?}", result);
            }
        }

        Ok(())
    }
}

/// Run a single IMAP session (with IDLE support)
#[cfg(feature = "channel-email")]
async fn run_imap_session(
    config: &EmailConfig,
    password: &str,
    inbound_tx: &mpsc::Sender<InboundMessage>,
    seen_messages: &Arc<Mutex<HashSet<String>>>,
) -> Result<()> {
    let addr = format!("{}:{}", config.imap_host, config.imap_port);
    debug!("Connecting to IMAP server at {}", addr);

    // Connect TCP
    let tcp = TcpStream::connect(&addr)
        .await
        .context("Failed to connect to IMAP server")?;

    // Establish TLS using rustls
    let certs = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.into(),
    };
    let rustls_config = ClientConfig::builder()
        .with_root_certificates(certs)
        .with_no_client_auth();
    let tls_connector: TlsConnector = Arc::new(rustls_config).into();
    let sni: DnsName = config
        .imap_host
        .clone()
        .try_into()
        .context("Invalid IMAP hostname")?;
    let stream = tls_connector
        .connect(sni.into(), tcp)
        .await
        .context("TLS handshake failed")?;

    // Create IMAP client
    let client = async_imap::Client::new(stream);

    // Login
    let mut session: ImapSession = client
        .login(&config.username, password)
        .await
        .map_err(|(e, _)| anyhow!("IMAP login failed: {}", e))?;

    debug!("IMAP login successful");

    // Select mailbox
    session
        .select(&config.imap_folder)
        .await
        .context("Failed to select mailbox")?;

    info!(
        "Email IDLE listening on {} (instant push enabled)",
        config.imap_folder
    );

    // Check for existing unseen messages first
    process_unseen(&mut session, config, inbound_tx, seen_messages).await?;

    // IDLE loop
    let idle_timeout = Duration::from_secs(config.idle_timeout_secs);

    loop {
        // Start IDLE mode
        let mut idle = session.idle();
        idle.init().await.context("Failed to initialize IDLE")?;

        debug!("Entering IMAP IDLE mode");

        let (wait_future, _stop_source) = idle.wait();
        let result = timeout(idle_timeout, wait_future).await;

        match result {
            Ok(Ok(response)) => {
                debug!("IDLE response: {:?}", response);
                // Done with IDLE, return session to normal mode
                session = idle.done().await.context("Failed to exit IDLE mode")?;

                match response {
                    IdleResponse::NewData(_) => {
                        debug!("New mail notification received");
                        process_unseen(&mut session, config, inbound_tx, seen_messages).await?;
                    }
                    IdleResponse::Timeout => {
                        // Re-check after IDLE timeout (defensive)
                        process_unseen(&mut session, config, inbound_tx, seen_messages).await?;
                    }
                    IdleResponse::ManualInterrupt => {
                        info!("IDLE interrupted, exiting");
                        let _ = session.logout().await;
                        return Ok(());
                    }
                }
            }
            Ok(Err(e)) => {
                let _ = idle.done().await;
                return Err(anyhow!("IDLE error: {}", e));
            }
            Err(_) => {
                // Timeout - RFC 2177 recommends restarting IDLE every 29 minutes
                debug!("IDLE timeout reached, re-establishing");
                session = idle.done().await.context("Failed to exit IDLE mode")?;
                process_unseen(&mut session, config, inbound_tx, seen_messages).await?;
            }
        }
    }
}

/// Non-IDLE fallback (for IMAP servers without IDLE support)
#[cfg(not(feature = "channel-email"))]
async fn run_imap_session(
    _config: &EmailConfig,
    _password: &str,
    _inbound_tx: &mpsc::Sender<InboundMessage>,
    _seen_messages: &Arc<Mutex<HashSet<String>>>,
) -> Result<()> {
    anyhow::bail!("Email channel requires 'channel-email' feature to be enabled");
}

/// Fetch and process unseen messages
#[cfg(feature = "channel-email")]
async fn process_unseen(
    session: &mut ImapSession,
    config: &EmailConfig,
    inbound_tx: &mpsc::Sender<InboundMessage>,
    seen_messages: &Arc<Mutex<HashSet<String>>>,
) -> Result<()> {
    // Search for unseen messages
    let uids = session
        .uid_search("UNSEEN")
        .await
        .context("UID SEARCH failed")?;
    if uids.is_empty() {
        return Ok(());
    }

    debug!("Found {} unseen messages", uids.len());

    let uid_set: String = uids
        .iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");

    // Fetch message bodies
    let fetch_stream = session
        .uid_fetch(&uid_set, "RFC822")
        .await
        .context("UID FETCH failed")?;
    let messages: Vec<Fetch> = fetch_stream
        .try_collect()
        .await
        .context("FETCH collect failed")?;

    for msg in messages {
        let uid = msg.uid.unwrap_or(0);
        if let Some(body) = msg.body() {
            if let Some(parsed) = MessageParser::default().parse(body) {
                // Extract sender
                let sender = parsed
                    .from()
                    .and_then(|addr| addr.first())
                    .and_then(|a| a.address())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "unknown".into());

                // Extract subject and body
                let subject = parsed.subject().unwrap_or("(no subject)").to_string();
                let body_text = if let Some(text) = parsed.body_text(0) {
                    text.to_string()
                } else if let Some(html) = parsed.body_html(0) {
                    EmailChannel::strip_html(html.as_ref())
                } else {
                    "(no readable content)".to_string()
                };

                let content = format!("Subject: {}\n\n{}", subject, body_text);
                let msg_id = parsed
                    .message_id()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("email-{}-{}", uid, chrono::Utc::now().timestamp()));

                // Check allowlist
                let config_clone = config.clone();
                let is_allowed = {
                    let email_channel = EmailChannel::new(config_clone);
                    email_channel.is_sender_allowed(&sender)
                };

                if !is_allowed {
                    warn!("Blocked email from {}", sender);
                    continue;
                }

                // Check if already seen
                let is_new = {
                    let mut seen = seen_messages.lock().await;
                    seen.insert(msg_id.clone())
                };
                if !is_new {
                    continue;
                }

                let inbound_msg = InboundMessage {
                    channel: "email".to_string(),
                    sender_id: sender.clone(),
                    chat_id: sender.clone(), // Use sender email as chat_id
                    content,
                    timestamp: Utc::now(),
                };

                if inbound_tx.send(inbound_msg).await.is_err() {
                    warn!("Channel closed, stopping email processing");
                    return Ok(());
                }
            }
        }
    }

    // Mark fetched messages as seen
    if !uids.is_empty() {
        let _ = session.uid_store(&uid_set, "+FLAGS (\\Seen)").await;
    }

    Ok(())
}

/// Send an email via SMTP
#[cfg(feature = "channel-email")]
async fn send_email(config: &EmailConfig, password: &str, msg: &OutboundMessage) -> Result<()> {
    // Parse subject and body from content
    let (subject, body) = if msg.content.starts_with("Subject: ") {
        if let Some(pos) = msg.content.find('\n') {
            (&msg.content[9..pos], msg.content[pos + 1..].trim())
        } else {
            ("Homun Response", msg.content.as_str())
        }
    } else {
        ("Homun Response", msg.content.as_str())
    };

    let email = Message::builder()
        .from(
            config
                .from_address
                .parse()
                .context("Invalid from address")?,
        )
        .to(msg.chat_id.parse().context("Invalid recipient address")?)
        .subject(subject)
        .singlepart(SinglePart::plain(body.to_string()))
        .context("Failed to build email")?;

    let creds = Credentials::new(config.username.clone(), password.to_string());

    let transport = if config.smtp_tls {
        SmtpTransport::relay(&config.smtp_host)
            .context("Failed to create SMTP transport")?
            .port(config.smtp_port)
            .credentials(creds)
            .build()
    } else {
        SmtpTransport::builder_dangerous(&config.smtp_host)
            .port(config.smtp_port)
            .credentials(creds)
            .build()
    };

    transport.send(&email).context("Failed to send email")?;

    info!("Email sent to {}", msg.chat_id);
    Ok(())
}

#[cfg(not(feature = "channel-email"))]
async fn send_email(_config: &EmailConfig, _password: &str, _msg: &OutboundMessage) -> Result<()> {
    anyhow::bail!("Email channel requires 'channel-email' feature to be enabled");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> EmailConfig {
        EmailConfig {
            enabled: true,
            imap_host: "imap.example.com".to_string(),
            imap_port: 993,
            imap_folder: "INBOX".to_string(),
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 465,
            smtp_tls: true,
            username: "bot@example.com".to_string(),
            password: "password".to_string(),
            from_address: "bot@example.com".to_string(),
            idle_timeout_secs: 1740,
            allow_from: vec![],
        }
    }

    #[test]
    fn test_email_channel_name() {
        let channel = EmailChannel::new(test_config());
        assert_eq!(channel.name(), "email");
    }

    #[test]
    fn test_is_sender_allowed_empty_list() {
        let channel = EmailChannel::new(test_config());
        assert!(!channel.is_sender_allowed("anyone@example.com"));
    }

    #[test]
    fn test_is_sender_allowed_wildcard() {
        let mut config = test_config();
        config.allow_from = vec!["*".to_string()];
        let channel = EmailChannel::new(config);
        assert!(channel.is_sender_allowed("anyone@example.com"));
    }

    #[test]
    fn test_is_sender_allowed_specific_email() {
        let mut config = test_config();
        config.allow_from = vec!["allowed@example.com".to_string()];
        let channel = EmailChannel::new(config);
        assert!(channel.is_sender_allowed("allowed@example.com"));
        assert!(!channel.is_sender_allowed("other@example.com"));
    }

    #[test]
    fn test_is_sender_allowed_domain() {
        let mut config = test_config();
        config.allow_from = vec!["@example.com".to_string()];
        let channel = EmailChannel::new(config);
        assert!(channel.is_sender_allowed("user@example.com"));
        assert!(!channel.is_sender_allowed("user@other.com"));
    }

    #[test]
    fn test_is_sender_allowed_domain_without_at() {
        let mut config = test_config();
        config.allow_from = vec!["example.com".to_string()];
        let channel = EmailChannel::new(config);
        assert!(channel.is_sender_allowed("user@example.com"));
        assert!(!channel.is_sender_allowed("user@other.com"));
    }

    #[test]
    fn test_strip_html() {
        assert_eq!(EmailChannel::strip_html("<p>Hello</p>"), "Hello");
        assert_eq!(
            EmailChannel::strip_html("<div><p>Hello <strong>World</strong></p></div>"),
            "Hello World"
        );
        assert_eq!(EmailChannel::strip_html("No tags here"), "No tags here");
    }

    #[test]
    fn test_config_is_configured() {
        let config = test_config();
        assert!(config.is_configured());

        let mut empty_config = EmailConfig::default();
        assert!(!empty_config.is_configured());
    }
}
