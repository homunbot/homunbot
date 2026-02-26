# Homun — Enterprise & Automation Tasks

> Last updated: 2026-02-23
> Vision: From personal assistant to multi-user enterprise platform

---

## Overview

Homun starts as a personal assistant, but the architecture should support:

1. **Multi-tenancy** — Multiple users with different permissions
2. **Marketplace** — Skills, data, and automation templates
3. **Workflow automation** — n8n/IFTTT-style visual builder
4. **Extensibility** — Plugin system for custom functionality

---

## Phase 1 — User Management & Permissions

### T-ENT-01: User System

**Priority: P1 | Complexity: High | Est: 1 week**

**Architecture**:
```
┌─────────────────────────────────────────────────────────────┐
│                      USER SYSTEM                            │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────┐     ┌─────────────┐     ┌─────────────────┐   │
│  │  User   │────▶│    Role     │────▶│   Permissions   │   │
│  │         │     │             │     │                 │   │
│  │ id      │     │ name        │     │ resource        │   │
│  │ name    │     │ priority    │     │ actions[]       │   │
│  │ email   │     │ is_default  │     │ conditions{}    │   │
│  │ status  │     │             │     │                 │   │
│  └─────────┘     └─────────────┘     └─────────────────┘   │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                    RESOURCES                         │   │
│  ├─────────────────────────────────────────────────────┤   │
│  │ agent.chat       │ Send messages to agent            │   │
│  │ agent.admin      │ Change agent settings             │   │
│  │ tools.shell      │ Execute shell commands            │   │
│  │ tools.vault      │ Access encrypted vault            │   │
│  │ skills.install   │ Install new skills                │   │
│  │ skills.execute   │ Run skill scripts                 │   │
│  │ memory.read      │ Read memory/USER.md               │   │
│  │ memory.write     │ Modify memory                     │   │
│  │ config.read      │ View configuration                │   │
│  │ config.write     │ Modify configuration              │   │
│  │ admin.users      │ Manage users                      │   │
│  │ admin.audit      │ View audit logs                   │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Default Roles**:
| Role | Permissions |
|------|-------------|
| **Admin** | All permissions |
| **User** | agent.chat, tools.*, skills.execute, memory.read |
| **Guest** | agent.chat (limited) |
| **Service** | tools.*, skills.execute (for automation) |

**Database Schema**:
```sql
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    external_id TEXT,  -- Telegram ID, Discord ID, etc.
    role_id TEXT NOT NULL,
    status TEXT DEFAULT 'active',  -- active, suspended, deleted
    created_at TEXT NOT NULL,
    last_seen_at TEXT
);

CREATE TABLE roles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    priority INTEGER DEFAULT 0,
    is_default BOOLEAN DEFAULT 0,
    permissions TEXT NOT NULL  -- JSON
);

CREATE TABLE user_channels (
    user_id TEXT NOT NULL,
    channel TEXT NOT NULL,  -- telegram, discord, etc.
    channel_user_id TEXT NOT NULL,  -- Telegram ID, etc.
    PRIMARY KEY (user_id, channel)
);
```

**Files**:
- `src/auth/mod.rs` — Auth module
- `src/auth/user.rs` — User struct and management
- `src/auth/permissions.rs` — Permission checking
- `src/storage/users.rs` — User persistence

---

### T-ENT-02: Channel Authentication

**Priority: P1 | Complexity: Medium | Est: 2-3 days**

**Problem**: Currently `allow_from` is a simple allowlist. Need proper user mapping.

**Solution**: Map channel identities to users.

**Telegram Flow**:
```
1. User sends /start to bot
2. Bot checks if Telegram ID is in allow_from
3. If new user: create guest account, link Telegram ID
4. If existing: load user, apply permissions
5. All subsequent messages are permission-checked
```

**Config**:
```toml
[auth]
enabled = true
default_role = "user"
# Auto-create users from allowed channels
auto_provision = true

[channels.telegram]
# Now supports role assignment
allow_from = [
    { id = "123456789", role = "admin" },
    { id = "987654321", role = "user" },
]
```

---

### T-ENT-03: Permission Enforcement

**Priority: P1 | Complexity: Medium | Est: 2-3 days**

**Implementation**: Middleware pattern in agent loop.

```rust
// Before executing tool
if !user.can("tools.shell") {
    return Err("Permission denied: tools.shell");
}

// Before installing skill
if !user.can("skills.install") {
    return Err("Permission denied: skills.install");
}
```

**Granular Control**:
```toml
# User-specific restrictions
[[users]]
name = "limited-user"
role = "user"

