use std::io::{self, BufRead, Write};

use anyhow::Result;

use crate::agent::AgentLoop;
use crate::session::SessionManager;

/// CLI channel — interactive REPL and one-shot mode.
///
/// Modeled after nanobot's cli/commands.py:
/// - One-shot: send a single message, print response, exit
/// - Interactive: REPL loop with prompt, history, exit commands
pub struct CliChannel {
    agent: AgentLoop,
    session_manager: SessionManager,
    session_key: String,
}

impl CliChannel {
    pub fn new(agent: AgentLoop, session_manager: SessionManager) -> Self {
        Self {
            agent,
            session_manager,
            session_key: "cli:default".to_string(),
        }
    }

    /// One-shot mode: send a message and return the response
    pub async fn one_shot(&self, message: &str) -> Result<String> {
        self.agent
            .process_message(message, &self.session_key, "cli", "local")
            .await
    }

    /// Interactive REPL mode
    pub async fn interactive(&self) -> Result<()> {
        println!("🧪 Homun — interactive mode");
        println!("Type your message. Commands: /new (reset), /quit (exit)\n");

        let stdin = io::stdin();
        let mut stdout = io::stdout();

        loop {
            // Print prompt
            print!("you> ");
            stdout.flush()?;

            // Read input
            let mut input = String::new();
            let bytes_read = stdin.lock().read_line(&mut input)?;

            // EOF (Ctrl+D)
            if bytes_read == 0 {
                println!();
                break;
            }

            let input = input.trim();

            // Empty input
            if input.is_empty() {
                continue;
            }

            // Exit commands
            if matches!(input, "/quit" | "/exit" | "exit" | "quit" | ":q") {
                break;
            }

            // New session — clear conversation history
            if input == "/new" {
                if let Err(e) = self.session_manager.clear(&self.session_key).await {
                    tracing::error!(error = %e, "Failed to clear session");
                    println!("[error] Failed to clear session: {}\n", e);
                } else {
                    println!("Session cleared.\n");
                }
                continue;
            }

            // Process message
            match self
                .agent
                .process_message(input, &self.session_key, "cli", "local")
                .await
            {
                Ok(response) => {
                    println!("\nhomun> {}\n", response);
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to process message");
                    println!("\n[error] {}\n", e);
                }
            }
        }

        println!("Goodbye! 🧪");
        Ok(())
    }
}
