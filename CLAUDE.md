# HomunBot — Claude Code Instructions

> **Read `PROJECT.md` first** for the full vision, positioning, architecture philosophy, and development phases.
> This file contains the technical implementation guidelines for writing code.

## What is HomunBot

HomunBot is an ultra-lightweight personal AI assistant written in Rust — a digital homunculus that lives in your computer and works for you 24/7. You manage it remotely via Telegram, WhatsApp, or CLI. It supports the open **Agent Skills** standard (skills.sh / agentskills spec) for extensible capabilities.

**Core philosophy**: single binary, local-first, privacy-focused, skill-powered.

Inspired by [nanobot](https://github.com/HKUDS/nanobot) (~4k lines Python) but rewritten from scratch in Rust for performance, reliability, and zero-dependency deployment.

## Architecture Overview

```
homunbot/
├── src/
│   ├── main.rs                 # Entry point, CLI setup
│   ├── lib.rs                  # Public API re-exports
│   ├── agent/
│   │   ├── mod.rs
│   │   ├── loop.rs             # Core agent loop (ReAct: reason → act → observe)
│   │   ├── context.rs          # Prompt/context builder
│   │   ├── memory.rs           # Long-term memory (consolidation via LLM)
│   │   └── subagent.rs         # Background task execution
│   ├── provider/
│   │   ├── mod.rs
│   │   ├── traits.rs           # Provider trait definition
│   │   ├── openai_compat.rs    # OpenAI-compatible (covers OpenRouter, Ollama, OpenAI)
│   │   └── anthropic.rs        # Native Anthropic API (streaming, tool_use)
│   ├── tools/
│   │   ├── mod.rs
│   │   ├── registry.rs         # Tool registry and dispatch
│   │   ├── shell.rs            # Shell command execution
│   │   ├── file.rs             # File read/write/edit
│   │   ├── web.rs              # Web search (Brave, Tavily)
│   │   ├── message.rs          # Send message to user
│   │   ├── spawn.rs            # Spawn subagent
│   │   └── cron.rs             # Schedule recurring tasks
│   ├── skills/
│   │   ├── mod.rs
│   │   ├── loader.rs           # Scan dirs, parse SKILL.md YAML frontmatter
│   │   ├── registry.rs         # In-memory skill registry
│   │   ├── installer.rs        # `homunbot skills add owner/repo` (GitHub fetch)
│   │   └── executor.rs         # Run skill scripts (Python/Bash/JS)
│   ├── channels/
│   │   ├── mod.rs
│   │   ├── traits.rs           # Channel trait (send/receive)
│   │   ├── cli.rs              # Interactive CLI / one-shot mode
│   │   ├── telegram.rs         # Telegram bot (teloxide)
│   │   └── whatsapp.rs         # WhatsApp bridge client (connects to Node.js bridge)
│   ├── bus/
│   │   ├── mod.rs
│   │   └── queue.rs            # Message bus (mpsc channels) for routing
│   ├── session/
│   │   ├── mod.rs
│   │   └── manager.rs          # Conversation session state
│   ├── scheduler/
│   │   ├── mod.rs
│   │   └── cron.rs             # Cron job scheduling (tokio-cron-scheduler)
│   ├── storage/
│   │   ├── mod.rs
│   │   └── db.rs               # SQLite via sqlx (memory, sessions, cron jobs)
│   └── config/
│       ├── mod.rs
│       └── schema.rs           # Config structs, deserialized from TOML
├── bridge/                     # WhatsApp Node.js bridge (separate process)
│   ├── src/
│   │   └── whatsapp.ts
│   ├── package.json
│   └── tsconfig.json
├── skills/                     # Bundled default skills
│   └── README.md
├── Cargo.toml
├── CLAUDE.md                   # This file
├── README.md
└── LICENSE                     # MIT
```

## Key Design Decisions

### Runtime & Async
- **Tokio** as async runtime — use `#[tokio::main]` and `tokio::spawn` for concurrency.
- All I/O operations must be async. Never use `std::thread::sleep`, use `tokio::time::sleep`.
- Use `tokio::sync::mpsc` for the internal message bus.

### LLM Provider System
- Define a `Provider` trait in `src/provider/traits.rs`:
  ```rust
  #[async_trait]
  pub trait Provider: Send + Sync {
      async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;
      async fn chat_stream(&self, request: ChatRequest) -> Result<Pin<Box<dyn Stream<Item = Result<ChatChunk>> + Send>>>;
      fn name(&self) -> &str;
  }
  ```
- **OpenAI-compatible** provider covers: OpenRouter, Ollama, OpenAI, DeepSeek, Groq, and any OpenAI-format API.
- **Anthropic** provider for native Claude API (different message format, tool_use blocks).
- Provider is selected based on model prefix in config: `anthropic/claude-*` → Anthropic provider, everything else → OpenAI-compatible.
- Ollama runs at `http://localhost:11434/v1/` with OpenAI-compatible API.

### Storage
- **SQLite via sqlx** for all persistent data: memory, sessions, cron jobs, skill state.
- Single database file at `~/.homunbot/homunbot.db`.
- Use sqlx migrations embedded in binary (`sqlx::migrate!`).
- **TOML** for configuration at `~/.homunbot/config.toml`.
- Never use serde_json for config files; TOML is the standard.

### Tool System
- Each tool implements a `Tool` trait:
  ```rust
  #[async_trait]
  pub trait Tool: Send + Sync {
      fn name(&self) -> &str;
      fn description(&self) -> &str;
      fn parameters(&self) -> serde_json::Value; // JSON Schema
      async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> Result<ToolResult>;
  }
  ```
- Tools are registered in a `ToolRegistry` at startup.
- The agent loop converts tool definitions to the LLM's expected format (OpenAI function calling or Anthropic tool_use).

### Agent Skills Integration
- Compatible with the open Agent Skills specification (https://github.com/agentskills/agentskills).
- Skills are directories containing a `SKILL.md` with YAML frontmatter.
- **Progressive disclosure**: only name + description loaded at startup; full SKILL.md body loaded when the LLM decides to activate a skill.
- Skills are scanned from:
  1. `~/.homunbot/skills/` (user-installed)
  2. `./skills/` (project-local)
  3. Bundled skills in binary
- `homunbot skills add owner/repo` fetches from GitHub and installs to `~/.homunbot/skills/`.
- Skill scripts in `scripts/` are executed via `tokio::process::Command`.

### Channel System
- All channels implement a `Channel` trait:
  ```rust
  #[async_trait]
  pub trait Channel: Send + Sync {
      async fn start(&mut self, tx: mpsc::Sender<InboundMessage>) -> Result<()>;
      async fn send(&self, msg: OutboundMessage) -> Result<()>;
      fn name(&self) -> &str;
  }
  ```
- Messages flow: Channel → InboundMessage → MessageBus → AgentLoop → OutboundMessage → Channel.
- **CLI**: interactive REPL or one-shot (`homunbot chat -m "..."`).
- **Telegram**: via `teloxide` crate (long polling, no webhook needed).
- **WhatsApp**: via Node.js bridge process communicating over local HTTP/WebSocket.

### Memory System
- Short-term: conversation messages in current session (in-memory + SQLite).
- Long-term: consolidated summaries created by the LLM itself.
- Memory consolidation runs when session exceeds threshold (e.g., 20 messages).
- Stored in SQLite `memories` table with timestamps and relevance metadata.

### Config
- Config file: `~/.homunbot/config.toml`
- Example:
  ```toml
  [agent]
  model = "anthropic/claude-sonnet-4-20250514"
  max_iterations = 20

  [providers.openrouter]
  api_key = "sk-or-v1-xxx"

  [providers.anthropic]
  api_key = "sk-ant-xxx"

  [providers.ollama]
  base_url = "http://localhost:11434/v1"

  [channels.telegram]
  enabled = true
  token = "123456:ABC..."
  allow_from = ["123456789"]

  [channels.whatsapp]
  enabled = false

  [tools.web_search]
  provider = "brave"
  api_key = "BSA-xxx"

  [storage]
  path = "~/.homunbot/homunbot.db"
  ```

## Rust Conventions

### General
- Edition 2021, MSRV 1.75+ (for async trait stabilization).
- Use `anyhow::Result` for application errors, `thiserror` for library-level typed errors.
- Prefer `tracing` over `log` for structured logging.
- Use `clap` (derive API) for CLI argument parsing.
- Use `serde` with `serde_derive` for all serialization.
- Format with `rustfmt`, lint with `clippy` (deny warnings in CI).

### Code Style
- Modules: one file per concern, keep files under 300 lines when possible.
- Use `impl` blocks, not free functions, for anything with state.
- Prefer `&str` over `String` in function parameters where possible.
- Use `Arc<T>` for shared state across tasks, `Arc<RwLock<T>>` when mutation is needed.
- Document public APIs with `///` doc comments.
- Tests in the same file (`#[cfg(test)] mod tests`) for unit tests.
- Integration tests in `tests/` directory.

### Error Handling
- Never use `.unwrap()` in production code (only in tests).
- Use `?` operator consistently.
- Wrap external errors with context: `anyhow::Context` trait.
  ```rust
  let config = fs::read_to_string(&path)
      .with_context(|| format!("Failed to read config from {}", path.display()))?;
  ```

### Dependencies (Cargo.toml)
Key crates to use:
- `tokio` (full features) — async runtime
- `reqwest` — HTTP client
- `sqlx` (sqlite, runtime-tokio) — database
- `serde`, `serde_json`, `toml` — serialization
- `clap` (derive) — CLI
- `tracing`, `tracing-subscriber` — logging
- `anyhow`, `thiserror` — errors
- `teloxide` — Telegram bot
- `async-trait` — async trait support
- `tokio-cron-scheduler` — cron jobs
- `notify` — file watcher (for skill hot-reload)
- `gray_matter` — YAML frontmatter parsing from SKILL.md

Do NOT add unnecessary dependencies. Keep the binary lean.

## CLI Commands

```
homunbot                        # Interactive chat (default)
homunbot chat                   # Interactive chat
homunbot chat -m "message"      # One-shot message
homunbot gateway                # Start gateway (all channels + cron + heartbeat)
homunbot config                 # Initialize config
homunbot status                 # Show status
homunbot skills list            # List installed skills
homunbot skills add owner/repo  # Install skill from GitHub
homunbot skills remove name     # Remove skill
homunbot cron list              # List scheduled jobs
homunbot cron add ...           # Add cron job
homunbot cron remove <id>       # Remove cron job
```

## Development Workflow

1. Run `cargo check` frequently — catch errors early.
2. Run `cargo clippy` before committing.
3. Run `cargo test` for all tests.
4. Use `RUST_LOG=debug cargo run` for verbose logging during development.
5. Database migrations are in `migrations/` and auto-applied on startup.

## What NOT to do

- Do NOT use `println!` for logging — use `tracing::info!`, `tracing::debug!`, etc.
- Do NOT block the async runtime — no `std::thread::sleep`, no sync I/O in async context.
- Do NOT hardcode API URLs — they come from config/provider.
- Do NOT store secrets in code — they go in `config.toml`.
- Do NOT add Python/Node.js dependencies to the core — the Rust binary must be self-contained (except for the WhatsApp bridge which is a separate process).
- Do NOT use `clone()` excessively — prefer references and borrows.
- Do NOT panic in library code — return `Result` instead.

## Current Phase

We are in **Phase 1: Core Foundation**. Focus on:
1. Project scaffolding (Cargo.toml, module structure)
2. Config loading (TOML)
3. Provider trait + OpenAI-compatible implementation
4. Basic agent loop (single iteration: user → LLM → response)
5. CLI channel (interactive + one-shot)
6. Shell tool (basic command execution)
7. SQLite setup with migrations

Do NOT build channels (Telegram/WhatsApp), skills system, or cron scheduler yet.
Build the core agent loop first, get it working end-to-end with CLI, then expand.

## Before Writing Code

**Always study the nanobot reference first.** Before implementing any module:

1. Clone the reference: `git clone https://github.com/HKUDS/nanobot.git /tmp/nanobot-reference`
2. Read the corresponding Python file (see mapping table in `PROJECT.md`)
3. Understand the logic, then rewrite it in idiomatic Rust with improvements

For the skills system specifically:
1. Read the Agent Skills specification: https://github.com/agentskills/agentskills
2. Browse real skills at https://skills.sh/ to understand the format
3. The SKILL.md format uses YAML frontmatter — parse with `gray_matter` crate

This is a **Rust rewrite**, not a blind port. Replicate the logic, improve the implementation.
