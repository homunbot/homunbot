# Homun Trust Model

This document defines who can interact with Homun, how they authenticate, what they can do, and where their state lives.

## Principals

| Principal | How they authenticate | Powers | State |
|-----------|----------------------|--------|-------|
| **Local admin** | Web login (PBKDF2 password) | Full control: config, vault, channels, tools, memory, agents | SQLite `users` table, session in-memory |
| **Remote admin** | Web login + device approval (if enabled) | Same as local admin, with session binding (IP + UA drift warnings) | Same + `trusted_devices` table |
| **API client** | Bearer token (`wh_*` webhook token) | Scoped: `admin` (full), `chat` (send/receive), `read` (query only) | SQLite `webhook_tokens` table |
| **Channel sender** | Channel-specific identity (Telegram ID, Discord ID, WhatsApp JID) | Send messages, trigger agent. Blocked if pairing required and not paired | `user_identities` table, `contacts` table |
| **Webhook caller** | Bearer token (`wh_*`) | Inject messages into agent (untrusted content, framed as `[INCOMING WEBHOOK]`) | `webhook_tokens` table |
| **MCP service** | OAuth or API key (per-service) | Execute tools exposed by the MCP server. Sandboxed by MCP protocol | `mcp_servers` table, OAuth tokens in vault |
| **Agent** | Internal (no external auth) | Execute tools, read/write memory, send messages, spawn subagents. Bounded by tool registry + approval gates | In-memory `AgentLoop`, config-defined `AgentDefinition` |
| **Cron job** | Internal scheduler | Trigger agent with predefined prompt. Same tool access as agent | SQLite `cron_jobs` table |

## Authentication Methods

### Web sessions
- PBKDF2 (600K iterations) password hashing
- HMAC-SHA256 signed session cookies (`homun_session`)
- CSRF protection via `homun_csrf` cookie + `X-CSRF-Token` header
- Session binding: client IP + User-Agent captured at login, drift logged
- Configurable TTL (`session_ttl_secs`, default 24h)
- SameSite=Strict cookies

### Device approval (opt-in)
- Enabled via `require_device_approval = true`
- SHA-256 fingerprint of `user_id + User-Agent`
- 6-digit OTP code for new device approval
- Approved devices stored in `trusted_devices` table

### Bearer tokens
- Webhook tokens (`wh_*` prefix) with configurable scope
- Created via web UI (Account > Webhook Tokens)
- Bypass CSRF (token itself proves identity)
- Rate limited (60 req/min per IP)

### Channel pairing
- Unknown senders get 6-digit OTP challenge
- Paired senders stored in `user_identities` table
- Configurable: `pairing_required` per channel

## Trust Boundaries

```
┌─────────────────────────────────────────────┐
│                  TRUSTED                     │
│  Agent loop, tools, memory, vault, config    │
│  (runs as local process, full OS access)     │
├─────────────────────────────────────────────┤
│              AUTHENTICATED                   │
│  Web sessions, Bearer tokens, paired senders │
│  (identity verified, scoped permissions)     │
├─────────────────────────────────────────────┤
│              UNTRUSTED                       │
│  Webhook payloads, MCP tool results,         │
│  browser page content, channel messages      │
│  from unknown senders                        │
│  (framed as untrusted, injection-scanned)    │
└─────────────────────────────────────────────┘
```

## Content Trust Levels

| Source | Trust | Treatment |
|--------|-------|-----------|
| Config files | Full | Direct use |
| User messages (paired) | High | Processed by agent |
| User messages (unpaired) | Low | Pairing challenge first |
| Tool results | Medium | Injection scanning (SEC-13) |
| Webhook payloads | Low | `[INCOMING WEBHOOK — UNTRUSTED CONTENT]` framing |
| Browser page content | Low | `browser page content (untrusted)` labeling (SEC-15) |
| MCP service responses | Medium | Protocol-sandboxed |
| RAG document content | Medium | Sensitive data vault-gating |

## Security Controls Summary

| Control | Location | Default |
|---------|----------|---------|
| Rate limiting (auth) | `auth.rs` | 5/min per IP |
| Rate limiting (API) | `auth.rs` | 60/min per IP |
| CSRF protection | `csrf.js` + `auth.rs` | On (all POST/PUT/PATCH/DELETE) |
| X-Forwarded-For | `auth.rs` | Off (enable behind proxy) |
| Device approval | `auth.rs` | Off (enable for remote) |
| Session binding | `auth.rs` | On (warn on drift) |
| Injection scanning | `security/exfiltration.rs` | On (7 patterns) |
| Vault encryption | `storage/secrets.rs` | AES-256-GCM + OS keychain |
| Emergency stop | `security/estop.rs` | Available via UI + API |
| Tool approval | `tools/approval.rs` | Configurable per autonomy level |
