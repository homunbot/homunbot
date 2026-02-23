// Allow dead code: this binary exposes a public API design for future lib.rs extraction.
// Many types/functions are pub but only used in specific subcommands.
#![allow(dead_code, unused_imports)]

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

mod agent;
mod bus;
mod channels;
mod config;
mod provider;
mod scheduler;
mod session;
mod skills;
mod storage;
mod tools;
mod tui;
mod utils;
mod web;

use crate::channels::CliChannel;
use crate::config::Config;
use crate::provider::{AnthropicProvider, OpenAICompatProvider};
use crate::session::SessionManager;
use crate::storage::Database;
use crate::tools::{
    CronTool, EditFileTool, ListDirTool, McpManager, MessageTool, ReadFileTool, ShellTool,
    SpawnTool, ToolRegistry, VaultTool, RememberTool, WebFetchTool, WebSearchTool, WriteFileTool,
};

#[derive(Parser)]
#[command(name = "homun", version, about = "🧪 The digital homunculus that lives in your computer")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

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
    },
    /// Remove an installed skill
    Remove { name: String },
}

#[derive(Subcommand)]
enum McpCommands {
    /// List configured MCP servers
    List,
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

/// Create the LLM provider from config.
///
/// Anthropic uses a native provider (different API format with content blocks
/// and tool_use). All other providers use the OpenAI-compatible format.
fn create_provider(config: &Config) -> Result<Arc<dyn provider::Provider>> {
    let model = &config.agent.model;
    let (provider_name, provider_config) = config
        .resolve_provider(model)
        .context("No provider configured. Add an API key to ~/.homun/config.toml")?;

    tracing::info!(
        provider = provider_name,
        model = model,
        "Using LLM provider"
    );

    // Get API key from secure storage (encrypted)
    let api_key = if provider_config.api_key == "***ENCRYPTED***" {
        // Retrieve from secure storage
        let secrets = storage::global_secrets()
            .context("Failed to access secure storage")?;
        let secret_key = storage::SecretKey::provider_api_key(provider_name);
        secrets.get(&secret_key)?
            .unwrap_or_default()
    } else if provider_config.api_key.is_empty() {
        String::new()
    } else {
        // Legacy: key stored in plaintext config — auto-migrate to encrypted storage
        tracing::warn!(
            provider = provider_name,
            "API key for '{}' is in plaintext config.toml — auto-migrating to encrypted storage",
            provider_name
        );
        let plaintext_key = provider_config.api_key.clone();
        if let Ok(secrets) = storage::global_secrets() {
            let secret_key = storage::SecretKey::provider_api_key(provider_name);
            if secrets.set(&secret_key, &plaintext_key).is_ok() {
                // Update config file to replace plaintext with marker
                let mut migrated = config.clone();
                if let Some(pc) = migrated.providers.get_mut(provider_name) {
                    pc.api_key = "***ENCRYPTED***".to_string();
                    if let Err(e) = migrated.save() {
                        tracing::warn!(error = %e, "Failed to save migrated config");
                    } else {
                        tracing::info!(
                            provider = provider_name,
                            "Auto-migrated API key to encrypted storage"
                        );
                    }
                }
            }
        }
        plaintext_key
    };

    if provider_name == "anthropic" {
        // Native Anthropic provider (Claude API with tool_use blocks)
        let provider = AnthropicProvider::new(
            &api_key,
            provider_config.api_base.as_deref(),
            provider_config.extra_headers.clone(),
        );
        Ok(Arc::new(provider))
    } else if provider_name == "ollama" {
        // Native Ollama provider (local /api/chat — NDJSON, native tool calls, think control)
        let provider = provider::OllamaProvider::new(
            &api_key,
            provider_config.api_base.as_deref(),
        );
        Ok(Arc::new(provider))
    } else {
        // OpenAI-compatible provider (covers OpenRouter, OpenAI, DeepSeek, Groq, Gemini,
        // Mistral, xAI, Together, Fireworks, Perplexity, Cohere, Venice, AiHubMix, Vercel,
        // Cloudflare, Copilot, Bedrock, Minimax, DashScope, Moonshot, Zhipu, vLLM, custom,
        // and ollama_cloud for Ollama's hosted service)
        let provider = OpenAICompatProvider::from_config(
            provider_name,
            &api_key,
            provider_config.api_base.as_deref(),
            provider_config.extra_headers.clone(),
        );
        Ok(Arc::new(provider))
    }
}

/// Create and register all tools from config
fn create_tool_registry(config: &Config) -> ToolRegistry {
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

    // Shell tool with OS-specific permissions
    registry.register(Box::new(ShellTool::with_permissions(
        config.tools.exec.timeout,
        config.tools.exec.restrict_to_workspace,
        Some(shell_permissions),
    )));

    // File tools with ACL-based permissions
    registry.register(Box::new(ReadFileTool::with_permissions(allowed_dir.clone(), permissions.clone())));
    registry.register(Box::new(WriteFileTool::with_permissions(allowed_dir.clone(), permissions.clone())));
    registry.register(Box::new(EditFileTool::with_permissions(allowed_dir.clone(), permissions.clone())));
    registry.register(Box::new(ListDirTool::with_permissions(allowed_dir, permissions)));

    // Web search tool (Brave API)
    registry.register(Box::new(WebSearchTool::new(
        &config.tools.web_search.api_key,
        config.tools.web_search.max_results,
    )));

    // Web fetch tool
    registry.register(Box::new(WebFetchTool::new()));

    // Vault tool (encrypted secrets storage)
    registry.register(Box::new(VaultTool::new()));

    // Remember tool (save personal information)
    registry.register(Box::new(RememberTool::new()));

    tracing::info!(
        tools = registry.len(),
        "Tool registry initialized"
    );

    registry
}

/// Try to create a MemorySearcher (embedding engine + vector index).
///
/// Returns `None` if the embedding engine fails to initialize (e.g. ONNX model
/// download fails). This keeps the agent functional without vector search.
fn try_create_memory_searcher(db: Database) -> Option<agent::MemorySearcher> {
    match agent::EmbeddingEngine::new() {
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Default (no subcommand) = interactive chat
    let command = cli.command.unwrap_or(Commands::Chat { message: None });

    // TUI commands use alternate screen — logs on stderr would corrupt the display.
    // Write logs to a file instead, or suppress them entirely.
    let is_tui_command = matches!(&command, Commands::Config { command: None });

    if is_tui_command {
        // During TUI: log to ~/.homun/tui.log so stderr stays clean
        let log_dir = Config::data_dir();
        std::fs::create_dir_all(&log_dir).ok();
        let log_file = std::fs::File::create(log_dir.join("tui.log")).ok();
        if let Some(file) = log_file {
            tracing_subscriber::fmt()
                .with_env_filter(
                    EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| EnvFilter::new("info")),
                )
                .with_writer(file)
                .with_ansi(false)
                .init();
        }
    } else {
        // Normal mode: log to stderr
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .init();
    }

    match command {
        Commands::Chat { message } => {
            let config = Config::load()?;
            let db = Database::open(&config.storage.resolved_path()).await?;
            let provider = create_provider(&config)?;
            let mut tool_registry = create_tool_registry(&config);

            // Connect to MCP servers and register their tools
            let (mcp_manager, mcp_tools) = McpManager::start(&config.mcp.servers).await;
            for tool in mcp_tools {
                tool_registry.register(tool);
            }

            let session_manager = SessionManager::new(db.clone());
            let db_for_searcher = db.clone();
            let mut agent = agent::AgentLoop::new(
                provider,
                config,
                session_manager.clone(),
                tool_registry,
                db,
            );

            // Initialize memory searcher (vector + FTS5 hybrid search)
            if let Some(searcher) = try_create_memory_searcher(db_for_searcher) {
                agent.set_memory_searcher(searcher);
            }

            // Load installed skills and inject into the agent's system prompt
            let mut skill_registry = skills::SkillRegistry::new();
            if let Err(e) = skill_registry.scan_and_load().await {
                tracing::warn!(error = %e, "Failed to load skills");
            }
            if !skill_registry.is_empty() {
                agent.set_skills_summary(skill_registry.build_prompt_summary()).await;
                tracing::info!(skills = skill_registry.len(), "Skills loaded into agent context");
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
            mcp_manager.shutdown().await;
        }
        Commands::Gateway => {
            use std::sync::Arc;
            use crate::scheduler::CronScheduler;

            // Write PID file so `homun stop` / `homun restart` can find us
            let pid_file = Config::data_dir().join("homun.pid");
            std::fs::write(&pid_file, std::process::id().to_string())?;

            let config = Config::load()?;
            let db = Database::open(&config.storage.resolved_path()).await?;

            // Try to create provider, but allow gateway to start without one
            // This enables configuration via Web UI
            let provider = match create_provider(&config) {
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

            // Build tool registry with CronTool + MessageTool + MCP tools
            let mut tool_registry = create_tool_registry(&config);
            tool_registry.register(Box::new(CronTool::new(cron_scheduler.clone())));
            tool_registry.register(Box::new(MessageTool::new()));

            // Connect to MCP servers and register their tools
            let (_mcp_manager, mcp_tools) = McpManager::start(&config.mcp.servers).await;
            for tool in mcp_tools {
                tool_registry.register(tool);
            }

            // Create tool message channel for proactive messaging (MessageTool → Gateway → Channel)
            let (tool_msg_tx, tool_msg_rx) = tokio::sync::mpsc::channel(100);

            // Create agent only if provider is available
            let db_for_searcher = db.clone();
            let db_for_web = db.clone();
            let mut agent = if let Some(p) = provider {
                let mut a = agent::AgentLoop::new(
                    p,
                    config.clone(),
                    session_manager.clone(),
                    tool_registry,
                    db,
                );
                a.set_message_tx(tool_msg_tx);

                // Initialize memory searcher (vector + FTS5 hybrid search)
                if let Some(searcher) = try_create_memory_searcher(db_for_searcher) {
                    a.set_memory_searcher(searcher);
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
                        tracing::info!(skills = reg.len(), "Skills loaded into agent context (gateway)");
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
            }

            // If no provider, start a setup-only Web UI and wait
            let Some(agent) = agent else {
                let web_config = config.clone();
                let web_port = config.channels.web.port;
                let web_server = crate::web::server::WebServer::setup_only(web_config);
                tokio::spawn(async move {
                    if let Err(e) = web_server.start().await {
                        tracing::error!(error = %e, "Web UI server failed");
                    }
                });
                tracing::info!(port = web_port, "Web UI available at http://localhost:{web_port}/");
                tracing::info!("Gateway running in setup mode. Configure a provider via Web UI.");
                // Wait forever (or until Ctrl+C)
                tokio::signal::ctrl_c().await?;
                return Ok(());
            };

            // Get the shared handles before wrapping agent in Arc
            let skills_summary_handle = agent.skills_summary_handle();
            let (bootstrap_content_handle, bootstrap_files_handle) = agent.bootstrap_handles();

            let agent = Arc::new(agent);

            // Create SubagentManager and register SpawnTool
            let (subagent_result_tx, _subagent_result_rx) = tokio::sync::mpsc::channel(50);
            let _subagent_manager = Arc::new(agent::SubagentManager::new(
                agent.clone(),
                subagent_result_tx,
            ));
            // Note: SpawnTool uses the SubagentManager but needs access to agent's tool_registry.
            // For now, subagent spawning is available through the gateway.
            // TODO: register SpawnTool in the tool_registry (requires registry to accept post-creation tools)

            tracing::info!("Subagent manager initialized");

            // Start skill hot-reload watcher (watches ~/.homun/skills/ for changes)
            let skills_dir = config::Config::data_dir().join("skills");
            let skill_watcher = skills::SkillWatcher::new(
                skills_summary_handle,
                skills_dir,
            );
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
                agent, config, session_manager, cron_scheduler, cron_event_rx, db_for_web,
            );
            gateway.set_tool_message_rx(tool_msg_rx);
            gateway.run().await?;
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
                use crate::skills::SkillInstaller;
                use crate::skills::list_skill_scripts;
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
            SkillsCommands::Add { repo } => {
                if let Some(clawhub_slug) = repo.strip_prefix("clawhub:") {
                    // Install from ClawHub
                    use crate::skills::ClawHubInstaller;
                    println!("Installing skill from ClawHub: {clawhub_slug}...");
                    let hub = ClawHubInstaller::new();
                    match hub.install(clawhub_slug).await {
                        Ok(result) => {
                            if result.already_existed {
                                println!("Skill '{}' is already installed at {}", result.name, result.path.display());
                                println!("Remove it first with: homun skills remove {}", result.name);
                            } else {
                                println!("\u{2705} Installed '{}' from ClawHub — {}", result.name, result.description);
                                println!("  Source: clawhub:{clawhub_slug}");
                                println!("  Path: {}", result.path.display());
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
                    match installer.install(&repo).await {
                        Ok(result) => {
                            if result.already_existed {
                                println!("Skill '{}' is already installed at {}", result.name, result.path.display());
                                println!("Remove it first with: homun skills remove {}", result.name);
                            } else {
                                println!("\u{2705} Installed '{}' from GitHub — {}", result.name, result.description);
                                println!("  Path: {}", result.path.display());
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
                            let status = if server.enabled { "\u{2713}" } else { "\u{2717}" };
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
                McpCommands::Remove { name } => {
                    if config.mcp.servers.remove(&name).is_some() {
                        config.save()?;
                        println!("MCP server '{name}' removed.");
                    } else {
                        eprintln!("MCP server '{name}' not found.");
                        std::process::exit(1);
                    }
                }
                McpCommands::Toggle { name } => {
                    if let Some(server) = config.mcp.servers.get_mut(&name) {
                        server.enabled = !server.enabled;
                        let state = if server.enabled { "enabled" } else { "disabled" };
                        config.save()?;
                        println!("MCP server '{name}' {state}.");
                    } else {
                        eprintln!("MCP server '{name}' not found.");
                        std::process::exit(1);
                    }
                }
            }
        },
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
                CronCommands::Add { name, message, cron, every } => {
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
                    db.insert_cron_job(&id, &name, &message, &schedule, None).await?;
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
        },
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
                        ("brain/INSTRUCTIONS.md", data_dir.join("brain").join("INSTRUCTIONS.md")),
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
                        let sessions: i64 =
                            sqlx::query_scalar::<_, i64>("SELECT COUNT(DISTINCT session_key) FROM messages")
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
                            println!("  ✓ Deleted {}", path.strip_prefix(&data_dir)
                                .unwrap_or(path).display());
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
