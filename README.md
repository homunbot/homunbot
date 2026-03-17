# Homun

**Your personal AI agent that lives on your machine and works 24/7.**

Homun is a single-binary, local-first AI assistant written in Rust. Manage it from Telegram, WhatsApp, Discord, Slack, Email, a Web dashboard, or the CLI. It learns from you, runs automations while you sleep, browses the web, and extends via the open Agent Skills ecosystem.

## Features

- **Multi-channel** -- talk to Homun from Telegram, WhatsApp, Discord, Slack, Email, Web UI, or CLI
- **14 LLM providers** -- Anthropic, OpenAI, Ollama (local), OpenRouter, DeepSeek, Groq, Gemini, and more
- **Long-term memory** -- remembers you across sessions, with searchable vector + full-text hybrid retrieval
- **Knowledge base** -- ingest PDFs, docs, code, spreadsheets (30+ formats) and search them via RAG
- **20+ built-in tools** -- shell, files, web search, browser automation, vault, email, scheduling, workflows
- **Automations** -- visual flow builder with triggers, retries, approval gates, and NLP generation
- **Browser automation** -- Playwright-powered headless browser with stealth and vision
- **Skills** -- extend Homun with community skills from GitHub, ClawHub, or create your own
- **MCP servers** -- connect external services (Google Workspace, GitHub, etc.) via Model Context Protocol
- **Security** -- encrypted vault, 2FA, sandboxed execution, exfiltration guard, emergency kill switch
- **Web dashboard** -- 20 pages: chat, dashboard, automations, workflows, skills, memory, knowledge, vault, and more

## Quick Start

### Docker (recommended)

```bash
git clone https://github.com/homunbot/homun.git
cd homun
cp .env.example .env
docker compose up -d
```

Open **https://localhost** and complete the setup wizard.

Want free local embeddings? Add Ollama:

```bash
docker compose --profile with-ollama up -d
```

### From source

Requires Rust 1.75+ and optionally Node.js (for browser automation / MCP servers).

```bash
cargo install --path . --features full
homun config        # Initialize configuration
homun gateway       # Start all services + web UI
```

Open **https://localhost** or use the CLI:

```bash
homun chat                            # Interactive chat
homun chat -m "What's on my calendar" # One-shot message
```

### Pre-built binaries

Download from [GitHub Releases](https://github.com/homunbot/homun/releases) for macOS (x64/ARM), Linux (x64/ARM), and Windows. SHA256 checksums included for verification.

## Configuration

Homun stores all data in `~/.homun/`. Configuration lives in `~/.homun/config.toml`.

The fastest way to configure is through the **web setup wizard** (launches on first boot). You can also edit the config directly:

```toml
[agent]
model = "anthropic/claude-sonnet-4-20250514"
fallback_models = ["openai/gpt-4o-mini", "ollama/qwen3:latest"]

[providers.anthropic]
api_key = "sk-ant-..."

[channels.telegram]
enabled = true
token = "123456:ABC..."
```

At minimum, you need **one LLM provider API key** (or Ollama running locally).

## Build Profiles

| Profile | Command | What you get |
|---------|---------|-------------|
| Default | `cargo install --path .` | CLI + Web UI + core tools + vault + MCP + browser |
| Gateway | `--features gateway` | + multi-channel + local embeddings/RAG + email |
| Full | `--features full` | + browser automation + vault 2FA |

## CLI Commands

```
homun chat           Interactive chat (or one-shot with -m)
homun gateway        Start gateway (channels + web UI + cron + heartbeat)
homun config         Initialize or edit configuration
homun status         Show system status
homun skills         List, add, or remove skills
homun cron           Manage scheduled jobs
homun service        Install as OS service (launchd / systemd)
```

## Documentation

- [Getting Started Guide](docs/GETTING-STARTED.md) -- step-by-step from install to first automation
- [Changelog](CHANGELOG.md) -- all notable changes
- [Roadmap](docs/ROADMAP.md) -- milestone plan and current status
- [Architecture](docs/services/README.md) -- subsystem documentation for contributors

## Requirements

| Dependency | Required | For |
|-----------|----------|-----|
| Rust 1.75+ | Build from source | Compilation |
| Docker | Docker install | Containerized deployment |
| Node.js / npx | Optional | Browser automation, MCP servers |
| Ollama | Optional | Free local LLMs and embeddings |

## License

[PolyForm Noncommercial License 1.0.0](LICENSE) -- free for personal and noncommercial use.
