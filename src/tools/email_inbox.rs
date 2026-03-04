use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

use super::registry::{get_optional_bool, get_optional_string, Tool, ToolContext, ToolResult};
use crate::config::{Config, EmailAccountConfig};

#[cfg(feature = "channel-email")]
use async_imap::types::Fetch;
#[cfg(feature = "channel-email")]
use futures::TryStreamExt;
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

#[cfg(feature = "channel-email")]
type ImapSession = async_imap::Session<TlsStream<TcpStream>>;

/// Tool to read emails from configured IMAP accounts.
pub struct ReadEmailInboxTool;

impl ReadEmailInboxTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ReadEmailInboxTool {
    fn name(&self) -> &str {
        "read_email_inbox"
    }

    fn description(&self) -> &str {
        "Read emails from configured IMAP accounts. Defaults to unread messages \
         on the default account. Use this for inbox digests and email monitoring automations."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "account": {
                    "type": "string",
                    "description": "Email account name from config (e.g. 'default', 'lavoro'). Optional."
                },
                "folder": {
                    "type": "string",
                    "description": "Mailbox folder. Optional; defaults to the account folder (usually INBOX)."
                },
                "limit": {
                    "type": "integer",
                    "description": "Max messages to return (1-50). Default 10."
                },
                "include_read": {
                    "type": "boolean",
                    "description": "If true, search all messages. If false, unread only (default)."
                },
                "mark_as_seen": {
                    "type": "boolean",
                    "description": "If true, mark returned messages as read after fetching. Default false."
                },
                "max_body_chars": {
                    "type": "integer",
                    "description": "Max body chars per email in output (100-5000). Default 1200."
                }
            }
        })
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        #[cfg(not(feature = "channel-email"))]
        {
            let _ = args;
            return Ok(ToolResult::error(
                "Email inbox reading requires the 'channel-email' feature.",
            ));
        }

        #[cfg(feature = "channel-email")]
        {
            let requested_account = get_optional_string(&args, "account");
            let folder_override = get_optional_string(&args, "folder");
            let include_read = get_optional_bool(&args, "include_read").unwrap_or(false);
            let mark_as_seen = get_optional_bool(&args, "mark_as_seen").unwrap_or(false);
            let limit = get_bounded_u64(&args, "limit", 10, 1, 50) as usize;
            let max_body_chars = get_bounded_u64(&args, "max_body_chars", 1200, 100, 5000) as usize;

            let mut config = Config::load().context("Failed to load config")?;
            config.channels.migrate_legacy_email();

            let (account_name, account_cfg) =
                select_account(&config, requested_account.as_deref())?;
            let password = resolve_account_password(&account_name, &account_cfg);
            if password.is_empty() || password == "***ENCRYPTED***" {
                return Ok(ToolResult::error(format!(
                    "Email account '{account_name}' has no usable password. \
                     Update it in channel settings."
                )));
            }

            let folder = folder_override.unwrap_or_else(|| account_cfg.imap_folder.clone());
            let emails = fetch_inbox(
                &account_cfg,
                &password,
                &folder,
                include_read,
                limit,
                max_body_chars,
                mark_as_seen,
            )
            .await?;

            let payload = json!({
                "account": account_name,
                "folder": folder,
                "query": if include_read { "ALL" } else { "UNSEEN" },
                "count": emails.len(),
                "emails": emails
            });
            let output = serde_json::to_string_pretty(&payload)
                .context("Failed to serialize inbox result")?;
            Ok(ToolResult::success(output))
        }
    }
}

fn get_bounded_u64(args: &Value, key: &str, default: u64, min: u64, max: u64) -> u64 {
    let value = args.get(key).and_then(Value::as_u64).unwrap_or(default);
    value.clamp(min, max)
}

fn select_account(
    config: &Config,
    requested: Option<&str>,
) -> Result<(String, EmailAccountConfig)> {
    if let Some(name) = requested {
        if let Some(acc) = config.channels.emails.get(name) {
            if !acc.enabled {
                bail!("Email account '{name}' is disabled");
            }
            if !acc.is_configured() {
                bail!("Email account '{name}' is not fully configured");
            }
            return Ok((name.to_string(), acc.clone()));
        }

        let available = active_account_names(config);
        bail!(
            "Email account '{name}' not found. Available accounts: {}",
            available.join(", ")
        );
    }

    if let Some(acc) = config.channels.emails.get("default") {
        if acc.enabled && acc.is_configured() {
            return Ok(("default".to_string(), acc.clone()));
        }
    }

    let mut active = config.channels.active_email_accounts();
    if active.is_empty() {
        bail!("No enabled and configured email accounts found");
    }
    active.sort_by(|(a, _), (b, _)| a.cmp(b));
    let (name, acc) = active[0];
    Ok((name.clone(), acc.clone()))
}

