# HomunBot — Project Vision & Context

## The Idea

HomunBot is a personal AI assistant that **lives in your computer** and works for you 24/7. You interact with it remotely via Telegram, WhatsApp, or locally via CLI. It's your digital homunculus — a small, loyal, intelligent creature that executes tasks, learns from interactions, and grows more capable over time through skills.

The name comes from alchemy: a **homunculus** is a small artificial being created to serve its maker. HomunBot is exactly that, but digital.

## Why HomunBot Exists

### The Problem

Current AI assistants fall into two categories:

1. **Cloud chatbots** (ChatGPT, Claude.ai) — powerful but stateless, no persistence, no automation, can't run tasks while you sleep, your data lives on their servers.

2. **Heavyweight agent frameworks** (LangChain, AutoGen, CrewAI) — complex, Python-heavy, require infrastructure, not designed for personal use.

3. **Lightweight agents** (nanobot, tinyclaw) — good idea but Python-based, fragile, slow startup, dependency hell, hard to deploy.

### The Gap

There is no **single-binary, privacy-first, skill-powered personal agent** that:
- Runs on your machine (or a cheap VPS/Raspberry Pi) with zero dependencies
- You manage from your phone via Telegram/WhatsApp
- Supports the open Agent Skills ecosystem for extensibility
- Works with any LLM (cloud or local via Ollama)
- Is fast, reliable, and just works

### Our Solution

HomunBot fills this gap. Written in Rust, it compiles to a single binary. Install it, configure your LLM API key, start the gateway, and you have a personal AI assistant accessible from anywhere.

## Positioning & Differentiation

### Competitive Landscape

| Project | Language | Skills Support | Channels | Binary | Local LLM |
|---------|----------|---------------|----------|--------|-----------|
| nanobot | Python | Custom format | TG/WA/Discord/Slack | No | Via provider |
| tinyclaw | JS/Bun | No | TG/WA/Discord | No | Via provider |
| meltbot | Python | No | TG/WA | No | No |
| **HomunBot** | **Rust** | **Agent Skills (open standard)** | **CLI/TG/WA** | **Yes** | **Ollama native** |

### Key Differentiators

1. **Single Rust binary** — no runtime, no dependencies, instant startup, ~10MB binary.
2. **Agent Skills compatible** — the ONLY personal assistant that supports the open skills.sh ecosystem (currently only coding agents like Claude Code and Cursor support it).
3. **Ollama-native** — first-class support for fully offline/private operation with local models.
4. **Designed for longevity** — Rust's reliability means this runs for months without crashes or memory leaks.

## Target Users

1. **Privacy-conscious developers** who want an AI assistant without sending everything to the cloud.
2. **Power users** who want automation (cron jobs, monitoring, daily briefings) running 24/7.
3. **Self-hosters** who run services on VPS/homelab and want a lightweight AI assistant alongside them.
4. **Open-source contributors** looking for a clean, hackable agent codebase in Rust.

## Reference Implementation: nanobot

