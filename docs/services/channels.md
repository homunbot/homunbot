# Channels

## Purpose

This subsystem owns inbound/outbound transport integrations. Channels normalize external messaging systems into `InboundMessage` and accept `OutboundMessage` from the gateway.

## Primary Code

- `src/channels/mod.rs`
- `src/channels/traits.rs`
- `src/channels/cli.rs`
- `src/channels/telegram.rs`
- `src/channels/discord.rs`
- `src/channels/slack.rs`
- `src/channels/whatsapp.rs`
- `src/channels/email.rs`

## Contract

Every channel implements the `Channel` trait:

- `start(inbound_tx, outbound_rx)`
- `name()`

The channel owns platform I/O. The gateway owns agent execution and routing.

## Implementations

### CLI

The CLI channel is the local interactive shell around `AgentLoop`. It is the simplest path and does not require the long-running gateway.

### Telegram

- long polling via `frankenstein`
- access control through `allow_from`
- optional mention gating in groups
- downloads document attachments and passes the local file path in message metadata
- handles `/start`, `/new`, `/reset`

### Discord

- uses `serenity`
- supports guild and DM traffic
- optional mention gating in guilds
- access control via `allow_from`
- ignores bot messages and can emit typing indicators

### Slack

- uses Slack Web API polling, not the Events API
- auto-discovers accessible channels when not pinned to one channel
- empty `allow_from` is fail-closed
- outbound path uses `chat.postMessage`

### WhatsApp

- native Rust client using `whatsapp-rust`
- reconnect-only in gateway mode
- pairing is intentionally done in the TUI, not by the gateway
- requires an existing SQLite session store
- empty `allow_from` is fail-closed
- ignores queued messages during a grace period after reconnect

### Email

- multi-account IMAP IDLE + SMTP
- per-account modes:
  - `assisted`
  - `automatic`
  - `on_demand`
- can require human approval before sending
- supports batching and trigger-word routing
- password and trigger-word resolution can come from encrypted secrets storage

## Secrets And Config

Channel credentials are often resolved from encrypted secrets even if config contains placeholder markers such as `***ENCRYPTED***`.

Relevant config areas:

- `[channels.telegram]`
- `[channels.discord]`
- `[channels.slack]`
- `[channels.whatsapp]`
- `[channels.email]`
- `[channels.web]`

## Maturity Notes

Code exists for all major external channels listed above, but not all of them are equally production-hardened. The roadmap still tracks more work for Discord, Slack, WhatsApp, and Email despite the code already being present.

## Failure Modes And Limits

- channel startup is mostly a one-time gateway action
- token/account changes usually require restart
- Slack is polling-based, so it is not equivalent to an event-driven bot
- WhatsApp pairing is intentionally split away from gateway runtime
- Email has the most routing modes and therefore the most edge cases

## Change Checklist

Update this document when you change:

- a channel's transport strategy
- allowlist/mention behavior
- attachment handling
- pairing/setup expectations
- delivery semantics for email or cross-channel replies
