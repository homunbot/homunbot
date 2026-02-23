# OpenClaw — Competitor Analysis

> Source: https://docs.openclaw.ai/ (Feb 2026)
> Language: TypeScript/Node.js
> License: Open source
> Maturity: Production — used by hundreds of self-hosters

## What It Is

OpenClaw is the most feature-complete open-source personal AI assistant.
It's the "big brother" in the ecosystem — Node.js based, feature-rich, but heavy.
Originally born as a Telegram bot, it has grown into a multi-channel gateway with web UI, automation, and multi-agent routing.

## Architecture

- **Gateway model**: single process manages all channels, routing, sessions
- **Node.js runtime**: requires Node 22+, heavy memory footprint (~1GB+)
- **Config**: JSON at `~/.openclaw/openclaw.json`
- **Default agent**: bundled Pi binary in RPC mode with per-sender sessions
- **Web UI**: Dashboard, WebChat, Control UI, TUI — all included
- **Default port**: 127.0.0.1:18789

## Channels (30+)

Native: WhatsApp, Telegram, Discord, iMessage, Slack, Teams, Signal, Matrix, Mattermost, IRC, LINE, Google Chat, Feishu, Zalo, Lark, DingTalk, QQ, Nostr, webhook ingress.

Features:
- Channel routing (intelligent message distribution)
- Broadcast groups (multi-channel messaging)
- Media support (images, audio, documents across channels)
- Per-sender session isolation

## Agent System

- Multi-agent routing with isolated sessions per agent/workspace/sender
- Context-aware processing with customizable system prompt
- Streaming and chunking for large responses
- Typing indicators and presence tracking
- Usage tracking for cost monitoring
- Model failover mechanisms
- Agent-to-agent communication via `agent_send` tool

## Memory & Identity System

OpenClaw usa un sistema a file markdown strutturati nel workspace dell'agente:

```
~/.openclaw/
├── AGENTS.md          # Multi-agent workflows, delegation patterns
├── SOUL.md            # Behavioral guidelines, personality, principles
├── TOOLS.md           # Tool capabilities, integration gotchas
├── MEMORY.md          # Long-term memory (main session only)
├── memory/            # Daily memory files
│   └── YYYY-MM-DD.md  # Memoria giornaliera con dettagli
```

**SOUL.md** — Non solo "chi sei" ma come rispondi: tono, principi, linee guida comportamentali. Definisce la *personalita'* dell'agente.

**AGENTS.md** — Pattern di delegazione multi-agente, quando e come spawnare subagent, workflow complessi.

**TOOLS.md** — Non solo l'elenco tool ma gotchas, best practices, quando usare cosa.

**MEMORY.md** — Memoria a lungo termine consolidata. Solo sessione principale.

**memory/YYYY-MM-DD.md** — Memoria giornaliera. Permette all'agente di ricordare cosa e' successo specifici giorni. Pattern potente per "cosa ho fatto ieri?" o "riprendi il task di lunedi'".

Questo approccio e' piu' ricco del nostro: noi abbiamo SOUL.md, USER.md, AGENTS.md ma mancano:
- TOOLS.md (meta-conoscenza sui tool)
- memory/ giornaliera (solo MEMORY.md monolitico)
- Il consolidamento non funziona ancora

## Tools

Built-in:
- **Browser automation** (Chrome-based, supports login persistence)
- **Exec tool** (command execution)
- **Background process execution**
- **apply_patch** (file modifications)
- **LLM Task** (internal AI processing — use LLM as a tool)
- **Web tools** (HTTP interactions)
- **Agent Send** (inter-agent communication)

Advanced:
- **Lobster**: typed workflow runtime — composable pipelines with approval gates
- **ClawHub**: community skill/plugin marketplace
- **Plugin system** for extensibility
- **Skills with configuration support**
- **Slash commands** for user interaction
- **Reactions** for feedback

## Automation & Scheduling

Rich automation system with multiple trigger types:
- **Cron jobs** with configurable scheduling
- **Heartbeat** (periodic proactive wake-up — docs explain "cron vs heartbeat" trade-offs)
- **Webhook integrations** (inbound triggers)
- **Poll-based monitoring**
- **Gmail PubSub** (email triggers)
- **Auth monitoring** (security event triggers)
- **Hooks** for custom event handling

## Deployment

Extensive deployment options:
- Docker / Podman
- Kubernetes via Ansible
- Cloud: GCP, Fly.io, Railway, Render, Northflank, Hetzner
- Local: Nix, Node.js, Bun (experimental)
- macOS (bundled Gateway), Linux, Windows (WSL2)
- Mobile: Android, iOS nodes
- Access: local browser, SSH tunneling, Tailscale

## Model Providers

Anthropic, OpenAI, OpenRouter, Amazon Bedrock, GLM, MiniMax, Moonshot AI, Qianfan, Z.AI, Vercel AI Gateway, OpenCode Zen, local models.

## Mobile/Desktop Nodes

- iOS and Android nodes with pairing
- Audio/voice notes processing
- Camera capture
- Voice wake detection
- Talk mode (conversational input)
- Location commands
- Canvas support for rich interactions
- macOS native app

## Security

- OAuth integration
- Sandbox execution
- Elevated mode for privileged operations
- Tool policy enforcement
- Gateway lock mechanisms
- Trusted proxy authentication
- Bonjour discovery (local network)
- Tailscale integration

## CLI

40+ commands for agent management, channel config, monitoring, diagnostics.

## Strengths vs Homun

1. **Channel breadth**: 30+ channels vs our 4 (CLI, TG, Discord, WA)
2. **Automation depth**: cron + heartbeat + webhooks + Gmail + hooks vs our cron-only
3. **Browser automation**: built-in Chrome CDP tool
4. **Multi-agent routing**: isolated sessions per agent/workspace/sender
5. **Mobile nodes**: iOS/Android apps with voice, camera, location
6. **Lobster workflows**: typed composable pipelines with approval gates
7. **Media handling**: images, audio, documents across all channels
8. **apply_patch**: structured file modification tool
9. **LLM Task**: use LLM as an internal sub-tool
10. **Deployment variety**: 10+ deployment platforms documented

## Weaknesses vs Homun

1. **Heavy**: Node.js, ~1GB RAM, slow startup
2. **Complex setup**: requires Node 22+, many config options
3. **No single binary**: dependencies, npm ecosystem
4. **Skills via ClawHub**: had security issues (ClawHavoc)
5. **No Agent Skills standard**: custom plugin format
6. **No Rust safety**: memory leaks, runtime crashes possible
7. **Cost**: needs Mac mini or decent server to run well