[users.restrictions]
# Can only use safe shell commands
tools.shell = { safe_only = true }
# Can only read memory, not write
memory = "read"
# Rate limited
rate_limit = { messages_per_hour = 50 }
```

---

## Phase 2 — Marketplace

### T-ENT-04: Skill Marketplace

**Priority: P2 | Complexity: High | Est: 2 weeks**

**Architecture**:
```
┌─────────────────────────────────────────────────────────────┐
│                    MARKETPLACE                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │   SKILLS    │  │    DATA     │  │   AUTOMATIONS       │ │
│  │             │  │             │  │                     │ │
│  │ • Verified  │  │ • Datasets  │  │ • Workflow templates│ │
│  │ • Community │  │ • Scraped   │  │ • Trigger templates │ │
│  │ • Paid      │  │ • Synthetic │  │ • Integration packs │ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘ │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                    VERIFICATION                       │   │
│  ├─────────────────────────────────────────────────────┤   │
│  │ • VirusTotal scan                                    │   │
│  │ • Static analysis                                    │   │
│  │ • Permission review                                  │   │
│  │ • Community ratings                                  │   │
│  │ • Homun team verification (paid option)              │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Marketplace API**:
```
GET  /api/v1/marketplace/skills          # List skills
GET  /api/v1/marketplace/skills/:id      # Skill details
POST /api/v1/marketplace/skills/:id/install

GET  /api/v1/marketplace/data            # List datasets
GET  /api/v1/marketplace/data/:id        # Dataset details
POST /api/v1/marketplace/data/:id/download

GET  /api/v1/marketplace/automations     # List automation templates
POST /api/v1/marketplace/automations/:id/install
```

---

### T-ENT-05: Data Marketplace

**Priority: P2 | Complexity: High | Est: 1 week**

**Concept**: Sell/share datasets that skills can use.

**Data Types**:
- Web-scraped data (product prices, news, etc.)
- API responses cached and structured
- User-generated datasets
- Synthetic data for testing

**Example Data Package**:
```json
{
  "id": "crypto-prices-daily",
  "name": "Cryptocurrency Daily Prices",
  "description": "Historical daily prices for top 100 cryptocurrencies",
  "format": "json",
  "schema": {
    "date": "YYYY-MM-DD",
    "symbol": "string",
    "price_usd": "float",
    "volume": "float"
  },
  "size_mb": 50,
  "updated_at": "2026-02-23",
  "price": "free",  // or "$4.99"
  "permissions": ["read", "cache"],
  "license": "CC-BY-4.0"
}
```

**Security**: Data packages are read-only, sandboxed.

---

### T-ENT-06: Monetization

**Priority: P3 | Complexity: High | Est: 2 weeks**

**Payment Options**:
- Stripe integration for paid skills/data
- Subscription model for premium features
- Revenue sharing with skill creators

**Implementation** (future):
- `src/marketplace/payments.rs`
- Webhook handlers for Stripe

---

## Phase 3 — Workflow Automation

### T-ENT-07: Visual Workflow Builder

**Priority: P2 | Complexity: Very High | Est: 4 weeks**

**Inspiration**: n8n, IFTTT, Zapier

**Architecture**:
```
┌─────────────────────────────────────────────────────────────┐
│                   WORKFLOW ENGINE                            │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  TRIGGERS                    ACTIONS                        │
│  ┌─────────────┐            ┌─────────────┐                │
│  │ Cron        │───────────▶│ Send msg    │                │
│  │ Webhook     │───────────▶│ HTTP req    │                │
│  │ Telegram    │───────────▶│ Shell cmd   │                │
│  │ Email       │───────────▶│ Agent task  │                │
│  │ File watch  │───────────▶│ Skill exec  │                │
│  │ API call    │───────────▶│ Data save   │                │
│  └─────────────┘            └─────────────┘                │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                    FLOW LOGIC                        │   │
│  ├─────────────────────────────────────────────────────┤   │
│  │ • Conditions (if/else)                              │   │
│  │ • Loops (for each)                                  │   │
│  │ • Variables ({{trigger.data}})                       │   │
│  │ • Error handling (retry, fallback)                   │   │
│  │ • Delays (wait N seconds)                            │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Workflow Definition** (YAML):
```yaml
name: "Daily Crypto Report"
description: "Send crypto prices to Telegram every morning"

trigger:
  type: cron
  schedule: "0 8 * * *"

