# Homun

Homun is a personal AI agent in Rust with a local-first architecture, a web control plane, long-term memory, automations, MCP integration, browser automation, skills, and multi-channel delivery.

It is no longer just a small CLI bot. The current codebase includes:

- Interactive chat in CLI and Web UI
- A full web dashboard with auth, setup wizard, logs, approvals, vault, memory, knowledge, MCP, workflows, automations, browser settings, and business screens
- Long-term memory plus a personal RAG knowledge base
- MCP server management and guided setup
- Skill loading, creation, adaptation, and security scanning
- Workflow orchestration and scheduled automations
- Browser automation through `@playwright/mcp`
- Multiple provider backends with failover and health tracking

## Current Status

As of March 10, 2026, the codebase is substantially implemented. The core desktop/web experience, automations, RAG, workflows, MCP, browser tool, web auth/security, and skill runtime are present in code and covered by a large test suite.

What is still incomplete or explicitly marked partial in the roadmap:

- Native hardened sandbox backends for Linux and Windows, beyond the current first-pass `linux_native` Bubblewrap path
- Sandbox runtime image lifecycle/versioning beyond the current explicit-policy model and last-pull drift tracking
- Full CI-backed E2E coverage for web chat and browser flows, although manual smoke coverage now exists
- Phase-2 channel hardening/polish for Discord, Slack, Email, and WhatsApp
- Business modules beyond the current core engine
- Mobile app

`cargo test -q` currently reports 506 tests.

## Build Profiles

Homun is feature-gated. The build you install matters.

| Build | Command | Includes |
|------|---------|----------|
| Default | `cargo install --path .` | CLI, Web UI, shell/file/web tools, vault, MCP, browser |
| Gateway | `cargo install --path . --features gateway` | Multi-channel gateway, local embeddings/RAG, email, MCP |
| Full | `cargo install --path . --features full` | Gateway + browser + vault 2FA |

If you want the project in its current "full product" shape, use `--features full`.

## Prerequisites

The Rust binary is the core runtime, but some capabilities depend on external tools:

- Node.js / `npx` for browser automation and many MCP servers
- A browser installed locally for Playwright MCP
- Docker if you want stricter sandbox isolation
- Ollama if you want fully local models

## Quick Start

Install the full build:

```bash
cargo install --path . --features full
```

Initialize config and open the local dashboard/TUI:

```bash
homun config
```

Run the gateway:

```bash
homun gateway
```