fn active_account_names(config: &Config) -> Vec<String> {
    let mut names: Vec<String> = config
        .channels
        .active_email_accounts()
        .into_iter()
        .map(|(name, _)| name.clone())
        .collect();
    names.sort();
    names
}

fn resolve_account_password(account_name: &str, config: &EmailAccountConfig) -> String {
    if config.password == "***ENCRYPTED***" {
        if let Ok(secrets) = crate::storage::global_secrets() {
            let key = crate::storage::SecretKey::custom(&format!("email.{account_name}.password"));
            if let Ok(Some(password)) = secrets.get(&key) {
                return password;
            }
            let legacy_key = crate::storage::SecretKey::channel_token("email");
            if let Ok(Some(password)) = secrets.get(&legacy_key) {
                return password;
            }
        }
        return String::new();
    }
    config.password.clone()
}

fn strip_html(html: &str) -> String {
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
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let char_count = input.chars().count();
    if char_count <= max_chars {
        return input.to_string();
    }
    let mut out: String = input.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

#[cfg(feature = "channel-email")]
async fn open_imap_session(config: &EmailAccountConfig, password: &str) -> Result<ImapSession> {
    let addr = format!("{}:{}", config.imap_host, config.imap_port);
    let tcp = TcpStream::connect(&addr)
        .await
        .with_context(|| format!("Failed to connect to IMAP server {addr}"))?;

    let certs = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.into(),
    };
    let rustls_config = ClientConfig::builder()
        .with_root_certificates(certs)
        .with_no_client_auth();
    let tls_connector: TlsConnector = std::sync::Arc::new(rustls_config).into();
    let sni: DnsName = config
        .imap_host
        .clone()
        .try_into()
        .context("Invalid IMAP hostname")?;
    let stream = tls_connector
        .connect(sni.into(), tcp)
        .await
        .context("IMAP TLS handshake failed")?;

    let client = async_imap::Client::new(stream);
    let session = client
        .login(&config.username, password)
        .await
        .map_err(|(e, _)| anyhow!("IMAP login failed: {e}"))?;
    Ok(session)
}

#[cfg(feature = "channel-email")]
async fn fetch_inbox(
    config: &EmailAccountConfig,
    password: &str,
    folder: &str,
    include_read: bool,
    limit: usize,
    max_body_chars: usize,
    mark_as_seen: bool,
) -> Result<Vec<Value>> {
    let mut session = open_imap_session(config, password).await?;
    session
        .select(folder)
        .await
        .with_context(|| format!("Failed to select mailbox folder '{folder}'"))?;

    let search_query = if include_read { "ALL" } else { "UNSEEN" };
    let uids = session
        .uid_search(search_query)
        .await
        .with_context(|| format!("IMAP search failed with query '{search_query}'"))?;

    if uids.is_empty() {
        let _ = session.logout().await;
        return Ok(Vec::new());
    }

    let mut selected: Vec<u32> = uids.into_iter().collect();
    selected.sort_unstable_by(|a, b| b.cmp(a));
    selected.truncate(limit);
    let uid_set = selected
        .iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let fetch_stream = session
        .uid_fetch(&uid_set, "BODY.PEEK[]")
        .await
        .context("IMAP fetch failed")?;
    let messages: Vec<Fetch> = fetch_stream
        .try_collect()
        .await
        .context("Failed to collect fetched messages")?;

    let mut emails = Vec::new();
    for msg in messages {
        let uid = msg.uid.unwrap_or(0);
        let Some(body) = msg.body() else {
            continue;
        };
        let Some(parsed) = MessageParser::default().parse(body) else {
            continue;
        };

        let from = parsed
            .from()
            .and_then(|addr| addr.first())
            .and_then(|a| a.address())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let subject = parsed.subject().unwrap_or("(no subject)").to_string();
        let body_text = if let Some(text) = parsed.body_text(0) {
            text.to_string()
        } else if let Some(html) = parsed.body_html(0) {
            strip_html(html.as_ref())
        } else {
            String::new()
        };
        let body = truncate_chars(body_text.trim(), max_body_chars);

        let message_id = parsed
            .message_id()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("uid-{uid}"));

        emails.push(json!({
            "uid": uid,
            "from": from,
            "subject": subject,
            "message_id": message_id,
            "body": body
        }));
    }

    emails.sort_by(|a, b| {
        b.get("uid")
            .and_then(Value::as_u64)
            .cmp(&a.get("uid").and_then(Value::as_u64))
    });

    if mark_as_seen && !uid_set.is_empty() {
        let _ = session.uid_store(&uid_set, "+FLAGS (\\Seen)").await;
    }
    let _ = session.logout().await;

    Ok(emails)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_chars() {
        assert_eq!(truncate_chars("ciao", 10), "ciao");
        assert_eq!(truncate_chars("0123456789", 5), "01234...");
    }

    #[test]
    fn test_strip_html() {
        assert_eq!(strip_html("<p>Hello <b>World</b></p>"), "Hello World");
    }
}
