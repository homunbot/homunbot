# Homun — Documentation & Website Tasks

> Last updated: 2026-02-23
> Goal: Professional documentation site + landing page

---

## Overview

Homun needs a public presence to:
1. **Attract users** — landing page with clear value proposition
2. **Onboard users** — installation guide, quick start
3. **Reference docs** — API, configuration, tools, skills
4. **Build trust** — security practices, comparison with competitors

---

## P1 — Website

### T-WEB-01: Landing Page

**Priority: P1 | Complexity: Medium | Est: 2-3 days**

**URL**: https://homun.dev (or homunbot.github.io initially)

**Sections**:
1. **Hero** — "The digital homunculus that lives in your computer"
   - Tagline: Personal AI assistant, single binary, privacy-first
   - CTA: Download, Quick Start, GitHub
2. **Features** — Grid of key features
   - 27 LLM providers
   - 8-page Web UI
   - Agent Skills standard
   - WhatsApp native Rust
   - Local embeddings
3. **Comparison** — vs OpenClaw, ZeroClaw (link to full comparison)
4. **Quick Start** — 3 commands to get running
5. **Screenshots** — Web UI, TUI dashboard
6. **Security** — Our approach (link to security docs)

**Tech Stack**:
- Static site: Hugo, Zola, or plain HTML
- Deploy: GitHub Pages or Cloudflare Pages
- Domain: homun.dev (~$12/year)

**Files**:
- `site/` directory in repo
- `CNAME` for custom domain

---

### T-WEB-02: Documentation Site

**Priority: P1 | Complexity: High | Est: 1 week**

**URL**: https://docs.homun.dev (or homun.dev/docs)

**Structure**:
```
docs/
├── Getting Started
│   ├── Installation
│   ├── Quick Start
│   ├── Configuration
│   └── First Steps
│
├── Channels
│   ├── CLI
│   ├── Web UI
│   ├── Telegram
│   ├── Discord
│   ├── WhatsApp
│   ├── Slack (TODO)
│   └── Email (TODO)
│
├── Providers
│   ├── Overview (27 providers)
│   ├── Anthropic
│   ├── OpenAI
│   ├── Ollama
│   ├── OpenRouter
│   └── Custom Endpoint
│
├── Tools
│   ├── Shell (with safety)
│   ├── File Operations
│   ├── Web Search
│   ├── Cron Scheduler
│   ├── Vault (Secrets)
│   └── MCP Servers
│
├── Skills
│   ├── What are Skills
│   ├── Installing Skills
│   ├── Creating Skills
│   ├── Skill Security
│   └── ClawHub Marketplace
│
├── Memory
│   ├── How Memory Works
│   ├── USER.md Format
│   ├── Memory Consolidation
│   └── Vector Search
│
├── Security
│   ├── Vault & Encryption
│   ├── Shell Sandboxing
│   ├── Skill Verification
│   ├── 2FA for Vault
│   └── Best Practices
│
├── Advanced
│   ├── Web UI Setup
│   ├── REST API
│   ├── WebSocket Streaming
│   ├── Service Install
│   └── Tuning Parameters
│
└── Reference
    ├── CLI Commands
    ├── Configuration Schema
    ├── API Reference
    └── Changelog
```

**Tech Stack**:
- Generator: **MkDocs** (Python) or **Zola** (Rust)
- Theme: Material (MkDocs) or custom
- Search: Built-in or Algolia

**Automation**:
- Generate CLI reference from `clap` args
- Generate config schema from Rust structs
- Auto-deploy on merge to main

---

### T-WEB-03: API Reference

**Priority: P1 | Complexity: Medium | Est: 2 days**

**URL**: https://docs.homun.dev/api

**Endpoints**:
```
# Chat
POST /api/v1/chat              # Send message, get response
GET  /api/v1/sessions          # List sessions
GET  /api/v1/sessions/:id      # Get session history
DELETE /api/v1/sessions/:id    # Delete session

# Skills
GET    /api/v1/skills          # List installed skills
POST   /api/v1/skills/install  # Install skill
DELETE /api/v1/skills/:name    # Remove skill
GET    /api/v1/skills/search   # Search ClawHub

# Config
GET   /api/v1/config           # Get config (no secrets)
PATCH /api/v1/config           # Update config

# Memory
GET  /api/v1/memory/status     # Memory statistics
GET  /api/v1/memory/search     # Search memories
POST /api/v1/memory/consolidate # Trigger consolidation

# Vault
GET  /api/v1/vault/keys        # List keys (not values)
GET  /api/v1/vault/:key        # Get value (requires 2FA)
POST /api/v1/vault/:key        # Store value
DELETE /api/v1/vault/:key      # Delete value

# System
GET /api/v1/status             # System status
GET /api/v1/health             # Health check
GET /api/v1/metrics            # Prometheus metrics
```

