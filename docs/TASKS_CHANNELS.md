# Homun — Channel Implementation Tasks

> Last updated: 2026-02-23
> Reference: ZeroClaw channels (`src/channels/`), OpenClaw docs

---

## Current State

| Channel | Status | Implementation |
|---------|--------|----------------|
| CLI | ✅ Done | `src/channels/cli.rs` |
| Web UI | ✅ Done | WebSocket, `src/web/` |
| Telegram | ✅ Done | teloxide, long polling |
| Discord | ✅ Done | serenity, Gateway |
| WhatsApp | ✅ Done | wa-rs (native Rust, no bridge!) |

---

## Priority 1 — High Impact

### T-CH-01: Slack Channel

**Priority: P1 | Complexity: Medium | Est: 2-3 days**

**Why**: Largest enterprise user base, business use case

**Implementation**:
- Crate: `slack-morphism` or custom with `tokio-tungstenite`
- Mode: Socket Mode (no public endpoint needed)
- Config needed:
  - `bot_token` (xoxb-...)
  - `app_token` (xapp-...) for Socket Mode
  - `channel_id` for listening

**Files to create**:
- `src/channels/slack.rs`

**Config**:
```toml
[channels.slack]
enabled = true
bot_token = "***ENCRYPTED***"
app_token = "***ENCRYPTED***"
channel_id = "C12345"
allowed_users = ["U123"]
```

**Reference**: ZeroClaw `slack.rs` (but uses polling, we want Socket Mode)

---

### T-CH-02: Email Channel (IMAP/SMTP)

**Priority: P1 | Complexity: Medium | Est: 2-3 days**

**Why**: Universal, automation potential (digest, notifications)

**Implementation**:
- IMAP: `async-imap` or `imap` crate
- SMTP: `lettre` crate
- Polling interval: 60s default
- Subject parsing: `[homun] command` triggers agent

**Files to create**:
- `src/channels/email.rs`

**Config**:
```toml
[channels.email]
enabled = true
imap_host = "imap.gmail.com"
imap_port = 993
smtp_host = "smtp.gmail.com"
smtp_port = 587
username = "bot@example.com"
password = "***ENCRYPTED***"
poll_interval = 60
allowed_senders = ["user@example.com"]
```

**Reference**: ZeroClaw `email_channel.rs`

---

### T-CH-03: Webhook Ingress

**Priority: P1 | Complexity: Low | Est: 1 day**

**Why**: Enables external integrations, automation triggers

**Implementation**:
- HTTP endpoint: `POST /api/v1/webhook/:channel`
- Secret verification via HMAC
- Triggers agent with webhook payload as message

**Files to modify**:
- `src/web/api.rs` — add webhook endpoint
- `src/channels/webhook.rs` — new channel

**Config**:
```toml
[channels.webhook]
enabled = true
secret = "***ENCRYPTED***"
```

---

## Priority 2 — Medium Impact

### T-CH-04: Matrix Channel

**Priority: P2 | Complexity: Medium | Est: 2 days**

**Why**: Privacy-focused users, decentralized, E2EE support

**Implementation**:
- Crate: `matrix-sdk` or direct API with `reqwest`
- Long-polling `/sync` endpoint
- Room-based messaging

**Files to create**:
- `src/channels/matrix.rs`

**Config**:
```toml
[channels.matrix]
enabled = true
homeserver = "https://matrix.org"
access_token = "***ENCRYPTED***"
room_id = "!room:matrix.org"
allowed_users = ["@user:matrix.org"]
```

**Reference**: ZeroClaw `matrix.rs`

---

### T-CH-05: IRC Channel

**Priority: P2 | Complexity: Low | Est: 1 day**

**Why**: Developer communities, legacy systems

**Implementation**:
- Crate: `tokio-tungstenite` + custom IRC protocol
- TLS with SASL PLAIN support
- 512 byte message splitting

**Files to create**:
- `src/channels/irc.rs`

**Config**:
```toml
[channels.irc]
enabled = true
server = "irc.libera.chat"
port = 6697
nickname = "homun-bot"
channels = ["#homun"]
sasl_password = "***ENCRYPTED***"
```

**Reference**: ZeroClaw `irc.rs`

