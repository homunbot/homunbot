# Homun — Claude Code Instructions

> **Reference docs**: `docs/PROJECT.md` (vision), `docs/ROADMAP.md` (sprint status), `docs/services/` (per-domain architecture)
> This file contains the technical guidelines for writing code in this codebase.

## What is Homun

Homun is a personal AI assistant written in Rust — a digital homunculus that lives on your machine and works 24/7. Managed via Telegram, WhatsApp, Discord, Slack, Email, Web UI, or CLI. Supports the open **Agent Skills** standard for extensible capabilities.

**Core philosophy**: single binary, local-first, privacy-focused, skill-powered.

**Scale**: ~78K LOC Rust, ~17K LOC JS, 130+ source files, 18 SQLite migrations, 522+ tests, 11-check CI pipeline.

## Architecture Overview

```
src/
├── main.rs                          # Entry point, CLI (clap)
├── logs.rs                          # Structured logging + SSE streaming
├── mcp_setup.rs                     # MCP server auto-setup
│
├── agent/                           # Core agent loop
│   ├── agent_loop.rs                # ReAct loop (reason → act → observe)
│   ├── context.rs                   # System prompt assembly
│   ├── prompt/                      # Prompt builder (sections.rs: 15+ prompt sections)
│   ├── gateway.rs                   # Message routing + channel orchestration
│   ├── memory.rs                    # Long-term memory consolidation
│   ├── memory_search.rs             # Hybrid vector + FTS5 search (RRF scoring)
│   ├── embeddings.rs                # Embedding providers (OpenAI + fastembed)
│   ├── subagent.rs                  # Background task spawning
│   ├── heartbeat.rs                 # Proactive wake-up scheduler
│   ├── bootstrap_watcher.rs         # Hot-reload USER.md/SOUL.md
│   ├── browser_task_plan.rs         # Browser automation orchestration
│   ├── execution_plan.rs            # Structured execution plans
│   ├── verifier.rs                  # Approval verification
│   ├── attachment_router.rs         # Media attachment routing
│   ├── email_approval.rs            # Email approval flow
│   └── stop.rs                      # Graceful shutdown
│
├── provider/                        # LLM providers (14 models supported)
│   ├── traits.rs                    # Provider trait (chat, chat_stream)
│   ├── anthropic.rs                 # Native Claude API (tool_use, streaming)
│   ├── openai_compat.rs             # OpenAI format (OpenRouter, DeepSeek, Groq, etc.)
│   ├── ollama.rs                    # Ollama-specific (localhost:11434)
│   ├── factory.rs                   # Model → provider routing
│   ├── capabilities.rs              # Model capability detection (vision, tool_use, thinking)
│   ├── health.rs                    # Circuit breaker + health monitoring
│   ├── reliable.rs                  # Failover + retry logic
│   ├── one_shot.rs                  # Unified LLM engine for non-conversational calls
│   └── xml_dispatcher.rs            # XML fallback for models without function calling
│
├── tools/                           # 20+ built-in tools
│   ├── registry.rs                  # Tool registry + dispatch
│   ├── shell.rs                     # Command execution (+ sandbox)
│   ├── file.rs                      # Read/write/edit/list files
│   ├── web.rs                       # Web search (Brave, Tavily) + fetch
│   ├── message.rs                   # Send message to user
│   ├── spawn.rs                     # Spawn background subagent
│   ├── cron.rs                      # Schedule recurring tasks
│   ├── vault.rs                     # Encrypted secret storage
│   ├── remember.rs                  # Update USER.md memory
│   ├── knowledge.rs                 # RAG search/ingest/list
│   ├── approval.rs                  # Request user approval for actions
│   ├── automation.rs                # Create/manage automations
│   ├── workflow.rs                  # Multi-step workflow orchestration
│   ├── browser.rs                   # Browser automation (17 actions via MCP Playwright)
│   ├── business.rs                  # Business OODA automation (13 actions)
│   ├── mcp.rs                       # MCP server management
│   ├── email_inbox.rs               # Read email (IMAP)
│   ├── skill_create.rs              # LLM-driven skill generation
│   └── sandbox/                     # Unified sandbox (7 files, 4 backends)
│       ├── mod.rs                   # SandboxManager + backend auto-detection
│       ├── types.rs                 # SandboxConfig, SandboxResult
│       ├── resolve.rs               # Backend resolution logic
│       ├── env.rs                   # Environment injection
│       ├── events.rs                # Execution event logging
│       ├── runtime_image.rs         # Docker runtime image management
│       └── backends/               # Docker, native/macOS, Linux Bubblewrap, Windows Job Objects
│
├── skills/                          # Agent Skills ecosystem
│   ├── loader.rs                    # Scan dirs, parse SKILL.md YAML frontmatter
│   ├── installer.rs                 # GitHub install (homun skills add owner/repo)
│   ├── executor.rs                  # Run scripts (Python/Bash/JS) with env injection
│   ├── creator.rs                   # LLM-driven skill generation
│   ├── security.rs                  # Pre-install security scanning
│   ├── adapter.rs                   # Format conversion (ClawHub SKILL.toml → SKILL.md)
│   ├── watcher.rs                   # Directory hot-reload
│   ├── search.rs                    # Skill discovery + search
│   ├── mcp_registry.rs             # MCP server registry + OAuth setup
│   ├── clawhub.rs                   # ClawHub marketplace integration
│   └── openskills.rs                # Open Skills registry integration
│
├── channels/                        # 7 messaging channels
│   ├── traits.rs                    # Channel trait (start, send, name)
│   ├── cli.rs                       # Interactive REPL + one-shot
│   ├── telegram.rs                  # Teloxide (long polling)
│   ├── whatsapp.rs                  # wa-rs native (no Node.js)
│   ├── discord.rs                   # Serenity
│   ├── slack.rs                     # Socket Mode
│   ├── email.rs                     # IMAP + SMTP
│   └── web (in web/ws.rs)          # WebSocket in Web UI
│
├── rag/                             # RAG Knowledge Base
│   ├── engine.rs                    # HNSW vector + FTS5 hybrid search
│   ├── chunker.rs                   # 30+ format support (md, pdf, docx, code...)
│   ├── parsers.rs                   # PDF/DOCX/XLSX parsing
│   ├── sensitive.rs                 # Sensitive data classification + vault-gating
│   ├── watcher.rs                   # Directory auto-ingestion
│   └── cloud.rs                     # MCP cloud source integration
│
├── workflows/                       # Persistent workflow engine
│   ├── engine.rs                    # Orchestration, retry, approval gates, resume-on-boot
│   └── db.rs                        # Workflow DB operations
│
├── business/                        # Business Autopilot
│   ├── engine.rs                    # OODA loop, budget enforcement, autonomy levels
│   ├── db.rs                        # Business DB operations
│   └── mod.rs                       # Domain types
│
├── security/                        # Security infrastructure
│   ├── mod.rs                       # Exfiltration guard
│   ├── estop.rs                     # Emergency kill switch
│   ├── pairing.rs                   # DM pairing + OTP verification
│   ├── totp.rs                      # TOTP 2FA
│   ├── two_factor.rs                # 2FA management
│   └── vault_leak.rs                # Vault leak detection
│
├── browser/                         # Browser automation
│   ├── mcp_bridge.rs               # Persistent MCP Playwright peer
│   ├── helpers.rs                   # Compact snapshot utilities
│   └── mod.rs                       # Browser manager
│
├── web/                             # Web UI (Axum, 20 pages)
│   ├── server.rs                    # Axum + TLS + session + rust-embed
│   ├── auth.rs                      # PBKDF2 auth, rate limiting, API keys
│   ├── api/                         # 50+ REST endpoints (v1/)
│   │   ├── mod.rs                   # Router + re-exports (81 lines)
│   │   ├── mcp/                     # MCP catalog, OAuth, install, CRUD (6 files)
│   │   └── {domain}.rs             # 21 domain files (account, chat, skills, etc.)
│   ├── pages.rs                     # HTML template generation
│   ├── ws.rs                        # WebSocket chat channel
│   ├── chat_attachments.rs          # File upload handling
│   └── run_state.rs                 # Run state tracking
│
├── scheduler/                       # Scheduling
│   ├── cron.rs                      # tokio-cron-scheduler
│   └── automations.rs               # Automation trigger engine
│
├── storage/
│   ├── db.rs                        # SQLite (sqlx, 18 migrations)
│   └── secrets.rs                   # AES-256-GCM vault + OS keychain
│
├── config/
│   ├── schema.rs                    # 15+ config sections (TOML)
│   └── dotpath.rs                   # Dot-path get/set
│
├── bus/queue.rs                     # Message bus (mpsc)
├── session/manager.rs               # Session state
├── queue/                           # Batch processing
├── service/                         # OS service install (launchd, systemd)
├── tui/                             # Terminal UI (ratatui)
├── user/                            # User management
└── utils/
    ├── retry.rs                     # Exponential backoff + network state
    └── reasoning_filter.rs          # Strip thinking blocks
```

