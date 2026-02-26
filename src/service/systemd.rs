//! Systemd user service support for Linux

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::{ServiceStatus, get_binary_path, get_home_dir};

const SERVICE_NAME: &str = "homun";

/// Get the systemd user service directory
fn get_service_dir() -> Result<PathBuf> {
    let home = get_home_dir()?;
    Ok(home.join(".config").join("systemd").join("user"))
}

/// Get the service file path
fn get_service_file() -> Result<PathBuf> {
    Ok(get_service_dir()?.join(format!("{}.service", SERVICE_NAME)))
}

/// Generate the systemd service unit content
fn generate_service_content(binary_path: &std::path::Path) -> String {
    format!(
        r#"[Unit]
Description=Homun - Personal AI Assistant
Documentation=https://github.com/fabic/homun
After=network.target

[Service]
Type=simple
ExecStart={} gateway
Restart=on-failure
RestartSec=10

# Environment
Environment=RUST_LOG=info
Environment=HOME={}

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths={}/.homun

[Install]
WantedBy=default.target
"#,
        binary_path.display(),
        get_home_dir().unwrap_or_default().display(),
        get_home_dir().unwrap_or_default().display(),
    )
}

/// Install the systemd user service
pub fn install() -> Result<()> {
    let service_dir = get_service_dir()?;
    let service_file = get_service_file()?;
    let binary_path = get_binary_path()?;

    // Create service directory if it doesn't exist
    fs::create_dir_all(&service_dir)
        .with_context(|| format!("Failed to create directory: {}", service_dir.display()))?;

    // Write service file
    let content = generate_service_content(&binary_path);
    fs::write(&service_file, &content)
        .with_context(|| format!("Failed to write service file: {}", service_file.display()))?;

    // Reload systemd daemon
    let output = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output()
        .context("Failed to run systemctl --user daemon-reload")?;
    
    if !output.status.success() {
        anyhow::bail!("systemctl daemon-reload failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    // Enable the service
    let output = Command::new("systemctl")
        .args(["--user", "enable", SERVICE_NAME])
        .output()
        .context("Failed to enable service")?;
    
    if !output.status.success() {
        anyhow::bail!("Failed to enable service: {}", String::from_utf8_lossy(&output.stderr));
    }

    tracing::info!("Homun service installed successfully");
    println!("Service installed to: {}", service_file.display());
    println!("Run 'homun service start' to start the service");
    println!("The service will auto-start on next login");

    Ok(())
}

/// Uninstall the systemd user service
pub fn uninstall() -> Result<()> {
    let service_file = get_service_file()?;

    // Stop the service first
    let _ = Command::new("systemctl")
        .args(["--user", "stop", SERVICE_NAME])
        .output();

    // Disable the service
    let _ = Command::new("systemctl")
        .args(["--user", "disable", SERVICE_NAME])
        .output();

    // Remove service file
    if service_file.exists() {
        fs::remove_file(&service_file)
            .with_context(|| format!("Failed to remove service file: {}", service_file.display()))?;
    }

    // Reload systemd daemon
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output();

    tracing::info!("Homun service uninstalled");
    println!("Service uninstalled successfully");

    Ok(())
}

/// Start the systemd user service
pub fn start() -> Result<()> {
    if !is_installed() {
        anyhow::bail!("Service not installed. Run 'homun service install' first.");
    }

    let output = Command::new("systemctl")
        .args(["--user", "start", SERVICE_NAME])
        .output()
        .context("Failed to start service")?;
    
    if !output.status.success() {
        anyhow::bail!("Failed to start service: {}", String::from_utf8_lossy(&output.stderr));
    }

    println!("Service started");
    Ok(())
}

/// Stop the systemd user service
pub fn stop() -> Result<()> {
    let output = Command::new("systemctl")
        .args(["--user", "stop", SERVICE_NAME])
        .output()
        .context("Failed to stop service")?;
    
    if !output.status.success() {
        anyhow::bail!("Failed to stop service: {}", String::from_utf8_lossy(&output.stderr));
    }

    println!("Service stopped");
    Ok(())
}

/// Check if the service is installed
pub fn is_installed() -> bool {
    get_service_file().map(|f| f.exists()).unwrap_or(false)
}

/// Get the service status
pub fn status() -> Result<ServiceStatus> {
    let service_file = get_service_file()?;
    let installed = service_file.exists();

    // Check if running
    let running = Command::new("systemctl")
        .args(["--user", "is-active", SERVICE_NAME])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    // Check if enabled
    let enabled = Command::new("systemctl")
        .args(["--user", "is-enabled", SERVICE_NAME])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    Ok(ServiceStatus {
        installed,
        running,
        enabled,
        service_file: if installed { Some(service_file.display().to_string()) } else { None },
    })
}
