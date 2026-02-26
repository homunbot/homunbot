//! Service management for auto-start at boot.
//!
//! Supports:
//! - Linux: systemd user service
//! - macOS: launchd user agent

use anyhow::{Context, Result};

mod launchd;
mod systemd;

/// Install homun as a user service (auto-start at boot)
pub fn install() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        systemd::install()?;
    }
    #[cfg(target_os = "macos")]
    {
        launchd::install()?;
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        anyhow::bail!("Service installation not supported on this OS");
    }
    Ok(())
}

/// Uninstall the homun user service
pub fn uninstall() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        systemd::uninstall()?;
    }
    #[cfg(target_os = "macos")]
    {
        launchd::uninstall()?;
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        anyhow::bail!("Service uninstallation not supported on this OS");
    }
    Ok(())
}

/// Start the homun service
pub fn start() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        systemd::start()?;
    }
    #[cfg(target_os = "macos")]
    {
        launchd::start()?;
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        anyhow::bail!("Service start not supported on this OS");
    }
    Ok(())
}

/// Stop the homun service
pub fn stop() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        systemd::stop()?;
    }
    #[cfg(target_os = "macos")]
    {
        launchd::stop()?;
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        anyhow::bail!("Service stop not supported on this OS");
    }
    Ok(())
}

/// Check if the homun service is installed
pub fn is_installed() -> bool {
    #[cfg(target_os = "linux")]
    {
        systemd::is_installed()
    }
    #[cfg(target_os = "macos")]
    {
        launchd::is_installed()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        false
    }
}

/// Get the current status of the homun service
pub fn status() -> Result<ServiceStatus> {
    #[cfg(target_os = "linux")]
    {
        systemd::status()
    }
    #[cfg(target_os = "macos")]
    {
        launchd::status()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        anyhow::bail!("Service status not supported on this OS");
    }
}

/// Service status information
#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub installed: bool,
    pub running: bool,
    pub enabled: bool,
    pub service_file: Option<String>,
}

impl std::fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Service Status:")?;
        writeln!(
            f,
            "  Installed: {}",
            if self.installed { "yes" } else { "no" }
        )?;
        writeln!(f, "  Running: {}", if self.running { "yes" } else { "no" })?;
        writeln!(
            f,
            "  Enabled (auto-start): {}",
            if self.enabled { "yes" } else { "no" }
        )?;
        if let Some(ref path) = self.service_file {
            writeln!(f, "  Service file: {}", path)?;
        }
        Ok(())
    }
}

/// Get the path to the homun binary
fn get_binary_path() -> Result<std::path::PathBuf> {
    std::env::current_exe().context("Failed to get current executable path")
}

/// Get the user's home directory
fn get_home_dir() -> Result<std::path::PathBuf> {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .context("HOME environment variable not set")
}