### Frontend
```
static/
├── css/style.css                    # Design System "Olive Moss Console"
└── js/                              # 28 files, ~19K LOC
    ├── chat.js                      # Chat with streaming, markdown, tool timeline
    ├── automations.js               # Visual flow builder (n8n-style SVG canvas)
    ├── auto-validate.js             # Builder real-time validation engine
    ├── flow-renderer.js             # Flow rendering engine
    ├── model-loader.js              # Shared LLM model fetcher (DRY utility)
    ├── mcp-loader.js                # Shared MCP server/tool discovery (DRY utility)
    ├── schema-form.js               # JSON Schema → form fields for tool params
    ├── workflows.js                 # Workflow builder + approval UI
    ├── business.js                  # Business dashboard
    ├── skills.js                    # Skill marketplace + install
    ├── knowledge.js                 # RAG document upload + search
    ├── memory.js                    # Memory editor + search
    ├── vault.js                     # Secret management + 2FA setup
    ├── mcp.js                       # MCP server discovery + OAuth
    ├── approvals.js                 # Approval queue
    ├── dashboard.js                 # Operational dashboard
    ├── dash-usage.js                # Dashboard usage analytics + charts
    ├── logs.js                      # Log streaming + filtering
    ├── setup.js                     # Config wizard
    ├── account.js                   # User settings + API tokens
    ├── sidebar.js                   # Navigation + session list
    ├── appearance.js                # Theme + accent picker
    ├── theme.js                     # Light/dark mode
    ├── sandbox.js                   # Sandbox settings UI
    ├── shell.js                     # Terminal interface
    └── file-access.js               # File access UI
```

