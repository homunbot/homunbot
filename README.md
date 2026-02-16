# 🧪 HomunBot

**The digital homunculus that lives in your computer.**

HomunBot is an ultra-lightweight personal AI assistant written in Rust — a single binary, zero dependencies, skill-powered agent you manage remotely via Telegram, WhatsApp, or CLI.

> *In alchemy, a homunculus is a small artificial being created to serve its maker.
> HomunBot is yours.*

## Features

- 🦀 **Single Rust binary** — no Python, no Node.js runtime, no Docker required
- 🧠 **Skill-powered** — compatible with the open [Agent Skills](https://github.com/agentskills/agentskills) standard (skills.sh)
- 📱 **Remote control** — manage via Telegram, WhatsApp, or CLI from anywhere
- 🔒 **Local-first** — your data stays on your machine, works with Ollama for fully offline operation
- ⚡ **Lightning fast** — instant startup, minimal memory footprint
- 🔧 **Multi-provider** — OpenRouter, Anthropic, OpenAI, Ollama, DeepSeek, Groq, and any OpenAI-compatible API

## Quick Start

```bash
# Install
cargo install homunbot

# Initialize
homunbot config

# Chat
homunbot chat -m "Hello, homunculus!"

# Or interactive mode
homunbot chat

# Start gateway (Telegram + WhatsApp + cron)
homunbot gateway
```

## Configuration

Edit `~/.homunbot/config.toml`:

```toml
[agent]
model = "anthropic/claude-sonnet-4-20250514"

[providers.openrouter]
api_key = "sk-or-v1-xxx"

[channels.telegram]
enabled = true
token = "123456:ABC..."
allow_from = ["123456789"]
```

## Skills

HomunBot supports the open Agent Skills standard. Install skills with a single command:

```bash
homunbot skills add vercel-labs/agent-skills
homunbot skills add anthropics/skills
homunbot skills list
```

## License

MIT
