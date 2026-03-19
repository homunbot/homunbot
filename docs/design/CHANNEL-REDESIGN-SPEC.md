# Channel Redesign: Unified Auth, Identity & Response Persona

> Status: DRAFT — 2026-03-18
> Scope: Auth unification, per-channel persona, MCP channel support, memory integration

## Problem Statement

The current channel system has grown organically and has inconsistencies:
1. **Auth is inconsistent**: Telegram/Discord are fail-open, WhatsApp/Slack are fail-closed
2. **No persona control**: Agent always responds as "Homun bot" — can't impersonate user or company
3. **Memory is global**: No per-contact conversation separation
4. **MCP channels not supported**: No way to add channels via MCP servers
5. **Response mode priority is unclear**: Contact vs channel vs global — which wins?

## Design Goals

1. **Uniform auth**: All channels use the same authorization logic (contact-based + config allow_from)
2. **Per-channel persona**: Define HOW the agent responds (as bot, as user, as company)
3. **Per-contact override**: Contact settings always win over channel defaults
4. **MCP channel parity**: MCP-added channels go through the same gateway pipeline
5. **Session-aware memory**: Conversations with each contact are contextually separate

---

## 1. Unified Authorization Model

### Current State
```
Channel Level:  allow_from (static list) → varies by channel (fail-open vs fail-closed)
Gateway Level:  pairing_config → OTP pairing for unknown senders
Contact Level:  contact_identities → auto-authorize known contacts
```

### New Model: Contact-First Authorization

```
                    ┌─────────────────────────────────────┐
                    │         Authorization Pipeline        │
                    │                                       │
  Inbound ──────►  │  1. Is sender a known contact?        │
  Message          │     YES → authorized (DB lookup)       │
                    │     NO  → continue                    │
                    │                                       │
                    │  2. Is sender in channel allow_from?  │
                    │     YES → authorized                  │
                    │     NO  → continue                    │
                    │                                       │
                    │  3. Is pairing enabled?               │
                    │     YES → OTP flow                    │
                    │     NO  → REJECT (fail-closed)       │
                    └─────────────────────────────────────┘
```

**Key change**: ALL channels become **fail-closed** by default. Contacts are always authorized. `allow_from` is the escape hatch for non-contact senders.

### Implementation

Move authorization OUT of individual channels and INTO the gateway. Channels only handle transport — they forward ALL messages to the gateway, which decides authorization.

```rust
// New: channels/traits.rs
pub trait Channel: Send + Sync {
    fn name(&self) -> &str;
    async fn start(&self, inbound_tx: Sender<InboundMessage>, outbound_rx: Receiver<OutboundMessage>) -> Result<()>;
    // Channels no longer filter — gateway handles auth
}
```

Remove `allow_from` checks from: `whatsapp.rs`, `telegram.rs`, `discord.rs`, `slack.rs`, `email.rs`.
Add unified auth check in `gateway.rs` routing loop (after receiving InboundMessage, before contact resolution).

---

## 2. Per-Channel Persona

### Concept

Each channel has a `persona` that defines how the agent presents itself:

```toml
[channels.whatsapp]
enabled = true
persona = "owner"  # "bot" | "owner" | "company" | "custom"

[channels.telegram]
enabled = true
persona = "bot"  # Default: responds as Homun

[channels.slack]
enabled = true
persona = "company"
company_name = "AcmeCorp"
company_role = "AI Assistant"

# Custom persona with full control
[channels.email.accounts.lavoro]
persona = "custom"
persona_name = "Fabio Cancellieri"
persona_instructions = "Respond as if you are Fabio. Use first person. Sign emails with 'Fabio'."
```

### Persona Types

| Persona | System Prompt Injection | Example |
|---------|------------------------|---------|
| `bot` (default) | "You are Homun, a personal AI assistant." + SOUL.md | Bot responds as itself |
| `owner` | "You are responding on behalf of {user_name}. Write in first person as if you are them." + USER.md style | Agent impersonates the owner |
| `company` | "You are {company_name}'s {company_role}. Respond professionally on behalf of the company." | Corporate assistant |
| `custom` | User-defined `persona_instructions` injected into system prompt | Full control |

### Per-Channel Tone of Voice

Each channel can also define a default `tone_of_voice`, same as contacts:

```toml
[channels.whatsapp]
persona = "owner"
tone_of_voice = "informal, friendly, use Italian"

[channels.slack]
persona = "company"
tone_of_voice = "professional, concise, English only"

[channels.emails.lavoro]
persona = "owner"
tone_of_voice = "formal, professional Italian"
```

**Priority chain** (same pattern for all settings):
```
Effective tone = Contact.tone_of_voice   (if non-empty)
                 ?? Channel.tone_of_voice (if non-empty)
                 ?? ""                    (no tone constraint)
```

This means: a contact's tone always wins. If a contact has no tone set, the channel's default tone applies. If neither has a tone, the agent uses its natural style (SOUL.md).

### Per-Contact Override

```rust
// contacts table: new field
persona_override: Option<String>  // "bot" | "owner" | "company" | "custom"
persona_instructions: Option<String>  // Custom instructions for this contact

// Priority chain:
// 1. Contact.persona_override (if set)
// 2. Channel.persona (from config)
// 3. Global default: "bot"
```

### Implementation

Add `persona` field to channel configs in `schema.rs`.
Add `persona_override` + `persona_instructions` to `Contact` struct.
In `agent_loop.rs` / `context.rs`: resolve persona from contact → channel → default, inject appropriate system prompt prefix.

---

## 3. Response Mode Clarification

### Current Confusion
- `response_mode` exists on both Channel config AND Contact
- Priority: Contact > Channel > "automatic"
- But this isn't documented or obvious to users

### New Model: Explicit Priority Chain