## Key Design Decisions

### Runtime & Async
- **Tokio** as async runtime — `#[tokio::main]`, `tokio::spawn` for concurrency.
- All I/O must be async. Never `std::thread::sleep`, use `tokio::time::sleep`.
- `tokio::sync::mpsc` for internal message bus.

### LLM Provider System
- `Provider` trait in `provider/traits.rs` — `chat()`, `chat_stream()`, `name()`.
- **Model routing**: `anthropic/claude-*` → Anthropic provider, `ollama/*` → Ollama, everything else → OpenAI-compatible.
- **ReliableProvider** wraps any provider with circuit breaker + failover.
- **`one_shot.rs`**: shared `llm_one_shot()` for non-conversational calls (automations generation, MCP setup, skill creation). Disables extended thinking, 30s timeout.
- **Capabilities detection**: auto-detect vision, tool_use, extended_thinking per model.
- **XML fallback**: `xml_dispatcher.rs` for models without native function calling.

### Storage
- **SQLite via sqlx** — 18 migrations, single file `~/.homun/homun.db`.
- **TOML** config at `~/.homun/config.toml`.
- Never serde_json for config files.

### Tool System
- `Tool` trait: `name()`, `description()`, `parameters()` (JSON Schema), `execute()`.
- `ToolRegistry` at startup, tools converted to LLM format (OpenAI/Anthropic).
- **OnceCell** late-binding pattern for tools needing gateway state at runtime.

### Channel System
- `Channel` trait: `start()`, `send()`, `name()`.
- Flow: Channel → InboundMessage → MessageBus → AgentLoop → OutboundMessage → Channel.
- 7 channels: CLI, Telegram, WhatsApp, Discord, Slack, Email, Web (WebSocket).

