# Homun — Development Roadmap

> Last updated: 2026-02-23
> Status: Phase 6+ (Web UI complete, 28K LOC, 211 tests)

---

## Current Status Summary

| Metric | Value |
|--------|-------|
| Source files | 71 |
| Lines of code | ~28,000 |
| Tests | 211 passing |
| Binary size | ~47MB (release) |
| LLM Providers | 27 |
| Channels | 5 (CLI, Telegram, Discord, WhatsApp, Web UI) |
| Tools | 11 built-in |
| Web UI pages | 8 |

---

## Priority System

- **P0 — Critical**: Security vulnerabilities, blocking issues
- **P1 — High**: Production viability, competitive features
- **P2 — Medium**: Feature parity, expansion
- **P3 — Low**: Polish, nice-to-have

---

## Task Documents

| Area | Document | Priority |
|------|----------|----------|
| Channels | `docs/TASKS_CHANNELS.md` | P1-P2 |
| Security | `docs/TASKS_SECURITY.md` | P0-P1 |
| Documentation | `docs/TASKS_DOCS.md` | P1-P2 |
| Enterprise | `docs/TASKS_ENTERPRISE.md` | P1-P3 |

---

## P0 — Critical

| Task | Status | Description |
|------|--------|-------------|
| Memory consolidation | ✅ DONE | LLM classification, dedup, vault |
| Shell sandboxing | ✅ DONE | 5-layer protection |
| Graceful shutdown | ✅ DONE | ctrl_c(), abort, grace period |
| **Vault 2FA** | ✅ DONE | TOTP authenticator, 5-min session, recovery codes |
| **Exfiltration prevention** | ❌ TODO | Block secret patterns in output |
| CI Pipeline | ❌ TODO | GitHub Actions workflow |

---

## P1 — Production & Security

### Infrastructure
| Task | Status | Description |
|------|--------|-------------|
| Rate limiting | ❌ TODO | Per-channel, per-user limits |
| Token/cost tracking | ❌ TODO | Usage per session/model |
| Service install | ❌ TODO | `homun service install` |
| Channel reconnect | ⚠️ PARTIAL | Auto-reconnect with backoff |
| Health checks | ⚠️ PARTIAL | Detailed component status |
| Retry/backoff | ⚠️ PARTIAL | Generic for all providers |

### Security
| Task | Status | Description |
|------|--------|-------------|
| **VirusTotal integration** | ❌ TODO | Scan skills before install |
| **Skill compatibility analysis** | ❌ TODO | Static analysis, risk score |
| **Skill sandbox** | ❌ TODO | Isolated execution |
| **Audit logging** | ❌ TODO | Log all skill executions |

### Channels (Phase 1)
| Task | Status | Complexity |
|------|--------|------------|
| Slack | ❌ TODO | Medium |
| Email (IMAP/SMTP) | ❌ TODO | Medium |
| Webhook ingress | ❌ TODO | Low |

### User Management
| Task | Status | Description |
|------|--------|-------------|
| User system | ❌ TODO | Users, roles, permissions |
| Channel auth | ❌ TODO | Map channel IDs to users |
| Permission enforcement | ❌ TODO | Middleware in agent loop |

---

## P2 — Feature Expansion

### Channels (Phase 2)
| Task | Status | Complexity |
|------|--------|------------|
| Matrix | ❌ TODO | Medium |
| IRC | ❌ TODO | Low |
| Signal | ❌ TODO | High |

### Tools
| Task | Status | Description |
|------|--------|-------------|
| Browser automation (CDP) | ❌ TODO | Killer feature |
| Git tool | ❌ TODO | Safe git operations |
| Screenshot/image analysis | ❌ TODO | Vision capabilities |

### Distribution
| Task | Status | Description |
|------|--------|-------------|
| Pre-built binaries | ❌ TODO | GitHub Releases |
| Docker image | ❌ TODO | Multi-arch |
| Homebrew | ❌ TODO | `brew install homun` |
| Tunnel support | ❌ TODO | CF/Tailscale/ngrok |

### Documentation
| Task | Status | Description |
|------|--------|-------------|
| Landing page | ❌ TODO | homun.dev |
| Documentation site | ❌ TODO | docs.homun.dev |
| API reference | ❌ TODO | OpenAPI spec |
| Video tutorials | ❌ TODO | Quick start, setup |

### Marketplace
| Task | Status | Description |
|------|--------|-------------|
| Skill marketplace | ❌ TODO | Verified/community skills |
| Data marketplace | ❌ TODO | Datasets for skills |

### Workflows
| Task | Status | Description |
|------|--------|-------------|
| Workflow engine | ❌ TODO | n8n/IFTTT-style |
| Trigger system | ❌ TODO | Cron, webhook, events |
| Visual builder | ❌ TODO | Drag-drop UI |

---

## P3 — Polish

| Task | Status | Priority |
|------|--------|----------|
| Config wizard | ❌ TODO | Medium |
| Skill creator | ❌ TODO | Medium |
| Bundled skills (10) | ⚠️ 4 done | Medium |
| Voice transcription | ❌ TODO | Low |
| Mobile app | ❌ TODO | Low |
| Monetization | ❌ TODO | Low |
| Plugin system | ❌ TODO | Low |

---

## Phase Timeline

```
Phase 7 — Security & Stability (Current)
├── P0: Vault 2FA, Exfiltration prevention, CI
└── P1: Rate limiting, Cost tracking, Service install

Phase 8 — Channels (Q2 2026)
├── P1: Slack, Email, Webhook
└── P2: Matrix, IRC, Signal

Phase 9 — User & Marketplace (Q3 2026)
├── P1: User system, Permissions
├── P1: VirusTotal, Skill sandbox
└── P2: Skill/Data marketplace

Phase 10 — Workflows (Q4 2026)
├── P2: Workflow engine
├── P2: Trigger system
└── P2: Visual builder

Phase 11 — Distribution (Q1 2027)
├── P2: Pre-built binaries
├── P2: Docker, Homebrew
└── P2: Documentation site
```

---

## Quick Links

- **Competitor Analysis**: `docs/competitors/COMPARISON.md`
- **Status**: `docs/status.md`
- **Memory Architecture**: `docs/architecture/memory.md`
- **Security Architecture**: `docs/architecture/security.md`

---

## What Makes Homun Unique

1. **Web UI embedded** — 8 pages, zero frontend build
2. **27 LLM providers** — native tool calling, no LiteLLM
3. **Agent Skills standard** — open spec, not proprietary
4. **WhatsApp native Rust** — no Node.js bridge
5. **Local embeddings** — fastembed ONNX, no API calls
6. **Security-first** — 2FA, sandboxing, exfiltration prevention
