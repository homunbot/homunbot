# Web Control Plane

## Purpose

This subsystem owns the authenticated web application, REST API, WebSocket chat streaming, setup/login flow, and the server-side state needed to operate Homun remotely.

## Primary Code

- `src/web/server.rs`
- `src/web/api.rs`
- `src/web/pages.rs`
- `src/web/ws.rs`
- `src/web/run_state.rs`
- `src/web/auth.rs`
- `src/web/chat_attachments.rs`

## Runtime Modes

The web server can run in two modes:

- full mode
  Shared with the live agent/gateway and backed by SQLite.
- setup-only mode
  Used when no provider is configured; exposes setup/config UX without a live agent runtime.

## Shared State

`AppState` holds the live server state:

- shared config handle
- optional inbound message sender to the gateway
- `WebRunStore` for in-memory run tracking
- WebSocket maps for normal session messages and streaming events
- optional database handle
- optional memory searcher
- optional RAG engine
- optional provider health tracker
- optional workflow engine
- optional business engine
- shared E-stop handles
- optional auth session store
- auth and API rate limiters

## What The Web Layer Exposes

The current codebase exposes more than a chat screen. The web layer includes:

- setup wizard
- login/session management
- dashboard
- chat
- channels config
- provider config and health
- automations
- workflows
- business screens
- memory and knowledge views
- vault and permissions views
- logs
- browser settings/testing
- MCP management
- skills management

## Chat And Streaming

Web chat is not just a REST request/response flow.

- messages are submitted through API handlers into the gateway/agent path
- incremental chunks stream back over WebSockets
- tool start/end events are forwarded to the UI
- web chat runs are also persisted in SQLite so interrupted runs can be restored or marked interrupted after restart

On startup, stale persisted web runs are marked interrupted.

## Auth And Rate Limiting

The web auth layer currently includes:

- server-side session storage
- auth endpoint rate limiting
- general API rate limiting
- optional vault-backed capabilities depending on what is available at startup

If the session store cannot be initialized, the server logs the problem; setup-only or degraded behavior is possible depending on the runtime path.

## Deployment Notes

The web server binds to the configured host/port from `[channels.web]`. In Docker, Caddy handles TLS termination and reverse-proxies to Homun on port 18080. For standalone use, the server can auto-generate a self-signed cert (`auto_tls = true`).

## Manual E2E Coverage

Manual Playwright MCP smoke coverage exists under `scripts/e2e_*.sh`, including:

- web shell smoke
- chat send/stop
- multi-session
- restore-run
- attachments
- MCP picker
- browser smoke
- deterministic browser tool flow

These are intentionally manual today. They give the project a concrete smoke baseline for `CHAT-7`, but not a fully closed release-grade E2E program yet.

## Failure Modes And Limits

- `CHAT-7` is still partial at roadmap level: the smoke suite exists, but stronger assertions and operator release discipline are still required.
- Full CI-backed E2E coverage is still not wired into the main CI job.
- Some web features degrade when optional subsystems are not present in the build.
- The web server shares config live with the agent, but external channels do not hot-reload the same way.

## Change Checklist

Update this document when you change:

- auth/session behavior
- web chat run lifecycle
- pages/API ownership
- WebSocket event formats
- deployment assumptions (domain, TLS, reverse proxy)