### Memory System
- **Short-term**: session messages (in-memory + SQLite).
- **Long-term**: LLM-consolidated summaries in `memories` table.
- **Hybrid search**: vector (HNSW) + FTS5, RRF scoring in `memory_search.rs`.
- **Daily files**: `~/.homun/memory/YYYY-MM-DD.md`.
- **User profile**: `~/.homun/brain/USER.md` (written only by `remember` tool).

### Skills System
- Open Agent Skills spec compatible (SKILL.md with YAML frontmatter).
- **Progressive disclosure**: name + description at startup, full body on activation.
- Scanned from: `~/.homun/skills/` (user), `./skills/` (project), bundled (5 default).
- **Security shield**: pre-install scanning before execution.
- **Runtime parity** with OpenClaw: eligibility, invocation policy, tool restriction, env injection.

### RAG Knowledge Base
- Multi-format ingestion (30+ formats: md, pdf, docx, xlsx, code, etc.).
- Hybrid search: HNSW vectors + FTS5, with sensitive data vault-gating.
- Directory watcher for auto-ingest.

### Sandbox
- 4 backends: Docker, native/macOS, Linux Bubblewrap, Windows Job Objects.
- Auto-detection of best available backend.
- Event logging + runtime image management.

### Security
- **Auth**: PBKDF2 (600k iterations), HMAC-signed session cookies.
- **Rate limiting**: auth 5/min, API 60/min, per-IP.
- **E-Stop**: emergency kill switch (agent loop, browser, MCP).
- **Exfiltration guard**: detects + redacts sensitive data leaks.
- **Vault**: AES-256-GCM, OS keychain master key, zeroized memory.
- **2FA**: TOTP support.

### Web UI
- **Axum** server with TLS + rust-embed for static assets.
- **20 pages**: dashboard, chat, login, setup-wizard, channels, browser, automations, workflows, business, skills, mcp, memory, knowledge, vault, permissions, approvals, account, logs, maintenance, setup.
- **50+ REST API endpoints** under `/api/v1/`.
- Debug mode: CSS/JS served from filesystem (hot reload), HTML templates require recompile.

### Browser Automation
- MCP Playwright (`@playwright/mcp` via npx), persistent peer.
- Stealth anti-bot injection, compact snapshots (tree-preserving).
- Auto-snapshot after navigate/click/type.
- 17 actions in unified `browser` tool.

### Workflow Engine
- Persistent multi-step workflows with approval gates.
- Retry logic + resume-on-boot from SQLite.

### Automations Builder v2
- Visual flow canvas (n8n-style SVG), 11 node kinds.
- Guided inspector (dropdown from API, no free text).
- NLP flow generation via LLM (`one_shot.rs`).

### Business Autopilot
- OODA loop engine with budget enforcement.
- Autonomy levels, transaction tracking.
- 13 LLM tool actions.

## Rust Conventions

### General
- Edition 2021, MSRV 1.75+.
- `anyhow::Result` for app errors, `thiserror` for typed library errors.
- `tracing` for logging (never `println!`).
- `clap` (derive) for CLI, `serde` for serialization.
- `rustfmt` + `clippy` (deny warnings in CI).

### Code Style
- One file per concern, keep under 300 lines when possible.
- `impl` blocks for anything with state.
- Prefer `&str` over `String` in parameters.
- `Arc<T>` for shared state, `Arc<RwLock<T>>` when mutation needed.
- `///` doc comments on public APIs.
- Unit tests in `#[cfg(test)] mod tests`, integration tests in `tests/`.

### Error Handling
- Never `.unwrap()` in production (only tests).
- `?` operator consistently.
- `anyhow::Context` for wrapping: `.with_context(|| format!("Failed to read {}", path.display()))?`.

### Key Dependencies
- `tokio` (full) — async runtime
- `reqwest` — HTTP client
- `sqlx` (sqlite, runtime-tokio) — database
- `serde`, `serde_json`, `toml` — serialization
- `clap` (derive) — CLI
- `tracing`, `tracing-subscriber` — logging
- `anyhow`, `thiserror` — errors
- `teloxide` — Telegram
- `serenity` — Discord
- `wa-rs` — WhatsApp (GitHub fork: homunbot/wa-rs)
- `tokio-cron-scheduler` — cron
- `notify` — file watcher
- `gray_matter` — YAML frontmatter
- `keyring` — OS keychain (apple-native, linux-native, windows-native)
- `axum` — web server
- `rust-embed` — static asset embedding

