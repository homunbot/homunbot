# Storage And Sessions

## Purpose

This subsystem owns durable persistence for Homun. SQLite is the main system of record, while encrypted secrets storage and markdown memory files cover the non-SQL data that must survive restarts.

## Primary Code

- `src/storage/db.rs`
- `src/storage/secrets.rs`
- `src/storage/mod.rs`
- `src/session/manager.rs`

## SQLite

`Database::open()` creates the parent directory, enables WAL mode, turns on foreign keys, and runs migrations automatically.

The current migration chain reaches at least:

- initial sessions/messages
- memory chunks
- users and identities
- token usage
- email pending queue
- automations and runs
- automation plan metadata
- persisted web chat runs
- RAG knowledge
- workflows
- business tables
- skill audit
- web auth

The migration tracker table is `_migrations`.

## Main Persistent Domains

SQLite currently stores:

- sessions and messages
- memory chunks
- users, identities, webhook tokens
- token usage aggregates
- email pending approvals
- cron jobs
- automations and automation runs
- web chat runs
- RAG sources and chunks
- workflows and workflow steps
- business entities
- skill audit records
- web auth data

## Sessions

`SessionManager` is a thin SQLite-backed session abstraction used by the agent loop and channels. Messages are append-only for LLM cache efficiency. History retrieval converts stored messages into provider-facing chat messages.

Session key format is `channel:chat_id`.

## Encrypted Secrets

Secrets persistence is not inside SQLite. It uses:

- `~/.homun/secrets.enc`
- keychain or `~/.homun/master.key`

This is why database backups are not enough to fully migrate a runtime unless the secrets path is also handled.

## Non-SQL Files

Other important persistent files live under `~/.homun/`:

- `config.toml`
- `secrets.enc`
- `master.key`
- `brain/MEMORY.md`
- `brain/HISTORY.md`
- `brain/INSTRUCTIONS.md`
- vector index files
- PID and log files

## Failure Modes And Limits

- restoring only SQLite without the secrets/key material produces an incomplete runtime
- migration drift is possible if new tables are added but docs are not updated
- session history and consolidated memory are intentionally separate persistence layers

## Change Checklist

Update this document when you change:

- migration set
- session schema/behavior
- secrets storage format
- locations of persisted runtime files