HomunBot is a **Rust rewrite** of [nanobot](https://github.com/HKUDS/nanobot), an ultra-lightweight Python AI assistant (~4,000 lines). Claude Code should **clone the nanobot repository and study its source code** to understand the patterns we're replicating in Rust.

```bash
git clone https://github.com/HKUDS/nanobot.git /tmp/nanobot-reference
```

### Key files to study in nanobot

| nanobot file | What it does | HomunBot equivalent |
|---|---|---|
| `nanobot/agent/loop.py` | Core agent loop — ReAct pattern, LLM ↔ tool execution, max 20 iterations | `src/agent/loop.rs` |
| `nanobot/agent/context.py` | Builds the system prompt: persona + skills + tools + memory + history | `src/agent/context.rs` |
| `nanobot/agent/memory.py` | Long-term memory consolidation via LLM summarization | `src/agent/memory.rs` |
| `nanobot/agent/skills.py` | Loads skill markdown files from disk, injects into context | `src/skills/loader.rs` |
| `nanobot/agent/subagent.py` | Spawns isolated background agent for async tasks | `src/agent/subagent.rs` |
| `nanobot/agent/tools/*.py` | Tool implementations: shell, file, web, message, spawn, cron | `src/tools/*.rs` |
| `nanobot/providers/` | LLM provider abstraction (OpenRouter, Anthropic, etc.) | `src/provider/*.rs` |
| `nanobot/channels/telegram.py` | Telegram bot integration | `src/channels/telegram.rs` |
| `nanobot/bus/queue.py` | Message bus for routing between channels and agent | `src/bus/queue.rs` |
| `nanobot/session/` | Conversation session management | `src/session/manager.rs` |
| `nanobot/cron/service.py` | Cron job scheduling with apscheduler | `src/scheduler/cron.rs` |
| `nanobot/config/schema.py` | Pydantic config schema | `src/config/schema.rs` |
| `nanobot/heartbeat/service.py` | Periodic proactive wake-up | Future phase |
| `bridge/src/whatsapp.ts` | WhatsApp Node.js bridge (keep as-is, don't rewrite) | `bridge/src/whatsapp.ts` |

### What to replicate from nanobot
- The **ReAct agent loop** pattern (reason → act → observe → loop, max 20 iterations)
- The **context builder** approach (system prompt assembly with dynamic skill/tool injection)
- The **memory consolidation** strategy (LLM summarizes old messages into long-term memory)
- The **message bus** architecture (channels → inbound queue → agent → outbound queue → channels)
- The **tool execution** model (JSON schema definition, argument parsing, result formatting)
- The **skill loading** system (scan directories, read markdown, inject into prompt)
- The **subagent** pattern (spawn isolated agent loop for background tasks)
- The **WhatsApp bridge** (Node.js process, reuse their bridge/ directory as-is)

### What to improve over nanobot
- **Type safety**: nanobot uses Python dicts everywhere; we use typed Rust structs
- **Concurrency**: nanobot uses asyncio; we use tokio with real parallelism and mpsc channels
- **Storage**: nanobot uses JSON files; we use SQLite for reliability
- **Config**: nanobot uses JSON config; we use TOML (more readable, comments allowed)
- **Skills**: nanobot has a custom skill format; we use the open Agent Skills standard (skills.sh)
- **Error handling**: nanobot has bare try/except; we use Result<T> with context everywhere
- **Deployment**: nanobot requires Python + pip; we ship a single binary
- **Security**: better shell sandboxing, workspace isolation, path traversal prevention

## Agent Skills Ecosystem

HomunBot is compatible with the **open Agent Skills standard** maintained by Anthropic. The skills directory and discovery platform is at [skills.sh](https://skills.sh/).

### What are Agent Skills

Skills are reusable capability packages for AI agents. Each skill is a directory with:

```
skill-name/
├── SKILL.md          # YAML frontmatter (name, description) + markdown instructions
├── scripts/          # Optional executable code (Python, Bash, JS)
├── references/       # Optional additional docs loaded on-demand
└── assets/           # Optional static resources
```

The `SKILL.md` format:
```markdown
---
name: market-monitor
description: Monitor cryptocurrency and stock prices, alert on significant movements. Use when the user asks about market data, price tracking, or financial alerts.
license: MIT
metadata:
  author: homunbot
  version: "1.0"
---

# Market Monitor

## Instructions
When activated, use the following workflow...
```

### How skills.sh works

- Browse available skills at https://skills.sh/ (leaderboard sorted by installs)
- Skills are hosted as GitHub repositories
- Install with: `npx skills add owner/repo` (original CLI) or `homunbot skills add owner/repo` (our implementation)
- The specification is at https://github.com/agentskills/agentskills
- Currently **200+ skills** available, but almost all are coding-oriented (React, Next.js, testing, etc.)

### HomunBot's skill strategy

1. **Consume existing skills**: install any skill from skills.sh that's useful (debugging, writing, etc.)
2. **Create personal productivity skills**: we'll build skills that don't exist yet because no personal assistant supports the standard:
   - `daily-briefing` — morning summary (weather, calendar, news, tasks)
   - `market-monitor` — crypto/stock price tracking and alerts
   - `email-digest` — summarize unread emails via IMAP
   - `expense-tracker` — parse receipts, track spending
   - `habit-tracker` — daily check-ins and streaks
   - `reading-list` — manage and summarize saved articles/links
3. **Publish to skills.sh**: our skills will be installable by any agent that supports the standard

### Skill integration in HomunBot

- **Scan**: on startup, scan `~/.homunbot/skills/` and `./skills/` for `SKILL.md` files
- **Parse**: extract YAML frontmatter with `gray_matter` crate → `SkillMetadata { name, description }`
- **Register**: store metadata in `SkillRegistry` (HashMap)
- **Inject**: context builder includes all skill names + descriptions in system prompt
- **Activate**: when LLM references a skill, load full SKILL.md body into context
- **Execute**: if skill has scripts, run them via `tokio::process::Command`
- **Install**: `homunbot skills add owner/repo` clones from GitHub, extracts skill dirs

## Architecture Philosophy

### Core Principles

1. **Local-first**: everything runs on your machine. Cloud APIs are optional (you can use Ollama).
2. **Single binary**: `cargo install homunbot` and you're done. No Docker, no Python, no Node.js (except for WhatsApp bridge).
3. **Skill-powered**: capabilities come from the open Agent Skills standard, not hardcoded features.
4. **Channel-agnostic**: the agent doesn't care if the message comes from CLI, Telegram, or WhatsApp.
5. **Provider-agnostic**: swap LLMs freely — Claude, GPT, Llama, Mistral, DeepSeek — same agent, different brain.

### The Agent Loop (ReAct Pattern)

```
User Message
    ↓
┌─────────────────────────────┐
│  Context Builder            │
│  - System prompt            │
│  - Skill metadata           │  
│  - Tool definitions         │
│  - Conversation history     │
│  - Memory (long-term)       │
└─────────────┬───────────────┘
              ↓
┌─────────────────────────────┐
│  LLM Provider               │
│  (OpenRouter/Anthropic/     │
│   Ollama/OpenAI-compat)     │
└─────────────┬───────────────┘
              ↓
         Has tool calls?
        /            \
      Yes              No
       ↓                ↓
  Execute tools    Return response
       ↓                ↓
  Add results       Send to user
  to context
       ↓
  Loop (max 20 iterations)
```

### Message Flow

```
Telegram ─┐
WhatsApp ──┤──→ InboundMessage ──→ MessageBus ──→ AgentLoop ──→ OutboundMessage ──→ Channel
CLI ───────┘
```

### Skills Integration

HomunBot implements the [Agent Skills specification](https://github.com/agentskills/agentskills):

```
~/.homunbot/skills/
├── market-monitor/
│   ├── SKILL.md          # YAML frontmatter + instructions
│   ├── scripts/
│   │   └── fetch_prices.py
│   └── references/
│       └── exchanges.md
├── daily-briefing/
│   └── SKILL.md
└── email-summary/
    └── SKILL.md
```

**Progressive disclosure** (3 levels):
1. **Startup**: only skill name + description loaded into system prompt
2. **Activation**: when LLM decides a skill is relevant, full SKILL.md body is loaded
3. **Deep dive**: referenced files in `references/` and `scripts/` loaded on demand

This is the same pattern used by Claude Code, Cursor, and 20+ other agents — but HomunBot is the first **personal assistant** (not coding agent) to support it.

### Novel Skill Ideas (not coding-oriented)

Since existing skills are all coding-focused, HomunBot can pioneer personal/productivity skills:

- **daily-briefing**: compile weather, calendar, news, market data into a morning summary
- **market-monitor**: track crypto/stock prices, alert on thresholds
- **email-digest**: summarize unread emails (via IMAP)
- **expense-tracker**: parse receipts and track spending
- **habit-tracker**: daily check-ins and streaks
- **travel-planner**: research and organize trip details
- **reading-list**: manage and summarize saved articles

## Data Model

### Config (`~/.homunbot/config.toml`)
Human-readable, hand-editable. Contains API keys, channel tokens, agent settings.

### Database (`~/.homunbot/homunbot.db` — SQLite)
- **sessions**: conversation state per channel/user
- **messages**: conversation history
- **memories**: long-term memory (consolidated by LLM)
- **cron_jobs**: scheduled tasks
- **skill_state**: per-skill persistent key-value store

### Skills (`~/.homunbot/skills/`)
Standard Agent Skills directories, installed via `homunbot skills add` or manually.

### Workspace (`~/.homunbot/workspace/`)
Scratch space for the agent to read/write files during task execution.

## Development Phases

### Phase 1: Core Foundation ← DONE
- [x] Project scaffold (Cargo.toml, modules, CLAUDE.md)
- [x] Config loading from TOML
- [x] Provider trait + OpenAI-compatible implementation (covers OpenRouter, Ollama)
- [x] Basic agent loop (user → LLM → text response, no tools)
- [x] CLI channel (interactive REPL + one-shot mode)
- [x] SQLite setup with migrations
- [x] Basic conversation history (persisted in SQLite)

### Phase 2: Tools & Skills ← DONE
- [x] Tool trait + registry
- [x] Shell tool (command execution with sandboxing)
- [x] File tools (read, write, edit, list)
- [x] Web search tool (Brave API) + Web fetch tool
- [x] Tool calling integration in agent loop (ReAct iterations, max 20)
- [x] Skill loader (scan directories, parse YAML frontmatter)
- [x] Skill activation in context builder
- [x] `homunbot skills add/remove/list` CLI commands

### Phase 3: Channels & Communication ← DONE
- [x] Channel trait + message bus (tokio mpsc)
- [x] Telegram channel (teloxide, long polling)
- [ ] WhatsApp bridge (Node.js process, local HTTP API) ← deferred
- [x] Gateway command (orchestrate all channels)
- [x] Message routing (inbound → agent → outbound)

### Phase 4: Memory & Scheduling ← DONE
- [x] Memory consolidation (LLM-powered summarization)
- [x] Long-term memory retrieval in context builder
- [x] Cron scheduler (custom implementation, no external crate)
- [x] Cron tool (LLM can create/list/remove jobs via tool_use)
- [x] Cron CLI commands (`homunbot cron list/add/remove`)
- [x] Auto deliver_to (cron jobs route responses to originating channel)
- [x] Heartbeat system (periodic proactive wake-up)
- [x] Subagent system (background task spawning via spawn_subagent tool)

### Phase 5: Providers & Skills Ecosystem ← DONE
- [x] Anthropic native provider (Claude API with tool_use, content blocks, system extraction)
- [x] 14 LLM providers (anthropic, openai, openrouter, ollama, deepseek, groq, gemini, minimax, aihubmix, dashscope, moonshot, zhipu, vllm, custom)
- [x] Keyword-based provider resolution with gateway/local fallback
- [x] Skill installer (`homunbot skills add owner/repo` — GitHub fetch + tarball extraction)
- [x] Skill executor (run Python/Bash/JS/TS scripts from skill directories)
- [x] Bundled skills (daily-briefing, code-review)

### Phase 6: Polish & Release ← NEXT
- [ ] Skill hot-reload (file watcher)
- [ ] Config wizard (`homunbot config` interactive setup)
- [ ] Error recovery and graceful shutdown
- [ ] WhatsApp bridge (Node.js process)
- [ ] Documentation site (homunbot.github.io)
- [ ] Publish to crates.io
- [ ] Release binaries (GitHub Actions: Linux, macOS, ARM)

## Open Questions & Future Ideas

- **MCP integration**: should HomunBot be an MCP client (consume tools from MCP servers) or also an MCP server (expose its capabilities)?
- **Voice**: Whisper integration for voice messages via Telegram/WhatsApp
- **Multi-agent**: should skills be able to spawn sub-agents?
- **Skill marketplace**: could HomunBot host its own skill discovery beyond skills.sh?
- **Mobile app**: a minimal Flutter app for direct communication (leveraging your Flutter experience)?
- **Plugin system**: beyond skills, should there be compiled Rust plugins (dynamic loading)?

## Links

- **GitHub**: https://github.com/homunbot/homunbot
- **Email**: homunbot@gmail.com
- **Agent Skills spec**: https://github.com/agentskills/agentskills
- **Skills directory**: https://skills.sh
- **Inspiration**: https://github.com/HKUDS/nanobot
