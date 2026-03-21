//! Cloudflare Quick Tunnel via `cloudflared tunnel --url`.
//!
//! Spawns `cloudflared` as a child process and parses the generated
//! `*.trycloudflare.com` URL from its stderr output.

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// Cloudflare Tunnel provider using `cloudflared` CLI.
pub struct CloudflareTunnel {
    child: Option<Child>,
}

impl CloudflareTunnel {
    pub fn new() -> Self {
        Self { child: None }
    }
}

#[async_trait::async_trait]
impl super::Tunnel for CloudflareTunnel {
    fn name(&self) -> &str {
        "cloudflare"
    }

    async fn start(&mut self, local_port: u16) -> Result<String> {
        let target = format!("http://localhost:{local_port}");

        let mut child = Command::new("cloudflared")
            .args(["tunnel", "--url", &target])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context(
                "Failed to start cloudflared. Install it: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/",
            )?;

        // cloudflared prints the URL to stderr in a line like:
        // "... | https://random-name.trycloudflare.com"
        // or INF +---...---+ then INF |  https://...  |
        let stderr = child
            .stderr
            .take()
            .context("Cannot read cloudflared stderr")?;
        let mut reader = BufReader::new(stderr).lines();

        let url = tokio::time::timeout(tokio::time::Duration::from_secs(30), async {
            while let Ok(Some(line)) = reader.next_line().await {
                // Look for trycloudflare.com URL in the line
                if let Some(start) = line.find("https://") {
                    let url_part = &line[start..];
                    if url_part.contains("trycloudflare.com") {
                        // Trim any trailing whitespace or pipe chars
                        let url = url_part
                            .trim_end_matches(|c: char| c.is_whitespace() || c == '|')
                            .to_string();
                        return Ok(url);
                    }
                }
            }
            bail!("cloudflared exited without producing a tunnel URL")
        })
        .await
        .context("Timed out waiting for cloudflared URL (30s)")??;

        self.child = Some(child);
        Ok(url)
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
            tracing::debug!("cloudflared tunnel stopped");
        }
        Ok(())
    }
}