Keep dependencies lean. Do NOT add unnecessary crates.

## CLI Commands

```
homun                        # Interactive chat (default)
homun chat                   # Interactive chat
homun chat -m "message"      # One-shot message
homun gateway                # Start gateway (channels + cron + heartbeat + web UI)
homun config                 # Initialize config
homun status                 # Show status
homun skills list            # List installed skills
homun skills add owner/repo  # Install skill from GitHub
homun skills remove name     # Remove skill
homun cron list              # List scheduled jobs
homun cron add ...           # Add cron job
homun cron remove <id>       # Remove cron job
homun service install        # Install as OS service (launchd/systemd)
```

## Development Workflow

1. `cargo check` — catch errors early.
2. `cargo clippy` — lint before committing.
3. `cargo test` — run 522+ tests.
4. `RUST_LOG=debug cargo run -- gateway` — verbose logging.
5. Migrations in `migrations/` are auto-applied on startup.

## Development Conventions

### File Size Discipline
- **Hard limit**: no Rust file over 500 lines. If a file approaches 400 lines, plan a split.
- **Target**: 200-300 lines per file. One concern per file.
- **When splitting**: extract into a submodule directory (e.g., `tools/sandbox/` pattern). Keep the `mod.rs` as thin re-export + orchestration.
- **JS files**: same 500-line limit. Large pages (automations.js, chat.js) are grandfathered but new features go in separate files.
- **Never compact arbitrarily**: do not merge small files "for simplicity". Each file has a reason to exist.

### Research Before Building
Before implementing a new component or feature domain:
1. **Pull competitor repos** to get latest:
   ```
   cd ~/Projects/openclaw && git pull
   cd ~/Projects/zeroclaw && git pull
   ```
2. **Study how they do it**: check the relevant module in both projects.
   - OpenClaw (TypeScript): `~/Projects/openclaw/` — 30+ channels, Lobster workflows, ClawHub marketplace
   - ZeroClaw (Rust): `~/Projects/zeroclaw/` — lean binary, HNSW vectors, AIEOS identity
   - Analysis docs: `docs/competitors/openclaw.md`, `docs/competitors/zeroclaw.md`, `docs/competitors/COMPARISON.md`
3. **Document findings** in the plan before writing code.
4. This applies to: new tools, new channels, new storage patterns, new API designs, new skill features.

### Plan Mode for Large Tasks
- **Mandatory** for any task touching more than 3 files.
- Use Plan Mode (Shift+Tab x2) to analyze and plan before modifying files.
- Workflow: Plan → User reviews → Approved → Execute step by step.
- For very large features: write a SPEC.md first, `/clear`, then new session to execute.

### Context Window Management
- **One feature per session**. Start with `/clear` when switching tasks.
- **Document & Clear pattern** for large tasks: dump progress to a `.md` file, `/clear`, continue in new session reading that file.
- **Avoid `/compact`** — prefer explicit `/clear` with documented state.
- **Read only what's needed**: don't read entire large files when you need one function.

### Feature Development Workflow
1. Create git branch: `git checkout -b feat/feature-name`
2. Plan in 3-5 steps with small diffs
3. Execute step by step — `cargo check` after each edit
4. `cargo test` after each meaningful change
5. PR description generated at the end

### Testing Requirements
- **After every change**: run `cargo test`. Never disable tests — fix them.
- **Every new module** requires at least unit tests for the happy path.
- **Every bug fix** requires a regression test.
- **Integration tests** in `tests/` for cross-module behavior.
- Tests are the only reliable validation for AI-generated code.

### Code Quality Gates
- `cargo check` runs automatically after edits (via Claude Code hook).
- `cargo fmt` + `cargo clippy` run before commits (via Claude Code hook).
- If `cargo check` fails after an edit, fix immediately before continuing.
- Never skip or ignore compiler warnings.

