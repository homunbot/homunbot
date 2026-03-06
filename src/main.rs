// Allow dead code: this binary exposes a public API design for future lib.rs extraction.
// Many types/functions are pub but only used in specific subcommands.
#![allow(dead_code, unused_imports)]

use std::sync::Arc;

use anyhow::{Context, Result};
#[cfg(feature = "cli")]
use clap::{Parser, Subcommand};
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

mod agent;
#[cfg(feature = "browser")]
mod browser;
mod bus;
mod channels;
mod config;
mod logs;
mod mcp_setup;
mod provider;
mod queue;
mod scheduler;
mod security;
mod service;
mod session;
mod skills;
mod storage;
mod tools;
#[cfg(feature = "cli")]
mod tui;
mod user;
mod utils;
#[cfg(feature = "web-ui")]
mod web;

#[cfg(feature = "cli")]
use crate::channels::CliChannel;
use crate::config::Config;

use crate::session::SessionManager;
use crate::storage::Database;
#[cfg(feature = "channel-email")]
use crate::tools::ReadEmailInboxTool;
use crate::tools::{
    CreateAutomationTool, CronTool, EditFileTool, ListDirTool, MessageTool, ReadFileTool,
    ShellTool, SpawnTool, ToolRegistry, VaultTool, WebFetchTool, WebSearchTool, WriteFileTool,
};

#[cfg(feature = "mcp")]
use crate::tools::McpManager;

#[cfg(feature = "local-embeddings")]
use crate::tools::RememberTool;

#[cfg(feature = "browser")]
use crate::browser::BrowserTool;

#[cfg(feature = "cli")]
#[derive(Parser)]
#[command(
    name = "homun",
    version,
    about = "🧪 The digital homunculus that lives in your computer"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[cfg(feature = "cli")]
#[derive(Subcommand)]
enum Commands {
    /// Interactive chat or one-shot message
    Chat {
        /// Send a single message and exit
        #[arg(short, long)]
        message: Option<String>,
    },
    /// Start the gateway (all channels + cron + heartbeat)
    Gateway,
    /// Manage configuration (TUI dashboard if no subcommand)
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommands>,
    },
    /// Manage LLM providers
    Provider {
        #[command(subcommand)]
        command: ProviderCommands,
    },
    /// Show status
    Status,
    /// Manage skills
    Skills {
        #[command(subcommand)]
        command: SkillsCommands,
    },
    /// Manage cron jobs
    Cron {
        #[command(subcommand)]
        command: CronCommands,
    },
    /// Manage automations
    Automations {
        #[command(subcommand)]
        command: AutomationCommands,
    },
    /// Manage MCP servers
    Mcp {
        #[command(subcommand)]
        command: McpCommands,
    },
    /// Manage memory (reset, status)
    Memory {
        #[command(subcommand)]
        command: MemoryCommands,
    },
    /// Manage users and permissions
    Users {
        #[command(subcommand)]
        command: UserCommands,
    },
    /// Manage system service (auto-start at boot)
    Service {
        #[command(subcommand)]
        command: ServiceCommands,
    },
    /// Stop the running gateway
    Stop,
    /// Restart the gateway (stop + start)
    Restart,
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Get a config value by dot-path (e.g., agent.model)
    Get { key: String },
    /// Set a config value by dot-path
    Set { key: String, value: String },
    /// Initialize default configuration
    Init,
    /// Show config file path
    Path,
}

#[derive(Subcommand)]
enum ProviderCommands {
    /// List all providers and their status
    List,
    /// Configure a provider
    Add {
        /// Provider name (anthropic, openai, openrouter, ollama, etc.)
        name: String,
        /// API key
        #[arg(long)]
        api_key: Option<String>,
        /// Custom base URL
        #[arg(long)]
        api_base: Option<String>,
    },
    /// Remove a provider's configuration
    Remove { name: String },
}