steps:
  - id: fetch_prices
    action: http_get
    url: "https://api.coingecko.com/api/v3/simple/price"
    params:
      ids: "bitcoin,ethereum,cardano"
      vs_currencies: "usd"

  - id: format_message
    action: template
    template: |
      📊 Crypto Report - {{now | date: "%Y-%m-%d"}}

      BTC: ${{steps.fetch_prices.bitcoin.usd}}
      ETH: ${{steps.fetch_prices.ethereum.usd}}
      ADA: ${{steps.fetch_prices.cardano.usd}}

  - id: send_telegram
    action: send_message
    channel: telegram
    chat_id: "{{user.telegram_id}}"
    message: "{{steps.format_message}}"

on_error:
  action: send_message
  channel: telegram
  message: "⚠️ Crypto report failed: {{error}}"
```

**Files**:
- `src/workflow/mod.rs` — Workflow engine
- `src/workflow/triggers.rs` — Trigger handlers
- `src/workflow/actions.rs` — Action executors
- `src/workflow/parser.rs` — YAML parser
- `src/workflow/runner.rs` — Execution engine

---

### T-ENT-08: Workflow Web UI

**Priority: P2 | Complexity: High | Est: 2 weeks**

**Features**:
- Drag-and-drop node editor
- Real-time execution preview
- Debug mode with step-through
- Template library (pre-built workflows)

**Tech**:
- Frontend: React Flow or similar
- Backend: WebSocket for real-time updates

---

### T-ENT-09: Trigger System

**Priority: P2 | Complexity: Medium | Est: 1 week**

**Trigger Types**:

| Trigger | Description | Implementation |
|---------|-------------|----------------|
| **Cron** | Time-based | Existing cron system |
| **Webhook** | HTTP POST | New endpoint |
| **Telegram** | Message received | Hook into channel |
| **Email** | New email | IMAP IDLE or polling |
| **File** | File created/modified | `notify` crate |
| **Agent** | Agent response matches pattern | Hook in agent loop |
| **API** | External API call | New REST endpoint |

**Trigger Registry**:
```rust
pub trait Trigger: Send + Sync {
    fn name(&self) -> &str;
    fn subscribe(&self, callback: TriggerCallback) -> Result<()>;
    fn unsubscribe(&self) -> Result<()>;
}
```

---

## Phase 4 — Plugin System

### T-ENT-10: Plugin Architecture

**Priority: P3 | Complexity: Very High | Est: 4 weeks**

**Goal**: Allow third-party extensions without modifying core.

**Plugin Types**:
1. **Channel Plugins** — New messaging platforms
2. **Tool Plugins** — New agent tools
3. **Provider Plugins** — New LLM providers
4. **Trigger Plugins** — New automation triggers
5. **Action Plugins** — New workflow actions

**Plugin Interface**:
```rust
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn plugin_type(&self) -> PluginType;
    fn initialize(&mut self, context: PluginContext) -> Result<()>;
    fn shutdown(&mut self) -> Result<()>;
}
```

**Plugin Loading**:
- Dynamic loading via `libloading` crate
- Or WASM-based plugins (safer, more portable)

**Plugin Directory**:
```
~/.homun/plugins/
├── my-channel/
│   ├── plugin.toml
│   ├── plugin.so  # or plugin.wasm
│   └── config.toml
```

---

## Implementation Order

```
Month 1: User Management (P1)
├── T-ENT-01: User System
├── T-ENT-02: Channel Authentication
└── T-ENT-03: Permission Enforcement

Month 2: Marketplace Foundation (P2)
├── T-ENT-04: Skill Marketplace
└── T-ENT-05: Data Marketplace

Month 3: Workflow Engine (P2)
├── T-ENT-07: Visual Workflow Builder
├── T-ENT-09: Trigger System
└── T-ENT-08: Workflow Web UI

Month 4+: Advanced (P3)
├── T-ENT-06: Monetization
└── T-ENT-10: Plugin Architecture
```

---

## Database Migrations

```sql
-- Phase 1: Users
ALTER TABLE messages ADD COLUMN user_id TEXT;
CREATE TABLE users (...);
CREATE TABLE roles (...);
CREATE TABLE user_channels (...);

-- Phase 2: Marketplace
CREATE TABLE marketplace_items (...);
CREATE TABLE marketplace_purchases (...);

-- Phase 3: Workflows
CREATE TABLE workflows (...);
CREATE TABLE workflow_runs (...);
```

---

## Configuration Schema

```toml
# Enterprise features
[enterprise]
enabled = true
max_users = 100
marketplace_url = "https://marketplace.homun.dev"

[auth]
enabled = true
default_role = "user"
auto_provision = true
session_timeout_secs = 3600

[workflows]
enabled = true
max_concurrent = 10
default_timeout_secs = 300

[plugins]
enabled = true
allow_unsigned = false
allowed_sources = ["homun.dev", "internal"]
```
