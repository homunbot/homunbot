# Runtime And Config

## Purpose

This subsystem defines how Homun is assembled and booted. It owns feature-gated binary composition, config loading/saving, startup modes, and the boundary between live in-memory config and persisted disk config.

## Primary Code

- `src/main.rs`
- `src/config/schema.rs`
- `src/config/mod.rs`
- `src/config/dotpath.rs`
- `src/service/mod.rs`

## Runtime Role

Homun is a single Rust binary whose actual shape depends on compile-time features. The important ones in the current codebase are:

- `web-ui`
- `mcp`
- `browser`
- `embeddings`
- `channel-telegram`
- `channel-discord`
- `channel-whatsapp`
- `channel-email`
- `vault-2fa`

The three practical runtime entry modes are:

- `homun chat`
  Local interactive or one-shot agent execution.
- `homun gateway`
  Long-running multi-channel runtime with scheduler, web UI, workflows and business engine.
- `homun config`
  TUI-oriented configuration flow.

## Config Ownership

The root config is `~/.homun/config.toml`. `Config::load()` returns defaults if the file is missing. `Config::save_to()` intentionally strips virtual MCP servers, especially the Playwright browser server injected from `[browser]`, so generated runtime state is not written back to disk.

Owned config areas:

- `[agent]`
- `[providers]`
- `[channels]`
- `[tools]`
- `[storage]`
- `[memory]`
- `[knowledge]`
- `[mcp]`
- `[permissions]`
- `[security]`
- `[browser]`
- `[ui]`
- `[business]`
- `[skills]`

Useful derived paths:

- config file: `~/.homun/config.toml`
- data dir: `~/.homun/`
- workspace dir: `~/.homun/workspace/`

## Hot Reload Boundaries

Not all config changes behave the same way.

- The agent and web server share `Arc<RwLock<Config>>`, so many prompt/provider/tool decisions see updated config on the next request.
- Provider selection can rebuild lazily when `config.agent.model` changes.
- Web UI saves config both to disk and to the shared in-memory config.
- Gateway channel startup uses a startup snapshot of config. Channel enablement, tokens, and account lists generally require a gateway restart.
- Browser MCP is derived at startup from `[browser]`.

## Secrets Versus Config

Plain config is not the only configuration surface.

- provider API keys can live in encrypted secrets storage
- channel tokens can live in encrypted secrets storage
- some MCP preset env vars are stored in the vault and referenced as `vault://...`

`Config::is_provider_configured()` and `Config::is_channel_configured()` already check both config and encrypted storage.

## Startup Behavior

At startup the binary:

1. installs the rustls crypto provider
2. parses the CLI command
3. initializes logging
4. loads config
5. opens SQLite if the mode requires persistence
6. assembles providers, tools, MCP servers, workflows, web server, gateway, and optional search engines depending on features and mode

Gateway mode also manages the PID file in `~/.homun/homun.pid`.

## Failure Modes And Limits

- Some features are present only in richer builds; docs must not assume `cargo install --path .` equals `--features full`.
- Missing secrets can leave a service configured in the UI but not actually runnable.
- Channel runtime state is not fully hot-reloaded.
- Browser/MCP startup still depends on external Node.js tooling even though the core runtime is Rust.

## Change Checklist

When changing runtime composition or config semantics, also update:

- `README.md`
- `docs/UNIFIED-ROADMAP.md` if feature status changed
- the corresponding service document in this folder
