# 🧪 Homun

**The digital homunculus that lives in your computer.**

Homun is an ultra-lightweight personal AI assistant written in Rust — a single binary, zero-dependency, skill-powered agent you manage remotely via Telegram, WhatsApp, Discord, or CLI.

> *In alchemy, a homunculus is a small artificial being created to serve its maker.
> Homun is yours.*

## Features

- 🦀 **Single Rust binary** — no Python, no Node.js runtime, no Docker required
- 🧠 **Skill-powered** — compatible with the open [Agent Skills](https://github.com/agentskills/agentskills) standard
- 📱 **Multi-channel** — Telegram, WhatsApp, Discord, and CLI
- 🔒 **Local-first** — your data stays on your machine, works with Ollama for fully offline operation
- ⚡ **14 LLM providers** — Anthropic, OpenAI, OpenRouter, Ollama, DeepSeek, Groq, Gemini, and more
- 🔧 **8 built-in tools** — shell, file ops, web search, cron, MCP, message, subagent
- 📅 **Cron scheduling** — schedule recurring tasks with natural language
- 🧩 **MCP support** — connect Model Context Protocol servers
- 💾 **Long-term memory** — LLM-powered memory consolidation across sessions
- 🖥️ **TUI dashboard** — interactive terminal UI for configuration and management

## Quick Start

```bash
# Install from source
cargo install --path .

# Open the interactive dashboard to configure
homun config

# One-shot message
homun chat -m "What's the weather in Rome?"

# Interactive chat
homun chat

# Start the gateway (all channels + cron + heartbeat)
homun gateway
```

## Configuration

Homun stores everything in `~/.homun/`. Run `homun config` to open the TUI dashboard, or edit `~/.homun/config.toml` directly:

```toml
[agent]
model = "anthropic/claude-sonnet-4-20250514"
max_iterations = 20
temperature = 0.7

[providers.anthropic]
api_key = "sk-ant-xxx"

[providers.openrouter]
api_key = "sk-or-v1-xxx"

[providers.ollama]
api_base = "http://localhost:11434/v1"

[channels.telegram]
enabled = true
token = "123456:ABC..."
allow_from = ["123456789"]

[channels.whatsapp]
enabled = true
phone_number = "393331234567"

[channels.discord]
enabled = true
token = "your-discord-bot-token"

[tools.web_search]
provider = "brave"
api_key = "BSA-xxx"
```

### Supported Providers

| Provider | Model prefix | Notes |
|----------|-------------|-------|
| Anthropic | `anthropic/claude-*` | Native API with tool_use |
| OpenAI | `openai/gpt-*` | Via OpenAI API |
| OpenRouter | `openrouter/*` | 200+ models |
| Ollama | `ollama/*` | Local, fully offline |
| DeepSeek | `deepseek/*` | |
| Groq | `groq/*` | Ultra-fast inference |
| Gemini | `gemini/*` | Google AI |
| + 7 more | See docs | Any OpenAI-compatible API |

## Skills

Homun supports the open [Agent Skills](https://github.com/agentskills/agentskills) specification. Skills are directories with a `SKILL.md` that teaches the agent new capabilities.

```bash
# Search for skills
homun skills search "weather"

# Install from GitHub
homun skills add owner/repo

# Install from ClawHub registry
homun skills search "gmail"  # then install from TUI

# List installed
homun skills list

# Remove
homun skills remove skill-name
```

Or use the TUI dashboard (`homun config` → Skills tab) for a visual experience with search, install, and auto-setup.

## Tools

Homun comes with 8 built-in tools:

| Tool | Description |
|------|-------------|
| `shell` | Execute shell commands (with safety filters) |
| `read_file` / `write_file` / `edit_file` | File operations with workspace isolation |
| `list_dir` | Directory listing |
| `web_search` | Search the web (Brave API) |
| `web_fetch` | Fetch and read web pages |
| `cron` | Schedule recurring tasks |
| `send_message` | Send proactive messages to any channel |
| `spawn_subagent` | Run background tasks |

### MCP Servers

Connect external tools via the [Model Context Protocol](https://modelcontextprotocol.io/):

```bash
# Add an MCP server
homun mcp add filesystem --command npx --args "-y @modelcontextprotocol/server-filesystem /tmp"

# List servers
homun mcp list

# Toggle on/off
homun mcp toggle filesystem
```

## Channels

### Telegram
1. Create a bot via [@BotFather](https://t.me/BotFather)
2. Set the token in config: `channels.telegram.token`
3. Add your chat ID to `channels.telegram.allow_from`
4. Run `homun gateway`

### WhatsApp
1. Set your phone number: `channels.whatsapp.phone_number = "393331234567"`
2. Run `homun config` → WhatsApp tab → press `p` to pair
3. Enter the pairing code on your phone
4. Run `homun gateway`

### Discord
1. Create a bot at [Discord Developer Portal](https://discord.com/developers)
2. Set token and channel ID in config
3. Run `homun gateway`

## Cron Jobs

Schedule recurring tasks that run automatically:

```bash
# Add a cron job
homun cron add "0 8 * * *" "Give me a morning briefing"

# List jobs
homun cron list

# Remove
homun cron remove <id>
```

Cron responses are delivered to the channel where the job was created.

## Personalization

Create files in `~/.homun/` to customize your homunculus:

- **`SOUL.md`** — Personality and behavior instructions
- **`USER.md`** — Information about you (name, preferences, context)
- **`AGENTS.md`** — Agent-specific directives and rules

Example `~/.homun/SOUL.md`:
```markdown
You are a witty, efficient assistant. You speak Italian when the user writes in Italian.
You love making dad jokes but only occasionally.
```

## CLI Reference

```
homun                          # Interactive chat (default)
homun chat                     # Interactive chat
homun chat -m "message"        # One-shot message
homun gateway                  # Start all channels + cron
homun config                   # TUI dashboard
homun config show              # Print config
homun config get <key>         # Get a config value
homun config set <key> <val>   # Set a config value
homun status                   # Show agent status
homun skills list              # List installed skills
homun skills add owner/repo    # Install skill from GitHub
homun skills remove <name>     # Remove a skill
homun skills search <query>    # Search for skills
homun cron list                # List cron jobs
homun cron add <expr> <msg>    # Add a cron job
homun cron remove <id>         # Remove a cron job
homun mcp list                 # List MCP servers
homun mcp add <name> ...       # Add MCP server
homun mcp remove <name>        # Remove MCP server
homun mcp toggle <name>        # Enable/disable MCP server
homun provider list            # List providers
homun provider test [name]     # Test a provider
```

## Architecture

```
~/.homun/
├── config.toml          # Configuration
├── homun.db          # SQLite (sessions, messages, memory, cron)
├── workspace/           # Agent's working directory
├── skills/              # Installed skills
├── SOUL.md              # Personality (optional)
├── USER.md              # User context (optional)
├── AGENTS.md            # Agent directives (optional)
└── MEMORY.md            # Long-term memory (auto-generated)
```

Built with:
- **Tokio** — async runtime
- **SQLite** (sqlx) — persistent storage
- **Ratatui** — TUI dashboard
- **Reqwest** — HTTP client
- **Teloxide** — Telegram bot
- **Serenity** — Discord bot

## Development

```bash
# Check
cargo check && cargo clippy

# Test (167 tests)
cargo test

# Run with debug logging
RUST_LOG=debug cargo run -- chat

# Build release
cargo build --release
```

## License

MIT
