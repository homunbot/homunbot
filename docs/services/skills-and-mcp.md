# Skills And MCP

## Purpose

This subsystem owns Homun's extensibility surfaces:

- local and installed skills
- skill creation, adaptation, install, execution, and security scanning
- MCP server presets, setup helpers, runtime process management, and tool discovery

## Primary Code

- `src/skills/mod.rs`
- `src/skills/loader.rs`
- `src/skills/creator.rs`
- `src/skills/installer.rs`
- `src/skills/adapter.rs`
- `src/skills/security.rs`
- `src/skills/watcher.rs`
- `src/tools/mcp.rs`
- `src/mcp_setup.rs`

## Skills

The runtime follows the Agent Skills model with progressive disclosure.

Scan locations:

- `~/.homun/skills/`
- `./skills/`

Load behavior:

1. startup loads only metadata from `SKILL.md`
2. runtime eligibility checks can mark a skill ineligible
3. full skill body is loaded on demand when activated
4. scripts/references/assets are used only as needed

Skill metadata can also restrict:

- user invocability
- model invocability
- allowed tools
- required binaries/env vars/OS

## Skill Security

Skill installation is not treated as trusted by default.

- package scanning exists before install
- risk score and warnings are surfaced
- installs can be blocked or force-overridden
- installed skills are also audited in SQLite

## Skill Execution

Skill scripts can run with normal execution or sandbox-aware execution. `ToolContext.skill_env` exists specifically so activated skills can inject their environment variables into subprocess execution.

## MCP Runtime

MCP server configs live in `[mcp.servers]`. `McpManager` is responsible for:

1. starting enabled servers
2. performing the MCP handshake
3. listing tools
4. turning each tool into a Homun `Tool`
5. exposing connected server info for status/UI
6. shutting down servers cleanly

Tool naming is server-prefixed, for example `filesystem__read_file`.

## Guided MCP Setup

`src/mcp_setup.rs` provides the guided setup path used by CLI/UI for curated MCP services. It can:

- render templated arguments
- parse env assignments
- store secret env vars in the vault and rewrite them to `vault://...`
- save the server config into `[mcp.servers]`
- test the server connection

## Runtime Nuance

MCP tools can run in two different styles:

- persistent peer, used for stateful servers like Playwright
- one-shot process call path, used for stateless servers when runtime hot reload is needed

That distinction matters when debugging differences between browser behavior and other MCP integrations.

## Failure Modes And Limits

- skills can exist on disk but be excluded from the prompt because runtime requirements are not met
- MCP runtime depends on external commands and often Node.js packages
- server removal or config drift can invalidate existing automations

## Change Checklist

Update this document when you change:

- skill metadata semantics
- install/security scanning rules
- MCP server lifecycle behavior
- guided MCP setup behavior
- stateful versus stateless MCP execution paths