The web UI is available at `https://localhost` by default. For production, set a custom domain in config or via `HOMUN_DOMAIN` env var with the Docker stack (Caddy handles HTTPS automatically via Let's Encrypt).

On first boot, the UI redirects to the setup wizard/login flow.

Manual Playwright MCP smoke checks are available under `scripts/e2e_*.sh`, including a deterministic browser-tool flow via chat, with a manual GitHub Actions entrypoint in `.github/workflows/e2e-smoke.yml`.

Implementation-oriented subsystem documentation now lives in `docs/services/`. Start with `docs/services/README.md` for the service map and use those documents as the source of truth for how each major subsystem works today in code.

You can also use the CLI directly:

```bash
homun chat
homun chat -m "Summarize today's priorities"
homun status
```

## What Exists In Code Today

### Interfaces

- CLI chat and management commands
- Web UI with authenticated pages for chat, dashboard, setup, channels, browser, automations, workflows, business, skills, MCP, memory, knowledge, vault, permissions, approvals, account, logs, login, and setup wizard
- WebSocket streaming chat in the browser

### Agent Core

- Provider failover with retries and "last good" tracking
- Session compaction and token accounting
- Memory search injected into the agent loop
- Tool routing rules for web search vs browser use
- Stop/cancel propagation for web chat runs

### Knowledge And Memory

- Personal memory files plus searchable memory API/UI
- RAG knowledge base with ingestion, chunking, embeddings, hybrid search, directory indexing, watcher support, and sensitive-content gating
- File ingestion for advanced document types when built with local embeddings

### Automation And Orchestration

- Cron jobs
- Rich automations with history, "run now", triggers, and Web UI
- Workflow engine with steps, approvals, retries, resume, and web management
- Subagent spawning

### Skills And MCP

- Skill loading from disk
- Skill creation from prompts
- Skill adaptation from legacy formats
- Security scanning before install
- MCP server catalog, install guidance, OAuth setup flows, and runtime management

### Tools

Depending on build flags and runtime configuration, Homun exposes tools such as:

- `shell`
- `read_file`, `write_file`, `edit_file`, `list_dir`
- `web_search`, `web_fetch`
- `vault`
- `create_automation`
- `create_skill`
- `remember`
- `knowledge`
- `browser`
- `mcp` server tools
- `cron`
- `send_message`
- `spawn_subagent`
- `workflow`
- `business`
- `read_email_inbox`

### Security And Ops

- Web authentication and setup wizard
- API tokens with scopes
- Native HTTPS support
- API rate limiting
- Approval gates and audit trail
- E-stop / kill switch
- Provider health monitoring
- Execution sandbox configuration and UI
- Service install helpers

## Provider Support

The provider layer has three main implementations:

- Anthropic native
- Ollama native
- OpenAI-compatible providers

On top of that, the config and UI support a broad provider catalog, including:

- OpenAI
- OpenRouter
- Gemini
- DeepSeek
- Groq
- Mistral
- xAI
- Together
- Fireworks
- Perplexity
- Cohere
- Venice
- vLLM / custom OpenAI-compatible endpoints
- Vercel
- Cloudflare
- Copilot
- Bedrock
- MiniMax
- DashScope
- Moonshot
- Zhipu

## Channel Support

Channel support is real in the codebase, but not all channels are equally mature. External channels require the richer gateway/full builds.

| Channel | State |
|--------|-------|
| CLI | Solid |
| Web UI | Solid |
| Telegram | Implemented |
| Discord | Implemented, roadmap still tracks further completion/hardening |
| Slack | Implemented, roadmap still tracks further completion/hardening |
| WhatsApp | Implemented, roadmap still tracks stabilization |
| Email | Implemented, roadmap still tracks completion/hardening |

## Documentation

- `README.md`: product-level overview and current status
- `docs/ROADMAP.md`: milestone and delivery plan
- `docs/IMPLEMENTATION-GAPS.md`: real implementation backlog derived from code + roadmap
- `docs/SANDBOX-EXECUTION-PLAN.md`: technical breakdown for the remaining sandbox backlog
- `docs/SANDBOX-RUNTIME-BASELINE.md`: canonical core Docker runtime baseline for sandboxed skills and MCP
- `docs/TESTING-GUIDE.md`: manual and automated verification paths
- `docs/services/README.md`: implementation-oriented subsystem map
- `docs/services/*.md`: service-by-service runtime documentation tied to the current codebase

## CLI Overview

Top-level commands currently exposed by the richer builds include:

```text
homun chat
homun gateway
homun config
homun provider
homun status
homun skills
homun cron
homun automations
homun mcp
homun memory
homun knowledge
homun users
homun service
homun stop
homun restart
```

## Example Configuration

Homun stores state in `~/.homun/`. Configuration lives in `~/.homun/config.toml`.

```toml
[agent]
model = "anthropic/claude-sonnet-4-20250514"
fallback_models = ["openai/gpt-4o-mini", "ollama/qwen3:latest"]
max_iterations = 20

[providers.anthropic]
api_key = "sk-ant-xxx"

[providers.openai]
api_key = "sk-proj-xxx"

[providers.ollama]
api_base = "http://localhost:11434/v1"

[browser]
enabled = true
headless = true

[tools.web_search]
provider = "brave"
api_key = "BSA-xxx"

[channels.telegram]
enabled = true
token = "123456:ABC..."
allow_from = ["123456789"]
pairing_required = true
mention_required = true
```

## What The README Used To Miss

The previous README understated the current project. Compared with the actual codebase, it was missing:

- The authenticated Web UI and setup wizard
- Automations, workflows, approvals, permissions, account management, logs, and business screens
- RAG knowledge base and related CLI/API/UI
- MCP guided setup and OAuth flows
- Skill creator, skill adapter, and skill shield
- Browser automation through Playwright MCP
- Broader provider catalog and provider failover

It also overstated one thing: full Homun usage is not "zero dependency" anymore if you want browser automation, MCP ecosystems, or hardened sandboxing.

## Roadmap Snapshot

The roadmap currently treats these areas as complete in code:

- Sprints 1 through 6
- Sprint 8 hardening
- Browser automation core
- Workflow engine
- Skill runtime parity
- Web security

The main remaining roadmap items are:

- Sandbox hardening milestones
- Chat/browser E2E test suites
- Channel phase 2 completion
- Business modules after the core engine
- Mobile app

For the detailed operational status, see [docs/ROADMAP.md](docs/ROADMAP.md).

## License

This project is licensed under the [PolyForm Noncommercial License 1.0.0](LICENSE).

You may use, study, and modify the software for any **noncommercial** purpose. Commercial use is not permitted. See the [LICENSE](LICENSE) file for the full terms.
