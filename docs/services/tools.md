# Tools

## Purpose

This subsystem owns the built-in tool contract, tool registration, execution context, and the boundary between the agent loop and concrete side effects such as shell, file, web, vault, automation, workflow, business, and MCP-backed actions.

## Primary Code

- `src/tools/mod.rs`
- `src/tools/registry.rs`
- `src/main.rs`
- `src/tools/sandbox/` (modular sandbox execution layer, 11 files)
- individual tool modules under `src/tools/`

## Core Contract

Every tool implements the `Tool` trait:

- `name()`
- `description()`
- `parameters()`
- `execute(args, ctx)`

`ToolRegistry` is the single runtime registry used by the agent loop.

## Execution Context

`ToolContext` carries the per-call execution context:

- workspace path
- channel name
- chat id
- optional proactive message sender
- optional approval manager
- optional skill-specific environment variables

This is the main way tool behavior changes across CLI, web, channel, and skill execution.

## What Gets Registered

`create_tool_registry()` in `src/main.rs` registers the base built-in tools. Depending on build flags and runtime config, the registry can include:

- shell
- read/write/edit/list file
- web search
- web fetch
- vault
- create automation
- create skill
- email inbox
- remember
- knowledge
- cron
- send message
- spawn subagent
- workflow
- business
- MCP tools
- unified browser tool

## Permissions And Sandboxing

The tool layer is where execution constraints are attached.

- shell tool receives timeout, workspace restriction, shell permissions, sandbox config, and shared config
- file tools use ACL-style permission config and optional workspace restriction
- approval manager is initialized here for approval-gated actions
- MCP tools can also be launched through the sandbox builder
- the shared sandbox facade now resolves `none`, `docker`, `linux_native`, and the placeholder `windows_native` path through the same execution entrypoint
- sandbox runtime image inspection is now lifecycle-aware: the configured image reference and optional explicit policy are resolved into a policy/drift view instead of being treated as raw text only
- the repo now ships a canonical core Docker baseline for sandboxed runtimes in `docker/sandbox-runtime/Dockerfile`
- sandbox image operations now include inspect, pull, and local build of the canonical baseline `homun/runtime-core:2026.03`

## Browser And MCP Special Case

Browser automation is not registered as a flat set of Playwright-prefixed tools in the main runtime. MCP-discovered browser tools are collapsed behind one unified `browser` tool, while other MCP tools are registered individually.

## Failure Modes And Limits

- tool availability depends on both compile-time features and runtime config
- missing APIs or secrets can silently remove a tool from the effective registry
- tool behavior can differ between stateless and persistent MCP servers
- `linux_native` sandbox backend is implemented with Bubblewrap and has CI validation via `tests/sandbox_linux_native.rs` (8 tests on Ubuntu runner)
- `windows_native` sandbox backend is implemented with Win32 Job Objects (memory/CPU/kill-on-close); network and filesystem isolation are not enforced in v1
- the canonical runtime baseline currently covers core skill/MCP runtimes, not full browser-complete Docker parity
- cross-platform E2E sandbox tests exist in `tests/sandbox_e2e.rs` with CI workflow running on Linux/Windows/macOS

## Change Checklist

Update this document when you change:

- built-in tool registration
- tool context fields
- permission/sandbox wiring
- approval integration
- the browser/MCP registration split
