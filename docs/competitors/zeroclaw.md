# ZeroClaw — Competitor Analysis

> Source: https://github.com/zeroclaw-labs/zeroclaw, https://zeroclaw.net/ (Feb 2026)
> Language: Rust
> License: Open source
> Maturity: Growing — Rust rewrite of OpenClaw concepts

## What It Is

ZeroClaw is the Rust-based alternative to OpenClaw. Same philosophy (personal AI assistant, multi-channel, self-hosted) but optimized for edge/embedded deployment.
It's our **closest competitor** — same language, similar goals, but different design philosophy.

The tagline: "Fast, small, and fully autonomous AI assistant infrastructure — deploy anywhere, swap anything."

## Architecture

- **Trait-based modular design**: every subsystem (provider, channel, memory, tool, tunnel) implements a trait
- **Single static binary**: ~3.4-8.8MB depending on features
- **Config**: TOML-based with profiles
- **Factory registration**: lowercase keys ("openai", "discord", "shell")
- **Gateway + Daemon modes**: webhook server + full autonomous runtime
- **Default port**: 127.0.0.1:8080

## Performance

| Metric | ZeroClaw | OpenClaw | Homun |
|--------|----------|----------|-------|
| Binary size | ~3.4-8.8MB | N/A (Node.js) | ~30MB (debug) |
| RAM | <5MB | ~1GB+ | ~50MB |
| Startup | <10ms | seconds | ~100ms |
| Min hardware | $10 boards | Mac mini | RPi 4+ |

## Channels

Extensive — more than OpenClaw:
CLI, Telegram, Discord, Slack, Mattermost, iMessage, Matrix, Signal, WhatsApp, Email, IRC, Lark, DingTalk, QQ, Nostr, webhook ingress.

Each channel requires daemon runtime for async operation.

## Provider System

- **30+ built-in providers** via `zeroclaw providers`
- OpenAI, Anthropic, OpenRouter, Together, DeepSeek, custom endpoints
- Custom format: `custom:https://your-api.com`
- Encrypted auth profiles at `~/.zeroclaw/auth-profiles.json`
- Subscription-native OAuth + API key storage

## Memory System (Key Differentiator)

**Zero-dependency custom engine** — no Pinecone, no Elasticsearch, no LangChain:
- **Vector embeddings** stored as BLOBs in SQLite with cosine similarity
- **Keyword search** via FTS5 virtual tables with BM25 scoring
- **Hybrid search**: vector similarity (0.7 weight) + keyword matching (0.3 weight)
- Embedding provider trait (OpenAI or custom URLs)
- Line-based markdown chunker preserving heading context
- LRU-evicted embedding cache within SQLite
- Safe atomic reindex (rebuild FTS5 + re-embed vectors)
- Storage backends: SQLite, Markdown files, or ephemeral

This is significantly more advanced than our file-based MEMORY.md approach.

## Tools

Built-in:
- **Shell/exec** with sandboxing
- **File operations**
- **Memory operations** (read/write/search memory explicitly)
- **Cron scheduling**
- **Git operations**
- **Pushover notifications**
- **Browser automation**
- **HTTP requests**
- **Screenshot/image analysis**
- **Composio integration** (optional)
- **Delegation** (inter-agent)
- **Hardware tools** (for IoT/embedded)

## Identity System (AIEOS)

Supports "AI Entity Object Specification" (AIEOS v1.1):
- Portable agent personas via JSON or markdown
- Full backward compatibility with OpenClaw's IDENTITY.md
- Migration tooling from OpenClaw format

We use SOUL.md + USER.md + AGENTS.md — similar concept, different format.

## Security

- **Pairing codes** for new connections (like WhatsApp QR but text-based)
- **Workspace-scoped** file access
- **Command allowlisting** (git, npm, cargo — explicit list)
- **Encrypted-at-rest** API keys
- **Sandbox enforcement** per runtime adapter
- **Rate limiting**

## Deployment

- **Homebrew**: `brew install zeroclaw`
- **Bootstrap script**: `./bootstrap.sh` (prebuilt, Docker, onboarding)
- **Pre-built binaries**: Linux (x86_64/aarch64/armv7), macOS, Windows
- **Source build**: needs 2GB+ RAM, 6GB+ disk
- **Service management**: `zeroclaw service install` (systemd-like)
- **Docker sandboxing** as runtime adapter

## CLI Commands

```
zeroclaw onboard              # Setup wizard
zeroclaw agent -m "prompt"    # Single query
zeroclaw agent               # Interactive chat
zeroclaw gateway             # Webhook server
zeroclaw daemon              # Full autonomous runtime
zeroclaw status              # Health check
zeroclaw channel doctor      # Diagnose integrations
zeroclaw service install     # Background service
zeroclaw auth login          # Credential management
zeroclaw providers           # List 30+ providers
```

## Tunnel Support

Built-in tunnel abstraction for remote access:
- None (local only)
- Cloudflare Tunnel
- Tailscale
- ngrok
- Custom tunnel binary

## Observability

Noop, Log, and Multi implementations. Extensible for Prometheus or OpenTelemetry.

## Strengths vs Homun

1. **Memory engine**: vector embeddings + FTS5 hybrid search in SQLite — far ahead of our file-based MEMORY.md
2. **30+ providers**: more than our 14
3. **16+ channels**: more than our 4
4. **Service management**: `zeroclaw service install` for background daemon
5. **Tunnel abstraction**: Cloudflare, Tailscale, ngrok built-in
6. **Hardware tools**: IoT/embedded support
7. **Identity portability**: AIEOS spec with migration from OpenClaw
8. **Observability traits**: extensible metrics/tracing
9. **Channel doctor**: diagnostic tool for integration health
10. **Encrypted auth profiles**: OAuth + API key management

## Weaknesses vs Homun

1. **No Agent Skills standard**: uses own skill/identity format (AIEOS)
2. **No Web UI** (as far as documented — CLI/Gateway/Daemon only)
3. **No ClawHub/marketplace integration** for skill discovery
4. **Less mature documentation** than OpenClaw
5. **Smaller community** (newer project)
6. **No TUI** (we have ratatui-based TUI for WhatsApp pairing, etc.)

## Key Lessons for Homun

### Must-have to compete:
1. **Memory with semantic search**: vector embeddings + keyword hybrid in SQLite
2. **Service installation**: `homun service install` for background daemon
3. **More channels**: at minimum Slack, Email, Matrix
4. **Tunnel support**: remote access without manual port forwarding
5. **Auth encryption**: proper encrypted credential storage (we have keyring, good)

### Nice-to-have:
1. Channel doctor diagnostic
2. Observability/metrics traits
3. Hardware/IoT tools
4. AIEOS identity compatibility