---

### T-CH-06: Signal Channel

**Priority: P2 | Complexity: High | Est: 3-4 days**

**Why**: Privacy-focused, growing user base

**Implementation**:
- Requires `signal-cli` (Java) as subprocess
- OR: Use `presage` crate (Rust native, experimental)
- Device linking via QR code

**Files to create**:
- `src/channels/signal.rs`

**Config**:
```toml
[channels.signal]
enabled = true
phone_number = "+1234567890"
```

**Challenge**: signal-cli dependency or presage maturity

---

## Priority 3 — Specialized

### T-CH-07: iMessage Channel

**Priority: P3 | Complexity: High | Est: 3 days**

**Why**: Apple ecosystem, but macOS only + deprecation announced

**Implementation**:
- AppleScript for sending (`osascript`)
- SQLite polling `~/Library/Messages/chat.db` for receiving
- Requires Full Disk Access permission
- **WARNING**: Apple announced deprecation — stops working June 2026

**Files to create**:
- `src/channels/imessage.rs`

**Config**:
```toml
[channels.imessage]
enabled = true
allowed_contacts = ["+1234567890"]
```

**Note**: Low priority due to deprecation

---

### T-CH-08: Mattermost Channel

**Priority: P3 | Complexity: Medium | Est: 2 days**

**Why**: Self-hosted enterprise alternative to Slack

**Implementation**:
- Webhook-based bot integration
- Similar to Slack but simpler API

**Files to create**:
- `src/channels/mattermost.rs`

---

### T-CH-09: Nostr Channel

**Priority: P3 | Complexity: Medium | Est: 2 days**

**Why**: Decentralized social, niche but growing

**Implementation**:
- Crate: `nostr` or custom
- NIP-04 encrypted direct messages
- Relay connection management

**Files to create**:
- `src/channels/nostr.rs`

---

### T-CH-10: LINE Channel

**Priority: P3 | Complexity: Medium | Est: 2 days**

**Why**: Japan/Taiwan/Thailand markets

**Implementation**:
- LINE Messaging API
- Webhook for receiving

**Files to create**:
- `src/channels/line.rs`

---

## Not Planned (Low ROI)

| Channel | Reason |
|---------|--------|
| Microsoft Teams | Enterprise only, complex Bot Framework |
| Google Chat | Workspace only, limited appeal |
| Zalo | Vietnam specific |
| Feishu/Lark | China enterprise |
| DingTalk | China enterprise |
| Twitch | Streaming only |
| QQ | China specific |
| Mobile nodes | Complex, Telegram/WhatsApp cover mobile |

---

## Implementation Order

```
Sprint 1: Slack + Email + Webhook (P1)
├── T-CH-01: Slack
├── T-CH-02: Email
└── T-CH-03: Webhook

Sprint 2: Matrix + IRC (P2)
├── T-CH-04: Matrix
└── T-CH-05: IRC

Sprint 3: Signal (P2)
└── T-CH-06: Signal

Sprint 4: Specialized (P3)
├── T-CH-07: iMessage (if still viable)
├── T-CH-08: Mattermost
└── T-CH-09: Nostr
```

---

## Channel Architecture

All channels implement the `Channel` trait:

```rust
#[async_trait]
pub trait Channel: Send + Sync {
    async fn start(&mut self, tx: mpsc::Sender<InboundMessage>) -> Result<()>;
    async fn send(&self, msg: OutboundMessage) -> Result<()>;
    fn name(&self) -> &str;
}
```

**Message flow**:
```
Channel → InboundMessage → MessageBus → AgentLoop → OutboundMessage → Channel
```

---

## Testing Strategy

For each channel:
1. **Unit tests**: Message parsing, formatting
2. **Integration tests**: Mock server/echo bot
3. **Manual testing**: Real account interaction

---

## Dependencies to Add

```toml
# Slack
slack-morphism = "2"  # OR custom with tokio-tungstenite

# Email
lettre = "0.11"
async-imap = "0.10"  # OR imap = "0.15"

# Matrix
matrix-sdk = "0.10"  # OR custom reqwest

# IRC
# Uses existing tokio-tungstenite + custom protocol

# Signal
# presage = "0.10"  # Experimental
```