#[derive(Subcommand)]
enum SkillsCommands {
    /// List installed skills
    List,
    /// Search for skills on GitHub
    Search {
        /// Search query
        query: String,
        /// Maximum results to show
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// Search skills on ClawHub marketplace (3000+ skills)
    Hub {
        /// Search query (matches skill names)
        query: String,
        /// Maximum results to show
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Show details of an installed skill
    Info { name: String },
    /// Install a skill (GitHub: owner/repo, ClawHub: clawhub:owner/skill)
    Add {
        /// Skill source: owner/repo (GitHub) or clawhub:owner/skill (ClawHub)
        repo: String,
        /// Install even if the post-download security scan wants to block it
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Remove an installed skill
    Remove { name: String },
}

#[derive(Subcommand)]
enum McpCommands {
    /// List configured MCP servers
    List,
    /// List curated MCP setup presets
    Catalog,
    /// Add an MCP server
    Add {
        /// Server name (unique identifier)
        name: String,
        /// Transport type
        #[arg(long, default_value = "stdio")]
        transport: String,
        /// Command to run (for stdio transport)
        #[arg(long)]
        command: Option<String>,
        /// Arguments for the command
        #[arg(long, num_args = 0..)]
        args: Vec<String>,
        /// Server URL (for http transport)
        #[arg(long)]
        url: Option<String>,
    },
    /// Guided setup for a known MCP service
    Setup {
        /// Preset/service id (e.g. github, gmail, notion)
        service: String,
        /// Optional override for configured server name
        #[arg(long)]
        name: Option<String>,
        /// Environment variables in KEY=VALUE format (repeatable)
        #[arg(long, num_args = 0.., value_name = "KEY=VALUE")]
        env: Vec<String>,
        /// Overwrite an existing server config with the same name
        #[arg(long, default_value_t = false)]
        overwrite: bool,
        /// Skip post-setup connection test
        #[arg(long, default_value_t = false)]
        skip_test: bool,
    },
    /// Remove an MCP server
    Remove { name: String },
    /// Enable/disable an MCP server
    Toggle { name: String },
}

#[derive(Subcommand)]
enum CronCommands {
    /// List scheduled jobs
    List,
    /// Add a new cron job
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        message: String,
        #[arg(long)]
        cron: Option<String>,
        #[arg(long)]
        every: Option<u64>,
    },
    /// Remove a cron job
    Remove { id: String },
}

#[derive(Subcommand)]
enum AutomationCommands {
    /// List automations
    List,
    /// Add a new automation
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        prompt: String,
        #[arg(long)]
        cron: Option<String>,
        #[arg(long)]
        every: Option<u64>,
        /// Delivery target in format channel:chat_id (default: cli:default)
        #[arg(long)]
        deliver_to: Option<String>,
        /// Trigger condition: always | on_change | contains
        #[arg(long)]
        trigger: Option<String>,
        /// Optional value for trigger=contains
        #[arg(long)]
        trigger_value: Option<String>,
        /// Create automation as disabled
        #[arg(long, default_value_t = false)]
        disabled: bool,
    },
    /// Toggle automation on/off
    Toggle { id: String },
    /// Run an automation immediately
    Run { id: String },
    /// Show execution history
    History {
        id: String,
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Remove an automation
    Remove { id: String },
}

#[derive(Subcommand)]
enum MemoryCommands {
    /// Show memory statistics
    Status,
    /// Reset all memory (conversations, facts, brain files)
    Reset {
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum UserCommands {
    /// List all users
    List,
    /// Create a new user
    Add {
        /// Username
        name: String,
        /// Make user an admin
        #[arg(long)]
        admin: bool,
    },
    /// Show user details
    Info {
        /// Username or ID
        user: String,
    },
    /// Link a channel identity to a user
    Link {
        /// Username or ID
        #[arg(long)]
        user: String,
        /// Channel type (telegram, discord, whatsapp, webhook)
        #[arg(long)]
        channel: String,
        /// Platform-specific ID (e.g., Telegram user ID)
        #[arg(long)]
        id: String,
        /// Display name for the identity
        #[arg(long)]
        display_name: Option<String>,
    },
    /// Unlink a channel identity from a user
    Unlink {
        /// Username or ID
        #[arg(long)]
        user: String,
        /// Channel type
        #[arg(long)]
        channel: String,
        /// Platform-specific ID
        #[arg(long)]
        id: String,
    },
    /// Create a webhook token for a user
    Token {
        /// Username or ID
        #[arg(long)]
        user: String,
        /// Token name/description
        #[arg(long)]
        name: String,
    },
    /// Delete a user
    Remove {
        /// Username or ID
        user: String,
    },
}

#[derive(Subcommand)]
enum ServiceCommands {
    /// Install homun as a user service (auto-start at boot)
    Install,
    /// Uninstall the homun service
    Uninstall,
    /// Start the homun service
    Start,
    /// Stop the homun service
    Stop,
    /// Show service status
    Status,
}

// Provider factory functions are in provider::factory (re-exported as
// provider::create_provider / provider::create_single_provider).

/// Create and register all tools from config
fn create_tool_registry(
    config: &Config,
    db: Database,
    shared_config: Option<Arc<tokio::sync::RwLock<Config>>>,
) -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    // Workspace directory restriction
    let allowed_dir = if config.tools.exec.restrict_to_workspace {
        Some(Config::workspace_dir())
    } else {
        None
    };

    // Prepare permissions config for tools
    let permissions = std::sync::Arc::new(config.permissions.clone());
    let shell_permissions = std::sync::Arc::new(config.permissions.shell.clone());

    // Initialize approval manager for command approval workflow
    tools::init_approval_manager(&config.permissions.approval);

    // Shell tool with OS-specific permissions
    registry.register(Box::new(ShellTool::with_permissions_sandbox_and_config(
        config.tools.exec.timeout,
        config.tools.exec.restrict_to_workspace,
        Some(shell_permissions),
        Some(config.security.execution_sandbox.clone()),
        shared_config,
    )));

    // File tools with ACL-based permissions
    registry.register(Box::new(ReadFileTool::with_permissions(
        allowed_dir.clone(),
        permissions.clone(),
    )));
    registry.register(Box::new(WriteFileTool::with_permissions(
        allowed_dir.clone(),
        permissions.clone(),
    )));
    registry.register(Box::new(EditFileTool::with_permissions(
        allowed_dir.clone(),
        permissions.clone(),
    )));
    registry.register(Box::new(ListDirTool::with_permissions(
        allowed_dir,
        permissions,
    )));

    // Web search tool (Brave API) — only register if API key is configured
    if !config.tools.web_search.api_key.is_empty() {
        registry.register(Box::new(WebSearchTool::new(
            &config.tools.web_search.api_key,
            config.tools.web_search.max_results,
        )));
    } else {
        tracing::debug!("web_search tool not registered: no API key configured");
    }

    // Web fetch tool
    registry.register(Box::new(WebFetchTool::new()));

    // Vault tool (encrypted secrets storage)
    registry.register(Box::new(VaultTool::new()));

    // Automation creation tool (shared storage with scheduler + web API)
    registry.register(Box::new(CreateAutomationTool::new(db)));

    // Skill creation tool (generates and installs starter skills in ~/.homun/skills)
    registry.register(Box::new(tools::CreateSkillTool::new()));

    // Email inbox reading tool (IMAP) for proactive automations and chat tasks.
    #[cfg(feature = "channel-email")]
    registry.register(Box::new(ReadEmailInboxTool::new()));

    // Remember tool (save personal information - requires local-embeddings feature)
    #[cfg(feature = "local-embeddings")]
    registry.register(Box::new(tools::RememberTool::new()));

    // Browser tool — register if Chrome/Chromium is found on the system
    #[cfg(feature = "browser")]
    {
        if let Some(executable) = config.browser.resolved_executable() {
            registry.register(Box::new(tools::BrowserTool::new()));
            tracing::info!(
                executable = %executable.display(),
                headless = config.browser.headless,
                "Browser tool registered (Chrome found)"
            );
        } else {
            tracing::debug!("Browser tool not registered: no Chrome/Chromium executable found");
        }
    }

    tracing::info!(tools = registry.len(), "Tool registry initialized");

    registry
}

/// Try to create a MemorySearcher (embedding engine + vector index).
///
/// Returns `None` if the embedding engine fails to initialize (e.g. ONNX model
/// download fails). This keeps the agent functional without vector search.
///
/// Only available when `local-embeddings` feature is enabled.
#[cfg(feature = "local-embeddings")]
fn try_create_memory_searcher(db: Database, config: &Config) -> Option<agent::MemorySearcher> {
    match agent::EmbeddingEngine::new(config) {
        Ok(engine) => {
            let searcher = agent::MemorySearcher::new(db, engine);
            tracing::info!("Memory searcher initialized (vector + FTS5 hybrid search)");
            Some(searcher)
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to initialize embedding engine, vector search disabled");
            None
        }
    }
}

/// Check if a process is alive by PID string.
#[cfg(unix)]
fn is_process_alive(pid_str: &str) -> bool {
    pid_str
        .parse::<u32>()
        .ok()
        .map(|pid| {
            std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_process_alive(_pid_str: &str) -> bool {
    // On Windows, assume alive if PID file exists (conservative)
    true
}

/// Stop the running gateway via PID file. Returns true if a running process was stopped.
fn stop_gateway() -> Result<bool> {
    let pid_file = Config::data_dir().join("homun.pid");

    let pid_str = match std::fs::read_to_string(&pid_file) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("No gateway running (PID file not found)");
            return Ok(false);
        }
    };

    let pid = pid_str.trim();

    if !is_process_alive(pid) {
        eprintln!("Process {pid} not found (stale PID file). Cleaning up.");
        let _ = std::fs::remove_file(&pid_file);
        return Ok(false);
    }

    #[cfg(unix)]
    {
        let status = std::process::Command::new("kill")
            .args(["-TERM", pid])
            .status();
        match status {
            Ok(s) if s.success() => {
                println!("Sent stop signal to gateway (PID {pid})");
                // Wait for process to exit (poll for PID file removal, max 5s)
                for _ in 0..50 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    if !pid_file.exists() {
                        println!("Gateway stopped.");
                        return Ok(true);
                    }
                }
                println!("Gateway may still be stopping (PID file not yet removed).");
                Ok(true)
            }
            _ => {
                eprintln!("Failed to stop process {pid}. Cleaning up stale PID file.");
                let _ = std::fs::remove_file(&pid_file);
                Ok(false)
            }
        }
    }

    #[cfg(not(unix))]
    {
        let status = std::process::Command::new("taskkill")
            .args(["/PID", pid, "/F"])
            .status();
        match status {
            Ok(s) if s.success() => {
                println!("Gateway stopped (PID {pid}).");
                let _ = std::fs::remove_file(&pid_file);
                Ok(true)
            }
            _ => {
                eprintln!("Failed to stop process {pid}. Cleaning up stale PID file.");
                let _ = std::fs::remove_file(&pid_file);
                Ok(false)
            }
        }
    }
}

fn print_install_security_summary(report: Option<&crate::skills::SecurityReport>, forced: bool) {
    let Some(report) = report else {
        return;
    };

    if report.warnings.is_empty() {
        println!(
            "  Security: clean (risk {}/100, {} file(s) scanned)",
            report.risk_score, report.scanned_files
        );
        return;
    }

    let status = if report.is_blocked() {
        if forced {
            "forced override"
        } else {
            "blocked"
        }
    } else {
        "review suggested"
    };

    println!(
        "  Security: risk {}/100, {} finding(s), {}",
        report.risk_score,
        report.warnings.len(),
        status
    );
    for warning in report.warnings.iter().take(3) {
        let location = match (&warning.file, warning.line) {
            (Some(file), Some(line)) => format!(" ({file}:{line})"),
            (Some(file), None) => format!(" ({file})"),
            _ => String::new(),
        };
        println!("    - {}{}", warning.description, location);
    }
    if report.warnings.len() > 3 {
        println!("    - ...and {} more finding(s)", report.warnings.len() - 3);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install rustls CryptoProvider before any TLS usage.
    // Multiple transitive deps enable both `ring` and `aws-lc-rs` features on rustls,
    // so we must pick one explicitly to avoid the auto-detection panic.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cli = Cli::parse();

    // Default (no subcommand) = interactive chat
    let command = cli.command.unwrap_or(Commands::Chat { message: None });

    // TUI commands use alternate screen — logs on stderr would corrupt the display.
    // Write logs to a file instead, or suppress them entirely.
    let is_tui_command = matches!(&command, Commands::Config { command: None });
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,homun=debug"));

    if is_tui_command {
        // During TUI: log to ~/.homun/tui.log so stderr stays clean
        let log_dir = Config::data_dir();
        std::fs::create_dir_all(&log_dir).ok();
        let log_file = std::fs::File::create(log_dir.join("tui.log")).ok();
        if let Some(file) = log_file {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(file)
                        .with_ansi(false),
                )
                .with(crate::logs::SseLogLayer)
                .init();
        }
    } else {
        // Normal mode: log to stderr
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .with(crate::logs::SseLogLayer)
            .init();
    }

    match command {
        Commands::Chat { message } => {
            let config = Config::load()?;
            let db = Database::open(&config.storage.resolved_path()).await?;
            let provider = provider::create_provider(&config)?;
            let mut tool_registry = create_tool_registry(&config, db.clone(), None);

            // Connect to MCP servers and register their tools
            #[cfg(feature = "mcp")]
            let (mcp_manager, mcp_tools) = McpManager::start_with_sandbox(
                &config.mcp.servers,
                Some(config.security.execution_sandbox.clone()),
                None,
            )
            .await;
            #[cfg(feature = "mcp")]
            for tool in mcp_tools {
                tool_registry.register(tool);
            }

            let session_manager = SessionManager::new(db.clone());
            #[cfg(feature = "local-embeddings")]
            let db_for_searcher = db.clone();
            #[cfg(not(feature = "local-embeddings"))]
            let _db_for_searcher = db.clone();
            // Capture tool names before moving registry (for system prompt routing rules)
            let tool_names: Vec<String> = tool_registry
                .names()
                .iter()
                .map(|s| s.to_string())
                .collect();
            // Wrap config in Arc for AgentLoop (no web server sharing in CLI mode,
            // but AgentLoop requires Arc<RwLock<Config>> for API uniformity)
            let shared_config = Arc::new(tokio::sync::RwLock::new(config));
            let mut agent = agent::AgentLoop::new(
                provider,
                shared_config.clone(),
                session_manager.clone(),
                tool_registry,
                db,
            )
            .await;
            agent.set_registered_tool_names(tool_names);

            // Initialize memory searcher (vector + FTS5 hybrid search)
            #[cfg(feature = "local-embeddings")]
            {
                let cfg = shared_config.read().await;
                if let Some(searcher) = try_create_memory_searcher(db_for_searcher, &cfg) {
                    agent.set_memory_searcher(searcher);
                }
            }

            // Load installed skills and inject into the agent's system prompt
            let mut skill_registry = skills::SkillRegistry::new();
            if let Err(e) = skill_registry.scan_and_load().await {
                tracing::warn!(error = %e, "Failed to load skills");
            }
            if !skill_registry.is_empty() {
                agent
                    .set_skills_summary(skill_registry.build_prompt_summary())
                    .await;
                tracing::info!(
                    skills = skill_registry.len(),
                    "Skills loaded into agent context"
                );
            }
            // Share the skill registry so skills can be activated on-demand
            let skill_registry = Arc::new(tokio::sync::RwLock::new(skill_registry));
            agent.set_skill_registry(skill_registry);

            let cli_channel = CliChannel::new(agent, session_manager);

            if let Some(msg) = message {
                // One-shot mode
                let response = cli_channel.one_shot(&msg).await?;
                println!("{}", response);
            } else {
                // Interactive mode
                cli_channel.interactive().await?;
            }

            // Gracefully shutdown MCP connections
            #[cfg(feature = "mcp")]
            mcp_manager.shutdown().await;
        }
        Commands::Gateway => {
            use crate::scheduler::CronScheduler;
            use std::sync::Arc;

            // PID file management: kill existing instance if running
            let pid_file = Config::data_dir().join("homun.pid");

            // Check if PID file exists and try to kill existing process
            if pid_file.exists() {
                if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
                    if let Ok(old_pid) = pid_str.trim().parse::<u32>() {
                        // Check if process is still running
                        #[cfg(unix)]
                        {
                            use std::process::Command;
                            // Send SIGTERM to the old process
                            let _ = Command::new("kill")
                                .arg("-TERM")
                                .arg(old_pid.to_string())
                                .output();

                            tracing::info!(
                                "Sent TERM signal to existing instance (PID {})",
                                old_pid
                            );

                            // Wait for process to die (up to 5 seconds)
                            for i in 1..=10 {
                                std::thread::sleep(std::time::Duration::from_millis(500));
                                // Check if process still exists
                                let check = Command::new("kill")
                                    .arg("-0")
                                    .arg(old_pid.to_string())
                                    .output();
                                if check.is_err()
                                    || check.map(|o| !o.status.success()).unwrap_or(true)
                                {
                                    tracing::info!(
                                        "Previous instance terminated after {}ms",
                                        i * 500
                                    );
                                    break;
                                }
                            }
                        }
                        #[cfg(windows)]
                        {
                            use std::process::Command;
                            let _ = Command::new("taskkill")
                                .args(["/PID", &old_pid.to_string(), "/F"])
                                .output();
                            tracing::info!("Killed existing instance (PID {})", old_pid);
                            std::thread::sleep(std::time::Duration::from_secs(1));
                        }
                    }
                }
            }

            // Write new PID file
            std::fs::write(&pid_file, std::process::id().to_string())?;

            let config = Config::load()?;
            // Shared config: web UI writes → agent reads on next request (hot-reload)
            let shared_config = Arc::new(tokio::sync::RwLock::new(config));
            // Snapshot for one-time startup operations (provider, tools, channels, etc.)
            let config = shared_config.read().await.clone();

            let db = Database::open(&config.storage.resolved_path()).await?;

            // Try to create provider, but allow gateway to start without one
            // This enables configuration via Web UI
            let provider = match provider::create_provider(&config) {
                Ok(p) => Some(p),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "No provider configured. Gateway starting in setup mode. \
                        Configure a provider at http://localhost:{}/setup",
                        config.channels.web.port
                    );
                    None
                }
            };

            let session_manager = SessionManager::new(db.clone());

            // Create CronScheduler before the tool registry so CronTool can use it
            let (cron_event_tx, cron_event_rx) = tokio::sync::mpsc::channel(50);
            let cron_scheduler = Arc::new(CronScheduler::new(db.clone(), cron_event_tx));

            // Build tool registry with CronTool + MessageTool + SpawnTool + MCP tools
            let mut tool_registry =
                create_tool_registry(&config, db.clone(), Some(shared_config.clone()));
            tool_registry.register(Box::new(CronTool::new(cron_scheduler.clone())));
            tool_registry.register(Box::new(MessageTool::new()));

            // SpawnTool uses a late-bound OnceCell because SubagentManager needs Arc<AgentLoop>
            let spawn_manager_cell = Arc::new(tokio::sync::OnceCell::new());
            tool_registry.register(Box::new(tools::SpawnTool::new(spawn_manager_cell.clone())));

            // Connect to MCP servers and register their tools
            #[cfg(feature = "mcp")]
            let (_mcp_manager, mcp_tools) = McpManager::start_with_sandbox(
                &config.mcp.servers,
                Some(config.security.execution_sandbox.clone()),
                Some(shared_config.clone()),
            )
            .await;
            #[cfg(feature = "mcp")]
            for tool in mcp_tools {
                tool_registry.register(tool);
            }

            // Create tool message channel for proactive messaging (MessageTool → Gateway → Channel)
            let (tool_msg_tx, tool_msg_rx) = tokio::sync::mpsc::channel(100);

            // Create agent only if provider is available
            #[cfg(feature = "local-embeddings")]
            let db_for_searcher = db.clone();
            #[cfg(not(feature = "local-embeddings"))]
            let _db_for_searcher = db.clone();
            let db_for_web = db.clone();
            // Capture tool names before moving registry (for system prompt routing rules)
            let tool_names: Vec<String> = tool_registry
                .names()
                .iter()
                .map(|s| s.to_string())
                .collect();
            let mut agent = if let Some(p) = provider {
                let mut a = agent::AgentLoop::new(
                    p,
                    shared_config.clone(),
                    session_manager.clone(),
                    tool_registry,
                    db,
                )
                .await;
                a.set_message_tx(tool_msg_tx);
                a.set_registered_tool_names(tool_names);

                // Initialize memory searcher (vector + FTS5 hybrid search)
                #[cfg(feature = "local-embeddings")]
                {
                    let cfg = shared_config.read().await;
                    if let Some(searcher) = try_create_memory_searcher(db_for_searcher, &cfg) {
                        a.set_memory_searcher(searcher);
                    }
                }

                Some(a)
            } else {
                None
            };

            // Load installed skills and inject into the agent's system prompt
            let mut skill_registry = skills::SkillRegistry::new();
            if let Err(e) = skill_registry.scan_and_load().await {
                tracing::warn!(error = %e, "Failed to load skills");
            }

            // Wrap in Arc<RwLock<>> so the agent can activate skills on-demand
            let skill_registry = Arc::new(tokio::sync::RwLock::new(skill_registry));

            if let Some(ref mut a) = agent {
                {
                    let reg = skill_registry.read().await;
                    if !reg.is_empty() {
                        a.set_skills_summary(reg.build_prompt_summary()).await;
                        tracing::info!(
                            skills = reg.len(),
                            "Skills loaded into agent context (gateway)"
                        );
                    }
                }
                a.set_skill_registry(skill_registry.clone());

                // Inject available channels info for cross-channel messaging
                let active_channels = config.channels.active_channels_with_chat_ids();
                if !active_channels.is_empty() {
                    let channel_refs: Vec<(&str, &str)> = active_channels
                        .iter()
                        .map(|(name, id)| (name.as_str(), id.as_str()))
                        .collect();
                    a.set_channels_info(&channel_refs);
                    tracing::info!(
                        channels = ?active_channels.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
                        "Cross-channel routing info injected into agent context"
                    );
                }

                // Inject email account info (name + mode) into system prompt
                let email_accounts: Vec<(String, crate::config::EmailMode)> = config
                    .channels
                    .active_email_accounts()
                    .into_iter()
                    .map(|(name, acc)| (name.clone(), acc.mode.clone()))
                    .collect();
                if !email_accounts.is_empty() {
                    a.set_email_accounts_info(&email_accounts);
                    tracing::info!(
                        accounts = ?email_accounts.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
                        "Email account info injected into agent context"
                    );
                }
            }

            // If no provider, start a setup-only Web UI and wait
            let Some(agent) = agent else {
                #[cfg(feature = "web-ui")]
                {
                    let web_config = config.clone();
                    let web_port = config.channels.web.port;
                    let web_server = crate::web::server::WebServer::setup_only(web_config);
                    tokio::spawn(async move {
                        if let Err(e) = web_server.start().await {
                            tracing::error!(error = %e, "Web UI server failed");
                        }
                    });
                    tracing::info!(
                        port = web_port,
                        "Web UI available at http://localhost:{web_port}/"
                    );
                    tracing::info!(
                        "Gateway running in setup mode. Configure a provider via Web UI."
                    );
                    // Wait forever (or until Ctrl+C)
                    tokio::signal::ctrl_c().await?;
                    return Ok(());
                }
                #[cfg(not(feature = "web-ui"))]
                {
                    tracing::error!("No provider configured and web-ui feature is disabled. Cannot start gateway.");
                    return Err(anyhow::anyhow!(
                        "No provider configured. Enable web-ui feature or configure a provider."
                    ));
                }
            };

            // Get the shared handles before wrapping agent in Arc
            let skills_summary_handle = agent.skills_summary_handle();
            let (bootstrap_content_handle, bootstrap_files_handle) = agent.bootstrap_handles();

            let agent = Arc::new(agent);

            // Create SubagentManager and bind it to the SpawnTool (late initialization via OnceCell)
            let (subagent_result_tx, _subagent_result_rx) = tokio::sync::mpsc::channel(50);
            let subagent_manager = Arc::new(agent::SubagentManager::new(
                agent.clone(),
                subagent_result_tx,
            ));
            if spawn_manager_cell.set(subagent_manager).is_err() {
                tracing::error!("SpawnTool OnceCell was already initialized — this is a bug");
            }

            tracing::info!("Subagent manager initialized (SpawnTool registered)");

            // Start skill hot-reload watcher (watches ~/.homun/skills/ for changes)
            let skills_dir = config::Config::data_dir().join("skills");
            let skill_watcher = skills::SkillWatcher::new(skills_summary_handle, skills_dir);
            let _watcher_handle = skill_watcher.start();

            // Start bootstrap file hot-reload watcher (watches ~/.homun/brain/ and ~/.homun/)
            // Now uses BOTH handles so the new modular prompt system stays synchronized across channels
            let data_dir = config::Config::data_dir();
            let bootstrap_watcher = agent::BootstrapWatcher::new(
                bootstrap_content_handle,
                bootstrap_files_handle,
                data_dir,
            );
            let _bootstrap_watcher_handle = bootstrap_watcher.start();

            let mut gateway = agent::Gateway::new(
                agent,
                shared_config,
                session_manager,
                cron_scheduler,
                cron_event_rx,
                db_for_web,
            );
            gateway.set_tool_message_rx(tool_msg_rx);

            // Run gateway and clean up PID file on exit
            let result = gateway.run().await;

            // Clean up PID file
            let _ = std::fs::remove_file(&pid_file);

            result?;
        }
        Commands::Config { command } => {
            use crate::config::dotpath;

            match command {
                None => {
                    // No subcommand → launch TUI dashboard
                    let config = Config::load()?;
                    tui::run_dashboard(config).await?;
                }
                Some(ConfigCommands::Show) => {
                    let config = Config::load()?;
                    let keys = dotpath::config_list_keys(&config);
                    for (key, value) in &keys {
                        println!("{:<40} {}", key, value);
                    }
                }
                Some(ConfigCommands::Get { key }) => {
                    let config = Config::load()?;
                    match dotpath::config_get(&config, &key) {
                        Ok(value) => println!("{value}"),
                        Err(e) => {
                            eprintln!("Error: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                Some(ConfigCommands::Set { key, value }) => {
                    let mut config = Config::load()?;
                    match dotpath::config_set(&mut config, &key, &value) {
                        Ok(()) => {
                            config.save()?;
                            println!("Set {key} = {value}");
                        }
                        Err(e) => {
                            eprintln!("Error: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                Some(ConfigCommands::Init) => {
                    let path = Config::default_path();
                    if path.exists() {
                        println!("Config already exists at {}", path.display());
                    } else {
                        let config = Config::default();
                        config.save()?;
                        println!("Created default config at {}", path.display());
                        println!("Edit it to add your API keys.");
                    }
                }
                Some(ConfigCommands::Path) => {
                    println!("{}", Config::default_path().display());
                }
            }
        }
        Commands::Provider { command } => {
            let mut config = Config::load()?;

            match command {
                ProviderCommands::List => {
                    let active = config
                        .resolve_provider(&config.agent.model)
                        .map(|(name, _)| name.to_string());

                    println!("LLM Providers:\n");
                    for (name, pc) in config.providers.iter() {
                        let configured = !pc.api_key.is_empty() || pc.api_base.is_some();
                        let status = if configured { "\u{2713}" } else { "\u{2717}" };
                        let active_mark = if active.as_deref() == Some(name) {
                            " (active)"
                        } else {
                            ""
                        };
                        let key_display = if pc.api_key.is_empty() {
                            "—".to_string()
                        } else if pc.api_key.len() > 6 {
                            format!("{}***", &pc.api_key[..6])
                        } else {
                            "***".to_string()
                        };
                        let base = pc.api_base.as_deref().unwrap_or("(default)");
                        println!(
                            "  [{status}] {name:<14} key={key_display:<20} base={base}{active_mark}"
                        );
                    }
                }
                ProviderCommands::Add {
                    name,
                    api_key,
                    api_base,
                } => {
                    if let Some(pc) = config.providers.get_mut(&name) {
                        if let Some(key) = api_key {
                            pc.api_key = key;
                        }
                        if let Some(base) = api_base {
                            pc.api_base = Some(base);
                        }
                        config.save()?;
                        println!("Provider '{name}' configured.");
                    } else {
                        eprintln!(
                            "Unknown provider '{name}'. Known: {}",
                            config::ProvidersConfig::known_names().join(", ")
                        );
                        std::process::exit(1);
                    }
                }
                ProviderCommands::Remove { name } => {
                    if let Some(pc) = config.providers.get_mut(&name) {
                        pc.api_key.clear();
                        pc.api_base = None;
                        pc.extra_headers.clear();
                        config.save()?;
                        println!("Provider '{name}' removed.");
                    } else {
                        eprintln!("Unknown provider '{name}'.");
                        std::process::exit(1);
                    }
                }
            }
        }
        Commands::Status => {
            println!("🧪 Homun v{}", env!("CARGO_PKG_VERSION"));
            let config = Config::load()?;
            println!("Model: {}", config.agent.model);
            if let Some((name, _)) = config.resolve_provider(&config.agent.model) {
                println!("Provider: {}", name);
            } else {
                println!("Provider: (none configured)");
            }
            println!("Config: {}", Config::default_path().display());
            println!("Data: {}", Config::data_dir().display());

            // Check if gateway is running via PID file
            let pid_file = Config::data_dir().join("homun.pid");
            if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
                let pid = pid_str.trim();
                if is_process_alive(pid) {
                    println!("Gateway: running (PID {pid})");
                } else {
                    println!("Gateway: not running (stale PID file)");
                    let _ = std::fs::remove_file(&pid_file);
                }
            } else {
                println!("Gateway: not running");
            }
        }
        Commands::Skills { command } => match command {
            SkillsCommands::List => {
                use crate::skills::SkillInstaller;
                match SkillInstaller::list_installed().await {
                    Ok(skills) => {
                        if skills.is_empty() {
                            println!("No skills installed.");
                            println!("Install one with: homun skills add owner/repo");
                        } else {
                            println!("Installed skills:\n");
                            for skill in &skills {
                                println!("  {} — {}", skill.name, skill.description);
                                println!("    {}", skill.path.display());
                            }
                            println!("\n{} skill(s) installed.", skills.len());
                        }
                    }
                    Err(e) => {
                        eprintln!("Error listing skills: {e}");
                    }
                }
            }
            SkillsCommands::Search { query, limit } => {
                use crate::skills::search::SkillSearcher;
                let searcher = SkillSearcher::new();
                match searcher.search(&query, limit).await {
                    Ok(results) => {
                        if results.is_empty() {
                            println!("No skills found for '{query}'.");
                            println!("Try a different search term or browse https://skills.sh/");
                        } else {
                            println!("Skills matching '{query}':\n");
                            for r in &results {
                                println!(
                                    "  \u{2605}{:<5} {} — {}",
                                    r.stars, r.full_name, r.description
                                );
                            }
                            println!("\nInstall with: homun skills add owner/repo");
                        }
                    }
                    Err(e) => {
                        eprintln!("Search failed: {e}");
                        std::process::exit(1);
                    }
                }
            }
            SkillsCommands::Info { name } => {
                use crate::skills::list_skill_scripts;
                use crate::skills::SkillInstaller;
                match SkillInstaller::list_installed().await {
                    Ok(skills) => {
                        if let Some(skill) = skills.iter().find(|s| s.name == name) {
                            println!("Skill: {}", skill.name);
                            println!("Description: {}", skill.description);
                            println!("Path: {}", skill.path.display());
                            let scripts = list_skill_scripts(&skill.path);
                            if !scripts.is_empty() {
                                println!("Scripts: {}", scripts.join(", "));
                            }
                        } else {
                            eprintln!("Skill '{name}' not found. Use 'homun skills list' to see installed skills.");
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    }
                }
            }
            SkillsCommands::Hub { query, limit } => {
                use crate::skills::ClawHubInstaller;
                println!("Searching ClawHub for '{query}'...\n");
                let hub = ClawHubInstaller::new();
                match hub.search(&query, limit).await {
                    Ok(results) => {
                        if results.is_empty() {
                            println!("No skills found for '{query}' on ClawHub.");
                            println!("Browse all skills at https://clawhub.ai/skills");
                        } else {
                            println!("ClawHub skills matching '{query}':\n");
                            for r in &results {
                                println!("  {} — {}", r.slug, r.description);
                            }
                            println!("\n{} result(s). Install with: homun skills add clawhub:owner/skill", results.len());
                        }
                    }
                    Err(e) => {
                        eprintln!("ClawHub search failed: {e}");
                        std::process::exit(1);
                    }
                }
            }
            SkillsCommands::Add { repo, force } => {
                let security_options = crate::skills::InstallSecurityOptions { force };
                if let Some(clawhub_slug) = repo.strip_prefix("clawhub:") {
                    // Install from ClawHub
                    use crate::skills::ClawHubInstaller;
                    println!("Installing skill from ClawHub: {clawhub_slug}...");
                    let hub = ClawHubInstaller::new();
                    match hub
                        .install_with_options(clawhub_slug, security_options.clone())
                        .await
                    {
                        Ok(result) => {
                            if result.already_existed {
                                println!(
                                    "Skill '{}' is already installed at {}",
                                    result.name,
                                    result.path.display()
                                );
                                println!(
                                    "Remove it first with: homun skills remove {}",
                                    result.name
                                );
                            } else {
                                println!(
                                    "\u{2705} Installed '{}' from ClawHub — {}",
                                    result.name, result.description
                                );
                                println!("  Source: clawhub:{clawhub_slug}");
                                println!("  Path: {}", result.path.display());
                                print_install_security_summary(
                                    result.security_report.as_ref(),
                                    force,
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to install from ClawHub: {e}");
                            std::process::exit(1);
                        }
                    }
                } else {
                    // Install from GitHub
                    use crate::skills::SkillInstaller;
                    println!("Installing skill from GitHub: {repo}...");
                    let installer = SkillInstaller::new();
                    match installer
                        .install_with_options(&repo, security_options)
                        .await
                    {
                        Ok(result) => {
                            if result.already_existed {
                                println!(
                                    "Skill '{}' is already installed at {}",
                                    result.name,
                                    result.path.display()
                                );
                                println!(
                                    "Remove it first with: homun skills remove {}",
                                    result.name
                                );
                            } else {
                                println!(
                                    "\u{2705} Installed '{}' from GitHub — {}",
                                    result.name, result.description
                                );
                                println!("  Path: {}", result.path.display());
                                print_install_security_summary(
                                    result.security_report.as_ref(),
                                    force,
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to install from GitHub: {e}");
                            std::process::exit(1);
                        }
                    }
                }
            }
            SkillsCommands::Remove { name } => {
                use crate::skills::SkillInstaller;
                match SkillInstaller::remove(&name).await {
                    Ok(()) => {
                        println!("Skill '{}' removed.", name);
                        let config = Config::load()?;
                        match Database::open(&config.storage.resolved_path()).await {
                            Ok(db) => {
                                let reason = format!("Missing skill dependency: {name}");
                                match db
                                    .invalidate_automations_by_dependency("skill", &name, &reason)
                                    .await
                                {
                                    Ok(affected) if affected > 0 => {
                                        println!(
                                            "Invalidated {affected} automation(s) depending on skill '{name}'."
                                        );
                                    }
                                    Ok(_) => {}
                                    Err(e) => {
                                        eprintln!(
                                            "Warning: failed to invalidate dependent automations: {e}"
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "Warning: could not open DB to invalidate automations: {e}"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to remove skill: {e}");
                        std::process::exit(1);
                    }
                }
            }
        },
        Commands::Mcp { command } => {
            use crate::config::McpServerConfig;

            let mut config = Config::load()?;

            match command {
                McpCommands::List => {
                    if config.mcp.servers.is_empty() {
                        println!("No MCP servers configured.");
                        println!("Add one with: homun mcp add <name> --command npx --args -y @modelcontextprotocol/server-xxx");
                    } else {
                        println!("MCP Servers:\n");
                        for (name, server) in &config.mcp.servers {
                            let status = if server.enabled {
                                "\u{2713}"
                            } else {
                                "\u{2717}"
                            };
                            let detail = match server.transport.as_str() {
                                "stdio" => {
                                    let cmd = server.command.as_deref().unwrap_or("?");
                                    let args = server.args.join(" ");
                                    format!("{cmd} {args}")
                                }
                                "http" => server.url.as_deref().unwrap_or("?").to_string(),
                                _ => server.transport.clone(),
                            };
                            println!("  [{status}] {name:<16} {:<8} {detail}", server.transport);
                        }
                        println!("\n{} server(s) configured.", config.mcp.servers.len());
                    }
                }
                McpCommands::Catalog => {
                    let presets = crate::skills::all_mcp_presets();
                    println!("Curated MCP presets:\n");
                    for preset in presets {
                        println!("  {:<18} {}", preset.id, preset.display_name);
                        println!("      {}", preset.description);
                        println!(
                            "      Command: {} {}",
                            preset.command,
                            preset
                                .args
                                .iter()
                                .map(|a| crate::mcp_setup::render_mcp_arg_template(a))
                                .collect::<Vec<_>>()
                                .join(" ")
                        );
                        if preset.env.is_empty() {
                            println!("      Env: none");
                        } else {
                            let env = preset
                                .env
                                .iter()
                                .map(|e| {
                                    if e.secret {
                                        format!(
                                            "{} (secret{})",
                                            e.key,
                                            if e.required { ", required" } else { "" }
                                        )
                                    } else if e.required {
                                        format!("{} (required)", e.key)
                                    } else {
                                        e.key.clone()
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join(", ");
                            println!("      Env: {env}");
                        }
                        if let Some(url) = &preset.docs_url {
                            println!("      Docs: {url}");
                        }
                        println!();
                    }
                    println!("Use: homun mcp setup <service> [--env KEY=VALUE ...]");
                }
                McpCommands::Add {
                    name,
                    transport,
                    command,
                    args,
                    url,
                } => {
                    let server = McpServerConfig {
                        transport,
                        command,
                        args,
                        url,
                        env: std::collections::HashMap::new(),
                        enabled: true,
                    };
                    config.mcp.servers.insert(name.clone(), server);
                    config.save()?;
                    println!("MCP server '{name}' added.");
                }
                McpCommands::Setup {
                    service,
                    name,
                    env,
                    overwrite,
                    skip_test,
                } => {
                    let Some(preset) = crate::skills::find_mcp_preset(&service) else {
                        eprintln!("Unknown MCP preset '{service}'.");
                        eprintln!("Run 'homun mcp catalog' to see available services.");
                        std::process::exit(1);
                    };

                    let server_name = name.unwrap_or_else(|| preset.id.clone());
                    let env_overrides = crate::mcp_setup::parse_env_assignments(&env)?;

                    let result = crate::mcp_setup::apply_mcp_preset_setup(
                        &mut config,
                        &preset,
                        &server_name,
                        &env_overrides,
                        overwrite,
                    )?;

                    config.save()?;
                    println!(
                        "MCP preset '{}' configured as server '{}'.",
                        preset.id, server_name
                    );

                    if !result.stored_vault_keys.is_empty() {
                        println!(
                            "Stored {} secret(s) in vault:",
                            result.stored_vault_keys.len()
                        );
                        for key in &result.stored_vault_keys {
                            println!("  - vault://{key}");
                        }
                    }

                    if !result.missing_required_env.is_empty() {
                        println!("\nSetup saved, but required env vars are still missing:");
                        for env_key in &result.missing_required_env {
                            println!("  - {env_key}");
                        }
                        println!("\nProvide them with:");
                        println!(
                            "  homun mcp setup {} --name {} --overwrite --env KEY=VALUE ...",
                            preset.id, server_name
                        );
                    } else if skip_test {
                        println!("Connection test skipped (--skip-test).");
                    } else {
                        #[cfg(feature = "mcp")]
                        {
                            if let Some(server) = config.mcp.servers.get(&server_name) {
                                print!("Testing MCP connection... ");
                                let report = crate::mcp_setup::test_mcp_server_connection(
                                    &server_name,
                                    server,
                                    Some(config.security.execution_sandbox.clone()),
                                )
                                .await;
                                if report.connected {
                                    println!("OK");
                                } else {
                                    println!("FAILED");
                                    eprintln!(
                                        "Server is configured but connection test failed. Verify command/env values."
                                    );
                                    if let Some(err) = report.error {
                                        eprintln!("Reason: {err}");
                                    }
                                    std::process::exit(1);
                                }
                            }
                        }

                        #[cfg(not(feature = "mcp"))]
                        {
                            println!(
                                "MCP runtime feature is disabled in this build; skipping connection test."
                            );
                        }
                    }
                }
                McpCommands::Remove { name } => {
                    if config.mcp.servers.remove(&name).is_some() {
                        config.save()?;
                        println!("MCP server '{name}' removed.");
                        match Database::open(&config.storage.resolved_path()).await {
                            Ok(db) => {
                                let reason = format!("Missing or disabled MCP dependency: {name}");
                                match db
                                    .invalidate_automations_by_dependency("mcp", &name, &reason)
                                    .await
                                {
                                    Ok(affected) if affected > 0 => {
                                        println!(
                                            "Invalidated {affected} automation(s) depending on MCP '{name}'."
                                        );
                                    }
                                    Ok(_) => {}
                                    Err(e) => {
                                        eprintln!(
                                            "Warning: failed to invalidate dependent automations: {e}"
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "Warning: could not open DB to invalidate automations: {e}"
                                );
                            }
                        }
                    } else {
                        eprintln!("MCP server '{name}' not found.");
                        std::process::exit(1);
                    }
                }
                McpCommands::Toggle { name } => {
                    if let Some(server) = config.mcp.servers.get_mut(&name) {
                        server.enabled = !server.enabled;
                        let state = if server.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        };
                        config.save()?;
                        println!("MCP server '{name}' {state}.");
                    } else {
                        eprintln!("MCP server '{name}' not found.");
                        std::process::exit(1);
                    }
                }
            }
        }
        Commands::Cron { command } => {
            let config = Config::load()?;
            let db = Database::open(&config.storage.resolved_path()).await?;

            match command {
                CronCommands::List => {
                    let jobs = db.load_cron_jobs().await?;
                    if jobs.is_empty() {
                        println!("No cron jobs scheduled.");
                        println!("Add one with: homun cron add --name \"my-job\" --message \"task\" --cron \"0 9 * * *\"");
                    } else {
                        println!("Scheduled jobs:\n");
                        for job in &jobs {
                            let status = if job.enabled { "✓" } else { "✗" };
                            let last = job.last_run.as_deref().unwrap_or("never");
                            println!("  [{status}] {id} | {name}", id = job.id, name = job.name);
                            println!("      Schedule: {}", job.schedule);
                            println!("      Message: {}", job.message);
                            println!("      Last run: {last}");
                            if let Some(deliver) = &job.deliver_to {
                                println!("      Deliver to: {deliver}");
                            }
                            println!();
                        }
                        println!("{} job(s) total.", jobs.len());
                    }
                }
                CronCommands::Add {
                    name,
                    message,
                    cron,
                    every,
                } => {
                    let schedule = if let Some(cron_expr) = cron {
                        format!("cron:{cron_expr}")
                    } else if let Some(secs) = every {
                        format!("every:{secs}")
                    } else {
                        eprintln!("Either --cron or --every must be specified.");
                        eprintln!("  --cron \"0 9 * * *\"  (cron expression)");
                        eprintln!("  --every 300          (every N seconds)");
                        std::process::exit(1);
                    };

                    let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
                    db.insert_cron_job(&id, &name, &message, &schedule, None)
                        .await?;
                    println!("Job created: id={id}, name={name}, schedule={schedule}");
                    println!("Note: Jobs run when the gateway is active (homun gateway)");
                }
                CronCommands::Remove { id } => {
                    let removed = db.delete_cron_job(&id).await?;
                    if removed {
                        println!("Job '{id}' removed.");
                    } else {
                        eprintln!("Job '{id}' not found.");
                        std::process::exit(1);
                    }
                }
            }
        }
        Commands::Automations { command } => {
            let config = Config::load()?;
            let db = Database::open(&config.storage.resolved_path()).await?;

            match command {
                AutomationCommands::List => {
                    let automations = db.load_automations().await?;
                    if automations.is_empty() {
                        println!("No automations found.");
                        println!(
                            "Add one with: homun automations add --name \"daily\" --prompt \"Morning briefing\" --cron \"0 9 * * *\""
                        );
                    } else {
                        println!("Automations:\n");
                        for a in &automations {
                            let enabled = if a.enabled { "✓" } else { "✗" };
                            let last = a.last_run.as_deref().unwrap_or("never");
                            let target = a.deliver_to.as_deref().unwrap_or("cli:default");
                            println!("  [{enabled}] {id} | {name}", id = a.id, name = a.name);
                            println!("      Schedule: {}", a.schedule);
                            println!("      Status: {}", a.status);
                            println!("      Deliver to: {target}");
                            if let Some(v) = &a.trigger_value {
                                println!("      Trigger: {} ({v})", a.trigger_kind);
                            } else {
                                println!("      Trigger: {}", a.trigger_kind);
                            }
                            println!("      Last run: {last}");
                            if let Some(res) = &a.last_result {
                                println!("      Last result: {res}");
                            }
                            println!();
                        }
                        println!("{} automation(s) total.", automations.len());
                    }
                }
                AutomationCommands::Add {
                    name,
                    prompt,
                    cron,
                    every,
                    deliver_to,
                    trigger,
                    trigger_value,
                    disabled,
                } => {
                    if name.trim().is_empty() {
                        eprintln!("Name cannot be empty.");
                        std::process::exit(1);
                    }
                    if prompt.trim().is_empty() {
                        eprintln!("Prompt cannot be empty.");
                        std::process::exit(1);
                    }
                    let schedule = match (cron, every) {
                        (Some(expr), None) => {
                            crate::scheduler::AutomationSchedule::from_cron(&expr)?.as_stored()
                        }
                        (None, Some(secs)) => {
                            crate::scheduler::AutomationSchedule::from_every(secs)?.as_stored()
                        }
                        _ => {
                            eprintln!("Specify exactly one schedule mode:");
                            eprintln!("  --cron \"0 9 * * *\"");
                            eprintln!("  --every 3600");
                            std::process::exit(1);
                        }
                    };

                    let deliver_to = deliver_to.unwrap_or_else(|| "cli:default".to_string());
                    if deliver_to.rsplit_once(':').is_none() {
                        eprintln!("--deliver-to must be in format channel:chat_id");
                        std::process::exit(1);
                    }

                    let trigger = trigger
                        .as_deref()
                        .unwrap_or("always")
                        .trim()
                        .to_ascii_lowercase()
                        .replace('-', "_");
                    let (trigger_kind, trigger_value) = match trigger.as_str() {
                        "always" => ("always".to_string(), None),
                        "on_change" | "changed" => ("on_change".to_string(), None),
                        "contains" => {
                            let value = trigger_value
                                .as_deref()
                                .map(str::trim)
                                .filter(|v| !v.is_empty())
                                .map(ToOwned::to_owned);
                            if value.is_none() {
                                eprintln!(
                                    "--trigger-value is required when --trigger contains is used."
                                );
                                std::process::exit(1);
                            }
                            ("contains".to_string(), value)
                        }
                        _ => {
                            eprintln!("--trigger must be one of: always, on_change, contains");
                            std::process::exit(1);
                        }
                    };

                    let enabled = !disabled;
                    let compiled_plan = crate::scheduler::automations::compile_automation_plan(
                        prompt.trim(),
                        &config,
                    );
                    let status = if !enabled {
                        "paused"
                    } else if compiled_plan.is_valid() {
                        "active"
                    } else {
                        "invalid_config"
                    };
                    let id = uuid::Uuid::new_v4().to_string();
                    let plan_json = compiled_plan.plan_json();
                    let dependencies_json = compiled_plan.dependencies_json();
                    let validation_errors_json = compiled_plan.validation_errors_json();
                    db.insert_automation_with_plan(
                        &id,
                        name.trim(),
                        prompt.trim(),
                        &schedule,
                        enabled,
                        status,
                        Some(&deliver_to),
                        &trigger_kind,
                        trigger_value.as_deref(),
                        Some(&plan_json),
                        &dependencies_json,
                        compiled_plan.plan.version,
                        validation_errors_json.as_deref(),
                    )
                    .await?;

                    println!("Automation created: id={id}");
                    println!("  Name: {name}");
                    println!("  Schedule: {schedule}");
                    println!("  Deliver to: {deliver_to}");
                    println!(
                        "  Trigger: {}{}",
                        trigger_kind,
                        trigger_value
                            .as_deref()
                            .map(|v| format!(" ({v})"))
                            .unwrap_or_default()
                    );
                    println!("  Enabled: {enabled}");
                    println!("  Status: {status}");
                    if !compiled_plan.is_valid() {
                        println!(
                            "  Validation errors: {}",
                            compiled_plan.validation_errors.join(" | ")
                        );
                    }
                }
                AutomationCommands::Toggle { id } => {
                    let row = db.load_automation(&id).await?;
                    let Some(current) = row else {
                        eprintln!("Automation '{id}' not found.");
                        std::process::exit(1);
                    };
                    let next_enabled = !current.enabled;
                    let next_status = if next_enabled { "active" } else { "paused" };
                    let changed = db
                        .update_automation(
                            &id,
                            crate::storage::AutomationUpdate {
                                enabled: Some(next_enabled),
                                status: Some(next_status.to_string()),
                                ..Default::default()
                            },
                        )
                        .await?;
                    if !changed {
                        eprintln!("Automation '{id}' not updated.");
                        std::process::exit(1);
                    }
                    println!("Automation '{id}' is now {next_status}.");
                }
                AutomationCommands::Run { id } => {
                    let endpoint = format!(
                        "http://{}:{}/api/v1/automations/{}/run",
                        config.channels.web.host, config.channels.web.port, id
                    );
                    let client = reqwest::Client::new();
                    let response = client.post(&endpoint).send().await;

                    let response = match response {
                        Ok(resp) => resp,
                        Err(e) => {
                            eprintln!("Failed to contact runtime API at {endpoint}");
                            eprintln!("Start gateway (and web UI) first: homun gateway");
                            eprintln!("Details: {e}");
                            std::process::exit(1);
                        }
                    };

                    if !response.status().is_success() {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        eprintln!("Runtime API returned {status} for run-now request.");
                        if !body.trim().is_empty() {
                            eprintln!("{body}");
                        }
                        std::process::exit(1);
                    }

                    let json: serde_json::Value = response.json().await.unwrap_or_default();
                    let run_id = json
                        .get("run_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    let status = json
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("queued");
                    let message = json
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Run request accepted");

                    println!("Run requested: {run_id}");
                    println!("  Status: {status}");
                    println!("  Message: {message}");
                }
                AutomationCommands::History { id, limit } => {
                    let runs = db.load_automation_runs(&id, limit).await?;
                    if runs.is_empty() {
                        println!("No runs found for automation '{id}'.");
                    } else {
                        println!("Runs for automation '{id}':\n");
                        for run in runs {
                            let finished = run.finished_at.as_deref().unwrap_or("in-progress");
                            println!(
                                "  {id} | status={status} | started={started} | finished={finished}",
                                id = run.id,
                                status = run.status,
                                started = run.started_at,
                            );
                            if let Some(result) = run.result {
                                println!("      {result}");
                            }
                        }
                    }
                }
                AutomationCommands::Remove { id } => {
                    let removed = db.delete_automation(&id).await?;
                    if removed {
                        println!("Automation '{id}' removed.");
                    } else {
                        eprintln!("Automation '{id}' not found.");
                        std::process::exit(1);
                    }
                }
            }
        }
        Commands::Memory { command } => {
            let data_dir = Config::data_dir();
            match command {
                MemoryCommands::Status => {
                    println!("📊 Memory Status");
                    println!("─────────────────────────────────");

                    // Files
                    let files = [
                        ("MEMORY.md", data_dir.join("MEMORY.md")),
                        ("HISTORY.md", data_dir.join("HISTORY.md")),
                        ("brain/USER.md", data_dir.join("brain").join("USER.md")),
                        (
                            "brain/INSTRUCTIONS.md",
                            data_dir.join("brain").join("INSTRUCTIONS.md"),
                        ),
                        ("brain/SOUL.md", data_dir.join("brain").join("SOUL.md")),
                        ("memory.usearch", data_dir.join("memory.usearch")),
                    ];

                    for (name, path) in &files {
                        if path.exists() {
                            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                            println!("  ✅ {name:<28} ({size} bytes)");
                        } else {
                            println!("  ⬜ {name:<28} (not created)");
                        }
                    }

                    // Daily memory files
                    let memory_dir = data_dir.join("memory");
                    let daily_count = std::fs::read_dir(&memory_dir)
                        .map(|entries| entries.filter_map(|e| e.ok()).count())
                        .unwrap_or(0);
                    if daily_count > 0 {
                        println!("  📁 memory/ (daily)          {daily_count} files");
                    }

                    // Database stats
                    println!("\n📦 Database");
                    let db_path = data_dir.join("homun.db");
                    if db_path.exists() {
                        let db = Database::open(&db_path).await?;

                        let chunks: i64 = db.count_memory_chunks().await?;
                        println!("  memory_chunks: {chunks} rows");

                        let pool = db.pool();
                        let sessions: i64 = sqlx::query_scalar::<_, i64>(
                            "SELECT COUNT(DISTINCT session_key) FROM messages",
                        )
                        .fetch_one(pool)
                        .await
                        .unwrap_or(0);
                        println!("  sessions: {sessions}");

                        let messages: i64 =
                            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM messages")
                                .fetch_one(pool)
                                .await
                                .unwrap_or(0);
                        println!("  messages: {messages}");
                    } else {
                        println!("  (no database found)");
                    }
                }
                MemoryCommands::Reset { force } => {
                    if !force {
                        eprint!(
                            "⚠️  This will delete ALL memory data:\n\
                             \n\
                             • Conversation history (all sessions)\n\
                             • Long-term memory (MEMORY.md, memory chunks)\n\
                             • Brain files (USER.md, INSTRUCTIONS.md, SOUL.md)\n\
                             • Daily memory files\n\
                             • Vector search index\n\
                             \n\
                             Type 'yes' to confirm: "
                        );
                        let mut input = String::new();
                        std::io::stdin().read_line(&mut input)?;
                        if input.trim() != "yes" {
                            println!("Aborted.");
                            return Ok(());
                        }
                    }

                    println!("🗑  Resetting memory...");

                    // 1. Delete files
                    let files_to_delete = [
                        data_dir.join("MEMORY.md"),
                        data_dir.join("HISTORY.md"),
                        data_dir.join("memory.usearch"),
                        data_dir.join("brain").join("USER.md"),
                        data_dir.join("brain").join("INSTRUCTIONS.md"),
                        data_dir.join("brain").join("SOUL.md"),
                    ];

                    for path in &files_to_delete {
                        if path.exists() {
                            std::fs::remove_file(path)?;
                            println!(
                                "  ✓ Deleted {}",
                                path.strip_prefix(&data_dir).unwrap_or(path).display()
                            );
                        }
                    }

                    // 2. Delete daily memory directory
                    let memory_dir = data_dir.join("memory");
                    if memory_dir.exists() {
                        std::fs::remove_dir_all(&memory_dir)?;
                        println!("  ✓ Deleted memory/ (daily files)");
                    }

                    // 3. Clear database tables
                    let db_path = data_dir.join("homun.db");
                    if db_path.exists() {
                        let db = Database::open(&db_path).await?;
                        db.reset_all_memory().await?;
                        println!("  ✓ Cleared database (memory_chunks, memories, messages)");
                    }

                    println!("\n✅ Memory reset complete. Restart the gateway to apply.");
                }
            }
        }
        Commands::Users { command } => {
            let data_dir = Config::data_dir();
            let db_path = data_dir.join("homun.db");
            let db = Database::open(&db_path).await?;
            let user_mgr = user::UserManager::new(db);

            match command {
                UserCommands::List => {
                    println!("👥 Users");
                    println!("─────────────────────────────────");

                    let users = user_mgr.list_users().await?;
                    if users.is_empty() {
                        println!("  No users configured.");
                        println!("\n  Create one with: homun users add <username>");
                    } else {
                        for u in users {
                            let roles: Vec<&str> = u.roles.iter().map(|r| r.as_str()).collect();
                            println!("  • {} ({}) [{}]", u.username, &u.id[..8], roles.join(", "));
                        }
                    }
                }
                UserCommands::Add { name, admin } => {
                    let user = if admin {
                        user_mgr.create_admin(&name).await?
                    } else {
                        user_mgr.create_user(&name).await?
                    };
                    let role = if admin { "admin" } else { "user" };
                    println!(
                        "✅ Created user '{}' with role {} (ID: {})",
                        name, role, user.id
                    );
                }
                UserCommands::Info { user } => {
                    let info = if user.contains('-') && user.len() == 36 {
                        // Looks like a UUID
                        user_mgr.get_user(&user).await?
                    } else {
                        // Treat as username
                        user_mgr.get_user_by_username(&user).await?
                    };

                    match info {
                        Some(u) => {
                            println!("👤 User: {}", u.username);
                            println!("─────────────────────────────────");
                            println!("  ID: {}", u.id);
                            let roles: Vec<&str> = u.roles.iter().map(|r| r.as_str()).collect();
                            println!("  Roles: {}", roles.join(", "));

                            // Show identities
                            let db = user_mgr.db();
                            let identities = db.load_user_identities(&u.id).await?;
                            if !identities.is_empty() {
                                println!("\n  Channel Identities:");
                                for id in identities {
                                    let dn = id
                                        .display_name
                                        .map(|d| format!(" ({})", d))
                                        .unwrap_or_default();
                                    println!("    • {}: {}{}", id.channel, id.platform_id, dn);
                                }
                            }

                            // Show webhook tokens
                            let tokens = db.load_webhook_tokens(&u.id).await?;
                            if !tokens.is_empty() {
                                println!("\n  Webhook Tokens:");
                                for t in tokens {
                                    let status = if t.enabled { "✓" } else { "✗" };
                                    let last = t
                                        .last_used
                                        .map(|l| format!(" (last: {})", l))
                                        .unwrap_or_default();
                                    println!(
                                        "    • [{}] {} – {}{}",
                                        status,
                                        &t.token[..12],
                                        t.name,
                                        last
                                    );
                                }
                            }
                        }
                        None => {
                            println!("❌ User not found: {}", user);
                        }
                    }
                }
                UserCommands::Link {
                    user,
                    channel,
                    id,
                    display_name,
                } => {
                    let info = if user.contains('-') && user.len() == 36 {
                        user_mgr.get_user(&user).await?
                    } else {
                        user_mgr.get_user_by_username(&user).await?
                    };

                    match info {
                        Some(u) => {
                            user_mgr
                                .link_identity(&u.id, &channel, &id, display_name.as_deref())
                                .await?;
                            println!(
                                "✅ Linked {} identity '{}' to user '{}'",
                                channel, id, u.username
                            );
                        }
                        None => {
                            println!("❌ User not found: {}", user);
                        }
                    }
                }
                UserCommands::Unlink { user, channel, id } => {
                    let info = if user.contains('-') && user.len() == 36 {
                        user_mgr.get_user(&user).await?
                    } else {
                        user_mgr.get_user_by_username(&user).await?
                    };

                    match info {
                        Some(u) => {
                            let removed = user_mgr.unlink_identity(&u.id, &channel, &id).await?;
                            if removed {
                                println!(
                                    "✅ Unlinked {} identity '{}' from user '{}'",
                                    channel, id, u.username
                                );
                            } else {
                                println!("⚠️  Identity not found");
                            }
                        }
                        None => {
                            println!("❌ User not found: {}", user);
                        }
                    }
                }
                UserCommands::Token { user, name } => {
                    let info = if user.contains('-') && user.len() == 36 {
                        user_mgr.get_user(&user).await?
                    } else {
                        user_mgr.get_user_by_username(&user).await?
                    };

                    match info {
                        Some(u) => {
                            let token = user_mgr.create_webhook_token(&u.id, &name).await?;
                            println!("✅ Created webhook token for user '{}':", u.username);
                            println!("   Token: {}", token);
                            println!("\n   Usage: POST /api/webhook/{}", token);
                        }
                        None => {
                            println!("❌ User not found: {}", user);
                        }
                    }
                }
                UserCommands::Remove { user } => {
                    let info = if user.contains('-') && user.len() == 36 {
                        user_mgr.get_user(&user).await?
                    } else {
                        user_mgr.get_user_by_username(&user).await?
                    };

                    match info {
                        Some(u) => {
                            let removed = user_mgr.delete_user(&u.id).await?;
                            if removed {
                                println!("✅ Deleted user '{}' ({})", u.username, u.id);
                            } else {
                                println!("⚠️  User not found");
                            }
                        }
                        None => {
                            println!("❌ User not found: {}", user);
                        }
                    }
                }
            }
        }
        Commands::Service { command } => {
            use service::*;
            match command {
                ServiceCommands::Install => {
                    install()?;
                }
                ServiceCommands::Uninstall => {
                    uninstall()?;
                }
                ServiceCommands::Start => {
                    start()?;
                }
                ServiceCommands::Stop => {
                    stop()?;
                }
                ServiceCommands::Status => {
                    let status = status()?;
                    println!("{}", status);
                }
            }
        }
        Commands::Stop => {
            stop_gateway()?;
        }
        Commands::Restart => {
            let was_running = stop_gateway()?;
            if was_running {
                // Small delay to let the port release
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            // Re-exec ourselves with `gateway` argument
            let exe = std::env::current_exe().context("Failed to find homun binary")?;
            let status = std::process::Command::new(exe)
                .arg("gateway")
                .status()
                .context("Failed to start gateway")?;
            std::process::exit(status.code().unwrap_or(1));
        }
    }

    Ok(())
}
