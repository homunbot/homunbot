//! Tunnel providers for exposing the local web server to the internet.
//!
//! Inspired by ZeroClaw's pluggable Tunnel trait. Spawns an external
//! process (cloudflared, ngrok, or custom) and parses the public URL
//! from its stdout.

mod cloudflare;
mod custom;
mod ngrok;

use anyhow::{bail, Result};
use async_trait::async_trait;

use crate::config::TunnelConfig;

/// Tunnel provider — exposes a local port to the internet.
#[async_trait]
pub trait Tunnel: Send + Sync {
    /// Provider name (for logging).
    fn name(&self) -> &str;

    /// Start the tunnel targeting a local port. Returns the public URL.
    ///
    /// Spawns an external process and parses its output to extract the URL.
    /// The process is kept running until `stop()` is called.
    async fn start(&mut self, local_port: u16) -> Result<String>;

    /// Gracefully stop the tunnel process.
    async fn stop(&mut self) -> Result<()>;
}

/// Create a tunnel from config.
///
/// Returns `Err` if the provider is unknown or the required binary is missing.
pub fn create_tunnel(config: &TunnelConfig) -> Result<Box<dyn Tunnel>> {
    match config.provider.as_str() {
        "cloudflare" => Ok(Box::new(cloudflare::CloudflareTunnel::new())),
        "ngrok" => Ok(Box::new(ngrok::NgrokTunnel::new(config.auth_token.clone()))),
        "custom" => {
            if config.custom_command.is_empty() {
                bail!("Tunnel provider 'custom' requires a custom_command");
            }
            Ok(Box::new(custom::CustomTunnel::new(
                config.custom_command.clone(),
                config.custom_args.clone(),
            )))
        }
        other => bail!("Unknown tunnel provider: '{other}'. Supported: cloudflare, ngrok, custom"),
    }
}
