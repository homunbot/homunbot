# Agent And Gateway

## Purpose

This subsystem owns the execution loop that turns inbound messages into model calls, tool executions, session updates, and outbound responses. It also owns the long-running gateway that routes messages between channels, schedulers, workflows, business events, and the web UI.

## Primary Code

- `src/agent/agent_loop.rs`
- `src/agent/context.rs`
- `src/agent/gateway.rs`
- `src/agent/stop.rs`
- `src/agent/subagent.rs`
- `src/bus/queue.rs`
- `src/session/manager.rs`

## Runtime Flow

The canonical path is:

1. a channel or scheduler emits `InboundMessage`
2. gateway receives it and derives routing/session context
3. `AgentLoop` loads session history
4. `ContextBuilder` assembles the system prompt
5. provider is invoked with tool definitions
6. tool calls execute through `ToolRegistry`
7. final text is redacted if needed and returned as `OutboundMessage`
8. gateway routes the response back to the correct channel or WebSocket stream

Web chat can also receive `StreamMessage` chunks for incremental UI updates and tool events.

## Context Builder

`ContextBuilder` is the place where the effective system prompt is composed. It pulls from:

- bootstrap files:
  - `~/.homun/brain/SOUL.md`
  - `~/.homun/AGENTS.md`
  - `~/.homun/brain/USER.md`
  - `~/.homun/brain/INSTRUCTIONS.md`
- loaded skill summaries
- long-term memory
- relevant memory search results
- RAG search results
- contextual MCP suggestions
- channel and email-account information
- registered tool names

This means prompt behavior is not defined by a single static template; it is assembled from runtime state.

## Agent Loop Behavior

Important properties of `AgentLoop` today:

- provider is stored behind `RwLock` and can be rebuilt lazily when the configured model changes
- tools live in `Arc<RwLock<ToolRegistry>>` so deferred MCP tools can be added after startup
- XML tool dispatch is supported for providers/models that do not reliably support native function calling
- browser tool state is kept separately when MCP/browser features are enabled
- memory consolidation, memory search, and RAG are optional integrations layered into the loop

The loop is ReAct-style: model call, detect tool calls, execute, append results, repeat until completion or max iterations.

## Gateway Role

`Gateway` is the orchestration shell around `AgentLoop`. It owns:

- inbound channel startup
- outbound channel routing
- cron and automation event handling
- workflow event handling
- tool-originated proactive messages
- forwarding web stream chunks to the web server
- shared provider health tracker
- shared E-stop handles

The gateway takes a config snapshot for channel startup, but it shares live config with the agent and web server.

## Sessions

Conversation history is stored in SQLite via `SessionManager`, not in JSONL files. The session key format is `channel:chat_id`. Messages are append-only and later compacted/consolidated by the memory subsystem.

## Failure Modes And Limits

- Channel runtime state is mostly static after gateway startup.
- If a provider changes in config, the next request can rebuild it, but in-flight work keeps using the current provider instance.
- Browser cancellation is currently soft: the MCP server owns browser process cleanup.
- The gateway is the place where cross-subsystem behavior accumulates fastest, so drift risk is high if docs are not maintained.

## Change Checklist

Update this document when you touch:

- prompt composition
- gateway routing rules
- session key semantics
- streaming/event flow
- stop/cancel propagation
