# Implementation Gaps

Updated: March 12, 2026

This document is the operational backlog derived from the real codebase and the current roadmap.

Use it after `docs/services/README.md`. The service docs explain how things work; this file explains what is still missing, how serious it is, and what should be done next.

## Priority Ladder

### Now

- Web chat/browser hardening beyond the current manual smoke baseline (`CHAT-7`, browser release-grade E2E/hardening)
- Phase-2 channel hardening (Discord, Slack, Email, WhatsApp)

### Next

- Business expansion beyond `BIZ-1`
- Deeper operational polish around deploy/release checks

### Later

- Mobile app
- Sprint 9+ exploratory items

## Cross-Cutting Gaps

### Sandbox

Status: complete (all SBX-1 through SBX-6 done)

What exists:

- modular sandbox architecture: `src/tools/sandbox/` (11 files, ~2,400 LOC) — refactored from monolithic `sandbox_exec.rs`
- unified sandbox wiring across shell/MCP/skills via `build_process_command()` facade
- runtime status surfaced to UI/API with capability/reason per backend
- Docker-backed path and permissions UX
- `linux_native` backend via Bubblewrap with env sanitization, namespace isolation, `prlimit` memory enforcement
- `windows_native` backend via Win32 Job Objects with memory/CPU/kill-on-close enforcement
- runtime image lifecycle: policy (`pinned`, `versioned_tag`, `floating`), drift tracking, build/pull/inspect
- canonical baseline `homun/runtime-core:2026.03` with Dockerfile and build script
- CI validation suite: 3 test files (21 tests total) + GitHub Actions workflow with 5 jobs across Linux/Windows/macOS

What is still missing:

- browser-heavy runtime parity on top of the current core baseline
- first CI run on GitHub Actions to validate the suite on real runners
- Windows v2 hardening: network isolation (AppContainer), filesystem restriction (NTFS ACL)

Recommended next step:

- push and trigger the CI workflow to validate on real Linux/Windows/macOS runners

Execution plan:

- `docs/SANDBOX-EXECUTION-PLAN.md`

### Web Chat And Browser E2E

Status: partial

What exists:

- web chat runtime and persisted runs
- browser automation through Playwright MCP
- manual smoke suite under `scripts/e2e_*.sh`
- manual GitHub workflow for smoke execution
- deterministic browser-tool flow already exists as a local/self-contained smoke path
- chat smoke coverage already includes send/stop, multi-session, restore run, attachment flow, and MCP picker

What is still missing:

- `CHAT-7` in roadmap terms: release-grade formalization of the existing smoke coverage, with tighter assertions and clearer operator procedure
- deeper browser hardening and broader end-to-end coverage beyond the deterministic smoke baseline
- cross-platform confidence and explicit pre-release discipline around these manual checks

Recommended next step:

- keep the manual smoke suite as-is, but formalize a manual release checklist that requires running the chat suite and browser tool flow before deploys

## Service-by-Service Gaps

## Runtime And Config

Status: mostly solid

Main gaps:

- feature-gated product shape is still easy to misunderstand from the outside
- some runtime changes hot-reload while channel/runtime topology changes still require restart

Next work:

- document restart-required config changes more explicitly in UI/admin flows

Related docs:

- `docs/services/runtime-and-config.md`

## Agent And Gateway

Status: solid, high-centrality

Main gaps:

- gateway remains the densest integration point in the codebase
- cross-subsystem regressions are likely whenever routing rules or event flows change

Next work:

- keep this area stable while sandbox/channel hardening is done around it

Related docs:

- `docs/services/agent-and-gateway.md`

## Web Control Plane

Status: solid core, partial hardening

Main gaps:

- `CHAT-7` is partial, not closed: the smoke baseline exists but the release-grade story is still open
- multimodal/document handling needs more hardening, even though the feature exists
- manual-only E2E means regressions depend on operator discipline

Next work:

- define the exact manual pre-release web test checklist and treat it as required

Related docs:

- `docs/services/web-control-plane.md`

## Channels

Status: implemented, partial hardening

Main gaps:

- Discord needs more hardening
- Slack is polling-based and still needs further production refinement
- Email has the richest routing modes and therefore the largest behavior surface to stabilize
- WhatsApp is implemented but still treated as stabilization work in the roadmap

Next work:

- harden Email and Slack first, then Discord and WhatsApp

Related docs:

- `docs/services/channels.md`

## Providers

Status: solid

Main gaps:

- broad provider catalog does not imply equal maturity across all providers
- capability mismatches still need runtime observation and maintenance

Next work:

- no major roadmap block here; maintain and avoid churn while higher-priority hardening is ongoing

Related docs:

- `docs/services/providers.md`

## Tools

Status: solid, cross-cutting

Main gaps:

- tool behavior still depends heavily on compile-time features, runtime config, approvals, and sandbox mode
- sandbox hardening is now complete across all platforms (Docker, Linux native, Windows native)

Next work:

- maintain sandbox module stability while channel hardening proceeds

Related docs:

- `docs/services/tools.md`

## Browser

Status: partial

Main gaps:

- browser smoke coverage exists, but deeper release-grade E2E/hardening is still open
- runtime depends on external `@playwright/mcp` and local browser availability

Next work:

- keep the deterministic browser tool flow as the baseline check and expand hardening around real-world failure handling, screenshot/vision fallback, and stronger release procedure

Related docs:

- `docs/services/browser.md`

## Skills And MCP

Status: strong core

Main gaps:

- MCP and skill execution quality still inherit sandbox/runtime hardening gaps
- automation dependency invalidation should remain closely watched as MCP/skills evolve

Next work:

- no major feature gap before sandbox/channel work; keep this stable

Related docs:

- `docs/services/skills-and-mcp.md`

## Automation And Workflows

Status: strong core

Main gaps:

- the core exists, but reliability depends on the same release discipline as web/chat/browser
- future work is more about operational confidence than missing foundation

Next work:

- maintain; avoid introducing complexity until sandbox/chat/channel backlog is reduced

Related docs:

- `docs/services/automation-and-workflows.md`

## Memory And Knowledge

Status: strong core

Main gaps:

- depends on successful embeddings initialization and parser coverage
- this is not a roadmap bottleneck right now

Next work:

- maintain parser/ingestion quality opportunistically, but do not prioritize over hardening backlog

Related docs:

- `docs/services/memory-and-knowledge.md`

## Security

Status: strong web/vault core, execution hardening complete

Main gaps:

- web security milestones are closed
- execution sandbox hardening is complete: modular architecture, Linux native backend (Bubblewrap), Windows native backend (Job Objects), runtime image lifecycle, cross-platform CI validation

Next work:

- no major remaining security backlog

Related docs:

- `docs/services/security.md`

## Storage And Sessions

Status: solid

Main gaps:

- mostly maintenance/documentation drift risk rather than missing capability

Next work:

- keep migrations and service docs aligned whenever schema changes

Related docs:

- `docs/services/storage-and-sessions.md`

## Business

Status: partial

What exists:

- `BIZ-1` core engine is done

What is still missing:

- `BIZ-2` payments
- `BIZ-3` accounting/tax exports
- `BIZ-4` marketing execution
- `BIZ-5` crypto flows

Next work:

- defer until sandbox/chat/channel hardening is in a better place

Related docs:

- `docs/services/business.md`

## Service Management

Status: sufficient for current scope

Main gaps:

- no Windows service-management equivalent in this subsystem
- not a near-term bottleneck compared to sandbox and channels

Next work:

- leave stable unless deployment requirements change

Related docs:

- `docs/services/service-management.md`

## Mobile

Status: not started

Main gaps:

- there is no mobile app implementation yet

Next work:

- keep this out of the active critical path until the current desktop/web/runtime hardening backlog is reduced

## Recommended Execution Order

1. ~~Sandbox hardening~~ ✅ (all SBX-1..6 complete)
2. Web chat/browser release hardening
3. Phase-2 channel hardening
4. Business expansion
5. Mobile app

## Working Rule

When one of these gaps is picked up:

1. update the corresponding `docs/services/*.md`
2. update this file
3. update `docs/ROADMAP.md` if milestone status changed