```
Effective Response Mode = Contact.response_mode    ?? Channel.response_mode    ?? "automatic"
Effective Persona       = Contact.persona_override  ?? Channel.persona          ?? "bot"
Effective Tone          = Contact.tone_of_voice     ?? Channel.tone_of_voice    ?? ""
```

Both are resolved in the gateway's contact resolution step and passed to the agent.

### Response Mode Values (unchanged)
- `automatic`: Agent responds immediately
- `assisted`: Agent drafts response, owner approves
- `on_demand`: Message saved, no agent processing unless requested
- `silent`: Message dropped entirely

---

## 4. MCP Channel Support

### Concept

MCP servers can register as channels. They go through the same gateway pipeline as native channels.

```toml
[channels.mcp."custom-sms"]
server = "sms-gateway"  # MCP server name
persona = "owner"
response_mode = "automatic"
allow_from = ["*"]  # Or specific identifiers
```

### Implementation

MCP channels register via the existing MCP tool system. They produce `InboundMessage` and consume `OutboundMessage` — same as native channels. The gateway doesn't need to know if a channel is native or MCP.

```rust
// New: channels/mcp_channel.rs
pub struct McpChannel {
    server_name: String,
    config: McpChannelConfig,
}

impl Channel for McpChannel {
    fn name(&self) -> &str { &self.server_name }
    async fn start(&self, inbound_tx, outbound_rx) -> Result<()> {
        // Bridge MCP server events to InboundMessage
        // Bridge OutboundMessage to MCP server calls
    }
}
```

---

## 5. Memory Integration

### Current State
- Memory is global (all contacts share the same memory pool)
- Session key is `channel:chat_id` — different per contact
- But memory search doesn't filter by contact

### Phase 1: Contact-Scoped Session Context (minimal change)
- Session history is already per-contact (via session_key)
- Add contact_id to memory chunks for future filtering
- Memory search remains global but contact context provides personalization

### Phase 2 (future): Contact-Filtered Memory Search
- `memory_search.rs`: add optional `contact_id` filter
- When responding to a contact, prioritize memories from conversations with that contact
- Still include global memories (shared knowledge) with lower weight

---

## 6. Config Schema Changes

```toml
# Channel-level additions
[channels.whatsapp]
enabled = true
persona = "owner"                          # NEW: how agent presents itself
tone_of_voice = "informal, friendly"       # NEW: default tone for this channel
# response_mode = "automatic"              # Already exists
# allow_from = [...]                       # Already exists, but now just fallback

[channels.telegram]
enabled = true
persona = "bot"
tone_of_voice = ""                         # Empty = use SOUL.md default

# Per-email-account persona + tone
[channels.emails.lavoro]
persona = "owner"
persona_name = "Fabio Cancellieri"
tone_of_voice = "formal, professional Italian"

[channels.emails.azienda]
persona = "company"
company_name = "AcmeCorp"
tone_of_voice = "professional, concise, English"
```

### Contact Schema Additions (migration 023)
```sql
ALTER TABLE contacts ADD COLUMN persona_override TEXT DEFAULT '';
ALTER TABLE contacts ADD COLUMN persona_instructions TEXT DEFAULT '';
```

---

## 7. Implementation Phases

### Phase A: Auth Unification (1 session)
1. Move auth checks from channels to gateway
2. All channels become transport-only (forward everything)
3. Gateway does: contact DB check → allow_from check → pairing check → reject
4. All channels become fail-closed

### Phase B: Persona System (1 session)
1. Add `persona` to channel configs
2. Add `persona_override` + `persona_instructions` to contacts
3. Migration 023
4. Resolve persona in gateway, inject into agent context
5. Update system prompt builder

### Phase C: MCP Channel Support (1 session)
1. `channels/mcp_channel.rs`: MCP → InboundMessage bridge
2. Config schema for MCP channels
3. Gateway: start MCP channels alongside native ones
4. Same auth + persona + response_mode pipeline

### Phase D: Memory Scoping (1 session)
1. Add `contact_id` to memory chunks
2. Filter memory search by contact when available
3. Keep global fallback for shared knowledge

---

## 8. Migration Checklist

- [ ] Phase A: Auth unification
  - [ ] Remove allow_from from whatsapp.rs SAFETY CHECK 4
  - [ ] Remove allow_from from telegram.rs
  - [ ] Remove allow_from from discord.rs
  - [ ] Remove is_user_allowed from slack.rs
  - [ ] Remove is_sender_allowed from email.rs
  - [ ] Add unified auth in gateway routing loop
  - [ ] All channels: fail-closed by default

- [ ] Phase B: Persona system
  - [ ] Migration 023: persona fields on contacts
  - [ ] Add persona to channel configs (schema.rs)
  - [ ] Persona resolver: contact > channel > "bot"
  - [ ] System prompt persona injection
  - [ ] UI: persona selector in channel settings + contact edit

- [ ] Phase C: MCP channels
  - [ ] McpChannel struct implementing Channel trait
  - [ ] Config schema for MCP channel registration
  - [ ] Gateway: start MCP channels

- [ ] Phase D: Memory scoping
  - [ ] Add contact_id to memory chunks
  - [ ] Filter memory_search by contact_id
  - [ ] UI: per-contact conversation history view

---

## Open Questions

1. **Persona in groups**: In group chats (Telegram, Discord, WhatsApp), should persona be "bot" always? Or should it match the channel persona?
2. **Multi-persona per channel**: Should a single channel support different personas for different contacts? (Current design: yes, via contact override)
3. **Persona in proactive messages**: When Homun sends proactive messages (heartbeat, cron), which persona should it use?
4. **Memory isolation vs sharing**: Should contacts in "owner" persona see the same memory as "bot" persona? Probably yes — the persona changes presentation, not knowledge.