### DRY — Don't Repeat Yourself
- **Before creating anything new**: search the codebase for existing code that does something similar. Extend it, don't duplicate.
- **Common patterns already exist** — reuse them:
  - `provider/one_shot.rs` → any non-conversational LLM call (don't create ad-hoc reqwest calls)
  - `utils/retry.rs` → any network operation that needs retry (don't write custom retry loops)
  - `storage/db.rs` → any SQLite operation (don't open new connections)
  - `web/auth.rs` → any auth/rate-limit check (don't reimplement)
  - `tools/registry.rs` → tool registration (follow the existing pattern exactly)
  - `channels/traits.rs` → channel abstraction (implement the trait, don't invent new flows)
- **Refactor over duplicate**: if two modules share >20 lines of similar logic, extract a shared function or trait.
- **Extend over replace**: add variants to existing enums, methods to existing impls, fields to existing structs. Don't create parallel types.
- **CSS**: reuse design tokens from `static/css/style.css`. Never hardcode colors, spacing, or font sizes. Use CSS variables (`var(--*)`).
- **JS**: check if a similar UI pattern already exists in another page's JS before writing new code.

### Roadmap Tracking
- **After completing a feature or significant change**, update `docs/ROADMAP.md`:
  - Mark relevant tasks as ✅ DONE with date
  - Update LOC counts if significant
  - Add new tasks discovered during implementation
  - Update "Status Attuale" metrics table if numbers changed
- Keep the ROADMAP as the **single source of truth** for project status.
- Format: `| task | files | LOC | ✅ DONE |` in the sprint tables.

### UX Conventions
- **Design system**: follow `docs/design/design-constitution.md` (Braun-inspired, 8px grid, specific scales).
- **Quality gate**: every UI change must pass `docs/design/ui-quality-gate.md` checklist.
- **States are mandatory**: every component must handle empty, loading, error, success states.
- **Mobile-first**: design at 375px, then scale up. Verify at 390, 768, 1024, 1280px.
- **Progressive disclosure**: hide advanced options behind expandable sections.
- **CSS tokens only**: use `var(--accent)`, `var(--surface-*)`, `var(--text-*)` etc. Never hardcode values.
- Use `/ux-review` and `/new-screen` commands for UI work.

## What NOT to Do

- Do NOT use `println!` — use `tracing::info!`, `tracing::debug!`, etc.
- Do NOT block the async runtime — no `std::thread::sleep`, no sync I/O.
- Do NOT hardcode API URLs — they come from config/provider.
- Do NOT store secrets in code — use vault or `config.toml`.
- Do NOT add Python/Node.js deps to the core binary.
- Do NOT use `.clone()` excessively — prefer references and borrows.
- Do NOT panic in library code — return `Result`.

## Project Status

**Feature-complete** — all 8 core sprints + 5 transversal programs done.

### Done
- Sprint 1-8: agent robustness, memory search, channel security, Web UI + automations, ecosystem (skills/MCP), RAG knowledge, channels phase 2, hardening
- Sandbox: Docker/native/Linux/Windows, CI cross-platform
- Chat Web UI: CHAT-1..6 (multi-session, streaming, markdown, tool timeline)
- Design System: Olive Moss Console (tokens, accent picker, semantic colors)
- Skill Runtime Parity: eligibility, invocation policy, tool restriction, env injection
- Security: auth + HTTPS + rate limiting + API keys + E-Stop + exfiltration guard
- Workflow Engine: persistent, approval gates, retry, resume-on-boot
- Business Autopilot BIZ-1: OODA core, budget enforcement

### In Progress / Remaining
- CHAT-7: E2E test formalization (release-grade)
- Browser: stealth hardening, screenshot/vision
- BIZ-2..5: payments, accounting, marketing, crypto
- Mobile App: Flutter, encrypted pairing
- Distribution: pre-built binaries, Docker, Homebrew

See `docs/ROADMAP.md` for detailed sprint status.

## Important Directories

- `~/.homun/` — Data dir (config, db, memory)
- `~/.homun/brain/` — Agent-writable memory (USER.md, INSTRUCTIONS.md, SOUL.md)
- `~/.homun/skills/` — User-installed skills
- `./skills/` — Project-local bundled skills (5)
- `./migrations/` — SQLite migrations (18, auto-applied)
- `./docs/services/` — Per-domain architecture docs (13 files)
- `./static/` — Web UI assets (CSS + 21 JS files)

## File Locations (Runtime)

- `~/.homun/config.toml` — Configuration
- `~/.homun/homun.db` — SQLite database
- `~/.homun/secrets.enc` — Encrypted vault
- `~/.homun/brain/USER.md` — User profile (remember tool writes)
- `~/.homun/brain/INSTRUCTIONS.md` — Learned instructions (consolidation)
- `~/.homun/MEMORY.md` — Long-term memory (consolidation)
- `~/.homun/memory/YYYY-MM-DD.md` — Daily memory files

## Integration Points — Where New Code Plugs In

Quick reference for adding new components without re-reading the whole codebase.

### New Tool
1. Create `src/tools/{name}.rs` — implement `Tool` trait
2. Register in `src/tools/mod.rs` (pub mod) + `src/tools/registry.rs` (register call)
3. Done — the agent loop auto-discovers registered tools

### New Channel
1. Create `src/channels/{name}.rs` — implement `Channel` trait
2. Add to `src/channels/mod.rs` (pub mod)
3. Config struct in `src/config/schema.rs` (under ChannelsConfig)
4. Start logic in `src/agent/gateway.rs` (match on channel name)
5. Web UI card in `src/web/pages.rs` (`build_channels_cards_html`)

### New API Endpoint
1. Add handler fn in the appropriate `src/web/api/{domain}.rs` file (or create a new domain file)
2. Register route in that file's `pub(super) fn routes()`, which is merged in `src/web/api/mod.rs`
3. Auth: use `require_auth()` middleware from `src/web/auth.rs`

### New Web UI Page
1. HTML template in `src/web/pages.rs` (fn + template)
2. Route in `src/web/server.rs`
3. JS file in `static/js/{name}.js`
4. Sidebar link in `src/web/pages.rs` (`build_sidebar_html`)

### New Migration
1. Create `migrations/NNN_{name}.sql`
2. Auto-applied on startup via `sqlx::migrate!`

### New Config Section
1. Add struct to `src/config/schema.rs`
2. Add field to parent config struct
3. Dotpath access via `src/config/dotpath.rs`

### New LLM One-Shot Call
1. Use `provider/one_shot.rs` → `llm_one_shot()` — do NOT create ad-hoc provider calls
2. Pass system prompt + user prompt + optional tools

### New Skill
1. Create dir in `skills/{name}/` with `SKILL.md` (YAML frontmatter)
2. Optional `scripts/` dir for executable scripts
3. Loaded automatically by `src/skills/loader.rs`

## Grandfathered Files (Pre-Convention)

These files exceed the 500-line limit and predate the convention. Do NOT split them unless explicitly asked — they work as-is. New code within them should follow conventions; new features should go in separate files.

**Rust (>1000 lines):**
- `web/pages.rs` (4.3K) — HTML templates; unavoidable size, templates are self-contained
- `agent/agent_loop.rs` (3.2K) — core loop; complex but cohesive
- `main.rs` (2.8K) — CLI entry; clap derive + subcommands
- `storage/db.rs` (2.7K) — all DB operations; cohesive single-concern
- `config/schema.rs` (2.2K) — all config structs; grows with features
- `tui/app.rs` (2K) — TUI state; cohesive
- `agent/gateway.rs` (1.6K) — message routing
- `skills/loader.rs` (1.5K) — skill parsing + validation
- `tools/browser.rs` (1.2K) — 17 browser actions
- `skills/clawhub.rs` (1.1K), `skills/security.rs` (1.1K), `channels/email.rs` (1K), `agent/memory.rs` (1K), `web/server.rs` (1K)

**JS (>500 lines):**
- `chat.js` (2.9K), `automations.js` (2.5K), `setup.js` (2.5K), `mcp.js` (1.7K), `skills.js` (1K)

## Git Commit Guidelines

- Conventional commit format: `type(scope): description`
- Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`
- Italian or English
- Do NOT add Claude as co-author
