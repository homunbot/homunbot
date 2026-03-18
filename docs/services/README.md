# Service Documentation

This directory is the implementation-oriented map of Homun.

Use these documents when you need to understand how a subsystem works today in code, not what the product was supposed to become in the roadmap. The intent is simple:

- keep one document per bounded context
- anchor each document to the Rust modules that actually implement it
- record operational behavior, persistence, config ownership, and known limits

For the prioritized backlog and roadmap, use [../UNIFIED-ROADMAP.md](../UNIFIED-ROADMAP.md).

## How To Use This Folder

Read in this order when you are onboarding or planning a change:

1. `runtime-and-config.md`
2. `agent-and-gateway.md`
3. `web-control-plane.md`
4. the specific service you are modifying

## Service Map

- `runtime-and-config.md`
  Binary composition, feature flags, config loading, startup modes, hot-reload boundaries.
- `agent-and-gateway.md`
  Agent loop, context building, bus routing, gateway orchestration, streaming.
- `web-control-plane.md`
  Web UI server, auth, API, WebSocket chat, persisted web run state.
- `channels.md`
  CLI, Telegram, Discord, Slack, WhatsApp, Email channel behavior and routing.
- `providers.md`
  Provider abstraction, provider resolution, health tracking, XML fallback.
- `tools.md`
  Built-in tool registry, tool execution contract, runtime context and permissions.
- `browser.md`
  Browser automation through Playwright MCP and the unified `browser` tool.
- `skills-and-mcp.md`
  Skill loading/execution, install/security scanning, MCP presets and runtime servers.
- `automation-and-workflows.md`
  Cron, automations, runtime plan compilation, workflow engine lifecycle.
- `memory-and-knowledge.md`
  Memory consolidation, memory search, RAG ingestion/search/watchers/cloud sync.
- `security.md`
  Vault, exfiltration filtering, pairing, E-stop, optional vault 2FA.
- `storage-and-sessions.md`
  SQLite persistence, migrations, encrypted secrets, session history model.
- `business.md`
  Business autopilot domain, OODA prompts, revenue/expense tracking, current limits.
- `service-management.md`
  `homun service` support on Linux/macOS and runtime process management expectations.

## Source Of Truth Rules

- `docs/UNIFIED-ROADMAP.md` is the planning document.
- this folder is the implementation document set
- when they disagree, trust the code first and update the docs

## Maintenance Rule

When a change touches a subsystem boundary, update:

1. the relevant file in `docs/services/`
2. `README.md` if user-facing behavior changed
3. `docs/UNIFIED-ROADMAP.md` if milestone status changed

## Writing Standard

Use [TEMPLATE.md](./TEMPLATE.md) for new subsystem docs or large rewrites.
