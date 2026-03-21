//! Custom tunnel provider — runs a user-defined command.
//!
//! The command receives the local port as the last argument.
//! The first line of stdout is parsed as the public URL.

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

/// Custom tunnel that runs an arbitrary command.
pub struct CustomTunnel {
    command: String,
    args: Vec<String>,
    child: Option<Child>,
}

impl CustomTunnel {
    pub fn new(command: String, args: Vec<String>) -> Self {
        Self {
            command,
            args,
            child: None,
        }
    }
}

#[async_trait::async_trait]
impl super::Tunnel for CustomTunnel {
    fn name(&self) -> &str {
        "custom"
    }

    async fn start(&mut self, local_port: u16) -> Result<String> {
        let mut cmd_args = self.args.clone();
        cmd_args.push(local_port.to_string());

        let mut child = Command::new(&self.command)
            .args(&cmd_args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("Failed to start custom tunnel: {}", self.command))?;

        let stdout = child
            .stdout
            .take()
            .context("Cannot read custom tunnel stdout")?;
        let mut reader = BufReader::new(stdout).lines();

        // First non-empty line containing "http" is the public URL
        let url = tokio::time::timeout(tokio::time::Duration::from_secs(30), async {
            while let Ok(Some(line)) = reader.next_line().await {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() && trimmed.contains("http") {
                    return Ok(trimmed);
                }
            }
            bail!(
                "Custom tunnel command '{}' exited without producing a URL on stdout",
                self.command
            )
        })
        .await
        .context("Timed out waiting for custom tunnel URL (30s)")??;

        self.child = Some(child);
        Ok(url)
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
            tracing::debug!(command = %self.command, "Custom tunnel stopped");
        }
        Ok(())
    }
}
