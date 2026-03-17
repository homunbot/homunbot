# Changelog

All notable changes to Homun are documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

#### Core Agent
- ReAct agent loop with reasoning, action, observation cycle
- 14 LLM providers: Anthropic, OpenAI, OpenRouter, DeepSeek, Groq, Ollama, Gemini, xAI, Mistral, Together, Fireworks, Cohere, Bedrock, Cloudflare
- ReliableProvider with circuit breaker, failover, and retry logic
- Model capability auto-detection (vision, tool_use, extended_thinking)
- XML fallback dispatcher for models without native function calling
- `one_shot.rs` shared LLM engine for non-conversational calls
- Session compaction and token usage tracking
- Loop detection and token budget enforcement
- Agent stop signaling for graceful shutdown
- Subagent background task spawning
- Heartbeat proactive wake-up scheduler
- Hot-reload for USER.md/SOUL.md

#### Channels (7)
- CLI interactive REPL + one-shot mode
- Telegram via frankenstein (migrated from teloxide)
- WhatsApp native (wa-rs, no Node.js)
- Discord via Serenity
- Slack Socket Mode
- Email (IMAP IDLE + SMTP)
- Web UI WebSocket
- Message debounce for multi-message input
- Channel hardening across all channels (Sprint 7)

#### Tools (20+)
- Shell command execution with sandbox integration
- File read/write/edit/list
- Web search (Brave, Tavily) + fetch
- Send message to user
- Spawn background subagent
- Schedule recurring tasks (cron)
- Encrypted vault (AES-256-GCM, OS keychain)
- Remember tool (USER.md memory)
- RAG knowledge search/ingest/list
- Request user approval
- Create/manage automations
- Multi-step workflow orchestration
- Browser automation (17 actions via MCP Playwright)
- Business OODA automation (13 actions)
- MCP server management
- Email inbox (IMAP)
- LLM-driven skill generation

#### Sandbox (4 backends)
- Docker container isolation
- macOS Seatbelt (sandbox-exec)
- Linux Bubblewrap (bwrap)
- Windows Job Objects
- Auto-detection of best available backend
- Runtime image management

#### Skills System
- Open Agent Skills spec compatible (SKILL.md with YAML frontmatter)
- GitHub install (`homun skills add owner/repo`)
- ClawHub marketplace integration
- Open Skills registry integration
- Security scanning before install
- Directory hot-reload
- LLM-driven skill generation
- Progressive disclosure (name + description at startup, full body on activation)
- Runtime parity: eligibility, invocation policy, tool restriction, env injection

#### Memory
- Short-term session messages (in-memory + SQLite)
- Long-term LLM-consolidated summaries
- Hybrid search: HNSW vectors + FTS5, RRF scoring
- Daily memory files (`~/.homun/memory/YYYY-MM-DD.md`)
- User profile in `~/.homun/brain/USER.md`

#### RAG Knowledge Base
- 30+ format support (md, pdf, docx, xlsx, code, etc.)
- HNSW vector + FTS5 hybrid search
- Sensitive data classification + vault-gating
- Directory watcher for auto-ingestion
- Provider-agnostic embedding factory (OpenAI, Ollama)
- Model mismatch detection + in-place index rebuild

#### Web UI (20 pages)
- Dashboard with token usage charts and cost tracking
- Chat with streaming, markdown rendering, tool timeline
- Multi-session support with sidebar navigation
- Login with PBKDF2 auth
- Setup wizard with guided configuration
- Channels management
- Browser automation viewer
- Automations visual flow builder (n8n-style SVG canvas)
- NLP flow generation via LLM
- Workflows builder + approval UI
- Business dashboard
- Skills marketplace + install
- MCP server discovery + OAuth
- Knowledge base document upload + search
- Memory editor + search
- Vault secret management + 2FA setup
- Permissions and file access management
- Approvals queue
- Maintenance (database purge)
- Log streaming + filtering
- Appearance theme + accent picker (Olive Moss Console design system)

#### Automations Engine
- Visual flow canvas with 11 node kinds
- Guided inspector (dropdown from API, no free text)
- Multi-step execution with retry
- NLP flow generation via `one_shot.rs`
- History panel with execution logs

#### Workflow Engine
- Persistent multi-step workflows
- Approval gates
- Retry logic
- Resume-on-boot from SQLite

#### Business Autopilot
- OODA loop engine with budget enforcement
- Autonomy levels
- Transaction tracking
- 13 LLM tool actions

#### Browser Automation
- MCP Playwright (`@playwright/mcp`) persistent peer
- Stealth anti-bot injection
- Compact snapshots (tree-preserving)
- Auto-snapshot after navigate/click/type
- Action policy (config-driven allow/deny)
- Error recovery and resource blocking
- Screenshot action with vision model analysis
- Per-conversation tab isolation

#### Security
- PBKDF2 auth (600k iterations), HMAC-signed session cookies
- Rate limiting: auth 5/min, API 60/min, per-IP
- HTTPS with TLS support
- API key authentication
- Emergency kill switch (E-Stop): agent loop, browser, MCP
- Exfiltration guard: detects + redacts sensitive data leaks
- Vault: AES-256-GCM, OS keychain master key, zeroized memory
- TOTP 2FA support
- Vault leak prevention
- Command approval workflow

#### MCP Integration
- MCP server registry + OAuth setup
- Connection Recipes for simplified onboarding
- Multi-instance support
- Hot-reload tools after connecting new service
- HTTP/SSE transport support
- OAuth token refresh
- Google Workspace recipe

#### Infrastructure
- Docker deployment stack (Dockerfile, docker-compose)
- Ollama sidecar for free Docker embeddings
- OS service install (launchd, systemd)
- 18 SQLite migrations (auto-applied)
- 11-check CI pipeline
- Feature flags for optional components

#### Release Hardening
- Detailed `/health/components` endpoint (6 subsystem checks)
- Graceful shutdown: SIGTERM + Ctrl+C, 30s grace period, DB pool close
- Unified toast notification system (`hm-toast`, 17 implementations consolidated)
- Flaky test fix: `serial_test` + `EnvGuard` RAII for sandbox tests

### Fixed
- Provider hot-reload and OpenRouter model routing
- Browser auto-cleanup on task completion
- WebSocket TLS upgrade
- Vault 2FA fail-closed logic
- Embedding provider URL fallback
- Ollama model auto-pull on save
- MCP TLS for streamable HTTP client
- Automations layout and history panel overflow
- Various clippy warnings and CI failures

### Changed
- License from MIT to PolyForm Noncommercial 1.0.0
- Migrated Telegram from teloxide to frankenstein
- Removed ONNX/fastembed, switched to provider-agnostic embedding factory
- Split monolithic `api.rs` (12K lines) into `api/` submodule (21 domain files)
- Removed hardcoded domain references, default to localhost

---

*Generated from 188 commits across 130+ source files.*
*Rust: ~78K LOC | JS: ~17K LOC | 522+ tests*
