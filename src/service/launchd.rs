//! Launchd user agent support for macOS

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::{get_binary_path, get_home_dir, ServiceStatus};

const SERVICE_NAME: &str = "ai.homun.daemon";
const PLIST_NAME: &str = "ai.homun.daemon.plist";

/// Get the current user ID for launchd
fn get_uid() -> Result<u32> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("Failed to get user ID")?;
    let uid_str = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .context("Failed to parse user ID")?;
    Ok(uid_str)
}

/// Get the launchd user agents directory
fn get_agents_dir() -> Result<PathBuf> {
    let home = get_home_dir()?;
    Ok(home.join("Library").join("LaunchAgents"))
}

/// Get the plist file path
fn get_plist_file() -> Result<PathBuf> {
    Ok(get_agents_dir()?.join(PLIST_NAME))
}

/// Generate the launchd plist content
fn generate_plist_content(binary_path: &std::path::Path, home_dir: &std::path::Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>gateway</string>
    </array>
    
    <key>RunAtLoad</key>
    <true/>
    
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
        <key>Crashed</key>
        <true/>
    </dict>
    
    <key>ThrottleInterval</key>
    <integer>10</integer>
    
    <key>EnvironmentVariables</key>
    <dict>
        <key>HOME</key>
        <string>{}</string>
        <key>RUST_LOG</key>
        <string>info</string>
    </dict>
    
    <key>StandardOutPath</key>
    <string>{}/.homun/logs/daemon.log</string>
    
    <key>StandardErrorPath</key>
    <string>{}/.homun/logs/daemon.log</string>
    
    <key>WorkingDirectory</key>
    <string>{}</string>
    
    <key>ProcessType</key>
    <string>Background</string>
</dict>
</plist>
"#,
        SERVICE_NAME,
        binary_path.display(),
        home_dir.display(),
        home_dir.display(),
        home_dir.display(),
        home_dir.display(),
    )
}

/// Install the launchd user agent
pub fn install() -> Result<()> {
    let agents_dir = get_agents_dir()?;
    let plist_file = get_plist_file()?;
    let binary_path = get_binary_path()?;
    let home_dir = get_home_dir()?;

    // Create agents directory if it doesn't exist
    fs::create_dir_all(&agents_dir)
        .with_context(|| format!("Failed to create directory: {}", agents_dir.display()))?;

    // Create logs directory
    let logs_dir = home_dir.join(".homun").join("logs");
    fs::create_dir_all(&logs_dir)
        .with_context(|| format!("Failed to create logs directory: {}", logs_dir.display()))?;

    // Write plist file
    let content = generate_plist_content(&binary_path, &home_dir);
    fs::write(&plist_file, &content)
        .with_context(|| format!("Failed to write plist file: {}", plist_file.display()))?;

    tracing::info!("Homun service installed successfully");
    println!("Service installed to: {}", plist_file.display());
    println!("Run 'homun service start' to start the service");
    println!("The service will auto-start on next login");

    Ok(())
}

/// Uninstall the launchd user agent
pub fn uninstall() -> Result<()> {
    let plist_file = get_plist_file()?;

    // Stop and unload the service first
    if let Ok(uid) = get_uid() {
        let _ = Command::new("launchctl")
            .args(["bootout", &format!("gui/{}", uid), SERVICE_NAME])
            .output();
    }

    // Remove plist file
    if plist_file.exists() {
        fs::remove_file(&plist_file)
            .with_context(|| format!("Failed to remove plist file: {}", plist_file.display()))?;
    }

    tracing::info!("Homun service uninstalled");
    println!("Service uninstalled successfully");

    Ok(())
}

/// Start the launchd user agent
pub fn start() -> Result<()> {
    if !is_installed() {
        anyhow::bail!("Service not installed. Run 'homun service install' first.");
    }

    let plist_file = get_plist_file()?;

    // Load the plist
    let output = Command::new("launchctl")
        .args(["load", "-w"])
        .arg(&plist_file)
        .output()
        .context("Failed to start service")?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to start service: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    println!("Service started");
    Ok(())
}

/// Stop the launchd user agent
pub fn stop() -> Result<()> {
    let uid = get_uid()?;
    let output = Command::new("launchctl")
        .args(["bootout", &format!("gui/{}", uid), SERVICE_NAME])
        .output()
        .context("Failed to stop service")?;

    // bootout returns error if service is not running, which is fine
    if output.status.success() {
        println!("Service stopped");
    } else {
        println!("Service was not running");
    }

    Ok(())
}

/// Check if the service is installed
pub fn is_installed() -> bool {
    get_plist_file().map(|f| f.exists()).unwrap_or(false)
}

/// Get the service status
pub fn status() -> Result<ServiceStatus> {
    let plist_file = get_plist_file()?;
    let installed = plist_file.exists();

    // Check if running using launchctl list
    let output = Command::new("launchctl")
        .args(["list", SERVICE_NAME])
        .output()
        .ok();

    let running = output.as_ref().map(|o| o.status.success()).unwrap_or(false);

    // On macOS, RunAtLoad=true means it's enabled
    let enabled = installed;

    Ok(ServiceStatus {
        installed,
        running,
        enabled,
        service_file: if installed {
            Some(plist_file.display().to_string())
        } else {
            None
        },
    })
}