**Format**: OpenAPI 3.0 spec, auto-generate from code with `utoipa`

---

## P2 — Video Tutorials

### T-VID-01: Quick Start Video

**Priority: P2 | Complexity: Low | Est: 1 day**

5-minute video: "Get Homun running in 5 minutes"

1. Download binary
2. Run `homun gateway`
3. Open Web UI
4. Configure provider
5. Send first message

---

### T-VID-02: WhatsApp Setup Video

**Priority: P2 | Complexity: Low | Est: 1 day**

3-minute video: "Connect WhatsApp to Homun"

1. Run `homun config`
2. Go to WhatsApp tab
3. Press `p` to pair
4. Enter code on phone
5. Test message

---

### T-VID-03: Skill Installation Video

**Priority: P2 | Complexity: Low | Est: 1 day**

5-minute video: "Install and use skills"

1. Browse ClawHub
2. Install skill
3. Security scan
4. Use skill
5. Disable/remove

---

## P3 — Community

### T-COM-01: Discord Server

**Priority: P3 | Complexity: Low | Est: 1 day**

Create Discord server for:
- User support
- Feature requests
- Announcements
- Community skills sharing

---

### T-COM-02: GitHub Discussions

**Priority: P3 | Complexity: Low | Est: 1 day**

Enable GitHub Discussions for Q&A.

---

### T-COM-03: Contributing Guide

**Priority: P3 | Complexity: Low | Est: 1 day**

`CONTRIBUTING.md` with:
- Code style guide
- PR process
- Testing requirements
- Skill contribution guidelines

---

## Implementation Order

```
Week 1: Core Docs
├── T-WEB-01: Landing Page
└── T-WEB-02: Documentation Site (Getting Started + Channels)

Week 2: Reference Docs
├── T-WEB-02: Documentation Site (Providers + Tools + Skills)
└── T-WEB-03: API Reference

Week 3: Advanced & Polish
├── T-WEB-02: Documentation Site (Memory + Security + Advanced)
├── T-VID-01: Quick Start Video
└── T-COM-01/02/03: Community setup

Week 4: Video Content
├── T-VID-02: WhatsApp Video
└── T-VID-03: Skill Video
```

---

## Tech Stack Decision

| Option | Pros | Cons |
|--------|------|------|
| **MkDocs + Material** | Great search, auto-nav, popular | Python dep |
| **Zola** | Rust native, fast, single binary | Smaller ecosystem |
| **Docusaurus** | React-based, feature-rich | Node.js dep |
| **Plain HTML** | Zero deps, full control | Manual everything |

**Recommendation**: **Zola** — Rust native, aligns with project philosophy

---

## Hosting Options

| Option | Cost | Setup |
|--------|------|-------|
| GitHub Pages | Free | Push to gh-pages branch |
| Cloudflare Pages | Free | Connect to GitHub |
| Netlify | Free tier | Connect to GitHub |
| VPS | $5/mo | Full control |

**Recommendation**: **Cloudflare Pages** — Fast, free, good analytics

---

## Domain

- **homun.dev** — Primary choice (~$12/year)
- **homunbot.org** — Alternative
- **gethomun.com** — Marketing-focused

---

## Files to Create

```
site/
├── config.toml           # Zola config
├── content/
│   ├── _index.md         # Landing page
│   ├── docs/
│   │   ├── _index.md
│   │   ├── getting-started/
│   │   ├── channels/
│   │   ├── providers/
│   │   ├── tools/
│   │   ├── skills/
│   │   ├── memory/
│   │   ├── security/
│   │   ├── advanced/
│   │   └── reference/
│   └── blog/
│       └── _index.md
├── templates/
│   ├── base.html
│   ├── page.html
│   ├── docs.html
│   └── partials/
├── static/
│   ├── css/
│   ├── js/
│   └── images/
│       └── screenshots/
└── themes/
    └── (or use built-in)
```
