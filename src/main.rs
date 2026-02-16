use anyhow::Result;
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

#[derive(Parser)]
#[command(name = "homunbot", version, about = "🧪 The digital homunculus that lives in your computer")]
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
    /// Initialize or show configuration
    Config,
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
}

#[derive(Subcommand)]
enum SkillsCommands {
    /// List installed skills
    List,
    /// Install a skill from GitHub (owner/repo)
    Add { repo: String },
    /// Remove an installed skill
    Remove { name: String },
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

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Chat { message }) => {
            tracing::info!("Starting chat mode");
            // TODO: implement chat
            if let Some(msg) = message {
                println!("One-shot mode: {msg}");
            } else {
                println!("Interactive mode (not yet implemented)");
            }
        }
        Some(Commands::Gateway) => {
            tracing::info!("Starting gateway");
            // TODO: implement gateway
        }
        Some(Commands::Config) => {
            tracing::info!("Config management");
            // TODO: implement config init
        }
        Some(Commands::Status) => {
            println!("🧪 HomunBot v{}", env!("CARGO_PKG_VERSION"));
            // TODO: show status
        }
        Some(Commands::Skills { command }) => match command {
            SkillsCommands::List => {
                // TODO: list skills
            }
            SkillsCommands::Add { repo } => {
                tracing::info!("Installing skill from {repo}");
                // TODO: install skill
            }
            SkillsCommands::Remove { name } => {
                tracing::info!("Removing skill {name}");
                // TODO: remove skill
            }
        },
        Some(Commands::Cron { command }) => match command {
            CronCommands::List => {
                // TODO: list cron jobs
            }
            CronCommands::Add { name, message, cron, every } => {
                tracing::info!("Adding cron job: {name}");
                // TODO: add cron job
                let _ = (message, cron, every);
            }
            CronCommands::Remove { id } => {
                tracing::info!("Removing cron job: {id}");
                // TODO: remove cron job
            }
        },
        // Default: interactive chat
        None => {
            tracing::info!("Starting interactive chat (default)");
            // TODO: implement interactive chat
            println!("🧪 HomunBot — interactive mode (not yet implemented)");
        }
    }

    Ok(())
}
