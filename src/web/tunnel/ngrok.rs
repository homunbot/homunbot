//! ngrok tunnel via `ngrok http` with JSON log parsing.
//!
//! Spawns `ngrok` as a child process with `--log stdout --log-format json`
//! and parses the `url` field from the `start_tunnel` log event.

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// ngrok tunnel provider.
pub struct NgrokTunnel {
    auth_token: String,
    child: Option<Child>,
}

impl NgrokTunnel {
    pub fn new(auth_token: String) -> Self {
        Self {
            auth_token,
            child: None,
        }
    }
}

#[async_trait::async_trait]
impl super::Tunnel for NgrokTunnel {
    fn name(&self) -> &str {
        "ngrok"
    }

    async fn start(&mut self, local_port: u16) -> Result<String> {
        let mut args = vec![
            "http".to_string(),
            local_port.to_string(),
            "--log".to_string(),
            "stdout".to_string(),
            "--log-format".to_string(),
            "json".to_string(),
        ];

        if !self.auth_token.is_empty() {
            args.push("--authtoken".to_string());
            args.push(self.auth_token.clone());
        }

        let mut child = Command::new("ngrok")
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context(
                "Failed to start ngrok. Install it: https://ngrok.com/download",
            )?;

        let stdout = child.stdout.take().context("Cannot read ngrok stdout")?;
        let mut reader = BufReader::new(stdout).lines();

        // ngrok JSON logs include a line with "msg":"started tunnel" and a "url" field
        let url = tokio::time::timeout(tokio::time::Duration::from_secs(15), async {
            while let Ok(Some(line)) = reader.next_line().await {
                // Parse JSON log line
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                    // Look for the tunnel URL in the "url" field
                    if let Some(url) = json.get("url").and_then(|v| v.as_str()) {
                        if url.starts_with("https://") {
                            return Ok(url.to_string());
                        }
                    }
                    // Also check "addr" field in start_tunnel messages
                    if json.get("msg").and_then(|v| v.as_str()) == Some("started tunnel") {
                        if let Some(url) = json.get("url").and_then(|v| v.as_str()) {
                            return Ok(url.to_string());
                        }
                    }
                }
            }
            bail!("ngrok exited without producing a tunnel URL")
        })
        .await
        .context("Timed out waiting for ngrok URL (15s)")??;

        self.child = Some(child);
        Ok(url)
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
            tracing::debug!("ngrok tunnel stopped");
        }
        Ok(())
    }
}
