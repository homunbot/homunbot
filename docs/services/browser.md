# Browser

## Purpose

This subsystem owns browser automation as exposed to the agent. In the current architecture the browser is not a native Rust browser driver; it is a managed Playwright MCP server translated from Homun's `[browser]` config into an MCP runtime.

## Primary Code

- `src/browser/mod.rs`
- `src/browser/mcp_bridge.rs`
- `src/tools/browser.rs`
- `src/tools/mcp.rs`
- `src/config/schema.rs`

## Runtime Model

The browser runtime is synthesized from `[browser]` config:

1. Homun loads `[browser]`
2. `browser_mcp_server_config()` converts it into an MCP server config
3. the server is injected under the canonical MCP server name `playwright`
4. `McpManager` connects to `@playwright/mcp`
5. Homun wraps the Playwright MCP peer behind a unified `browser` tool

This is why browser state lives in the MCP peer/session, not in a separate custom browser daemon.

## What Browser Config Controls

The current bridge maps browser config into MCP launch flags:

- browser type
- headless mode
- explicit executable path
- persistent user data dir
- viewport size
- `PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1`

The current implementation is explicitly designed to prefer the system browser instead of a bundled Playwright browser download.

## Why It Is Unified As One Tool

The runtime intentionally does not expose raw MCP Playwright tools directly to the model as a wide flat tool list. Instead:

- browser MCP tools stay behind the `browser` tool
- other MCP servers still register server-prefixed tools normally

This keeps browser workflows coherent and allows Homun-specific routing/planning around browser use.

## State And Persistence

Browser persistence comes from the configured user data dir for the selected browser profile. That is what keeps cookies, sessions, and page context available across actions and restarts.

## Manual E2E Coverage

The current manual browser smoke coverage is in:

- `scripts/e2e_browser_smoke.sh`
- `scripts/e2e_browser_tool_flow.sh`
- `scripts/fixtures/browser_tool_flow.html`

The deterministic tool flow uses a local `data:` page instead of a third-party website, so it is useful for debugging the actual browser tool path without external dependencies.

These checks are intentionally manual today. They are the current baseline for browser validation, but they should not be read as a fully closed release-grade E2E program yet.

## Failure Modes And Limits

- requires `npx` and a locally available browser
- depends on the external `@playwright/mcp` package
- the canonical sandbox runtime baseline currently covers core runtimes, not full browser-complete Docker parity
- deterministic browser smoke coverage exists, but roadmap hardening and release-grade E2E depth are still marked partial
- browser cancellation and cleanup are largely delegated to the MCP server/runtime

## Change Checklist

Update this document when you change:

- browser config mapping
- MCP server name or launch strategy
- browser tool registration behavior
- browser smoke/E2E assumptions
