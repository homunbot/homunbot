# Homun — Status Summary

> Generato: 17 marzo 2026
> Scopo: quadro completo di cosa esiste, cosa manca, e dove concentrare il lavoro.

---

## Metriche

| Metrica | Valore |
|---------|--------|
| LOC Rust | ~87,400 |
| LOC Frontend | ~19K JS + ~10K CSS |
| Test | 646 passing (1 flaky pre-esistente) |
| File sorgente | 130+ |
| Provider LLM | 14 |
| Canali | 7 (CLI, Telegram, Discord, WhatsApp, Slack, Email, Web) |
| Tool built-in | ~20 |
| Pagine Web UI | 20 |
| Migrazioni SQLite | 18 |
| Binary release | ~35MB |

---

## Cosa e' FATTO

### Core Agent (Sprint 1-3) — ~1,130 LOC

- **Provider failover** con circuit breaker, EMA latency, auto-skip su >80% errori
- **Session compaction** via LLM summarization + token counting
- **Hybrid memory search**: HNSW vector + FTS5 keyword + RRF merge + temporal decay
- **Embedding provider pluggable**: Ollama, OpenAI, Mistral, Cohere, Together, Fireworks
- **IndexMeta sidecar**: tracking provider/model/dimensions, mismatch detection, rebuild in-place
- **DM pairing** con OTP (5 min TTL, 3 tentativi)
- **Mention gating** per gruppi

### Web UI + Automations (Sprint 4) — ~3,200 LOC

- **Automations Builder v2**: canvas SVG n8n-style, 13 tipi nodo, NLP generation
- **Schema-driven forms**: JSON Schema -> form fields con smart API overrides
- **Validazione real-time**: 3 livelli (field/node/flow), `auto-validate.js`
- **6 template** automazioni preconfigurate
- **Dashboard operativa**: stat cards, automazioni upcoming, activity feed, health grid, usage analytics
- **Real-time logs** via SSE streaming

### Ecosistema (Sprint 5) — ~1,350 LOC

- **MCP client nativo**: catalog (Official Registry + MCPMarket + presets), install guidato
- **OAuth flows**: Google Workspace, GitHub, Notion (PKCE + Dynamic Client Registration)
- **Token auto-refresh**: Google + Notion con vault persistence
- **MCP hot-reload**: tool disponibili subito senza restart
- **Skill ecosystem**: loader, creator, adapter (ClawHub), security scanner, audit logging
- **Skill Runtime Parity**: eligibility, invocation policy, tool restriction, env injection (SKL-1..7)

### RAG Knowledge Base (Sprint 6) — ~2,830 LOC

- **Pipeline completa**: upload -> SHA-256 dedup -> chunk -> embed -> HNSW + FTS5
- **30+ formati** (8 con parsing dedicato, resto plain text fallback)
- **Sensitive data gating**: classificazione automatica, redazione, 2FA gate
- **Directory watcher**: auto-ingest su file nuovo/modificato
- **Web UI** `/knowledge`: drag&drop upload, search con attribuzione

### Workflow Engine — ~2,310 LOC

- **Persistent multi-step** con DB, approval gates, retry, resume-on-boot
- **Web UI** `/workflows`: timeline, approve/cancel, stats

### Sandbox — ~2,400 LOC

- **4 backend**: Docker, macOS Seatbelt, Linux Bubblewrap, Windows Job Objects
- **Auto-detection** del miglior backend disponibile
- **Always-On** di default su tutti gli OS
- **Runtime image lifecycle**: policy, drift tracking, build/pull/inspect
- **CI cross-platform**: 5 job (linux-native, runtime-image, e2e-linux, e2e-windows, e2e-macos)

### Browser Automation

- **17 azioni** via MCP Playwright (`@playwright/mcp`)
- **Stealth anti-bot**: `addInitScript` con navigator.webdriver patching
- **Snapshot compaction**: tree-preserving, keeps refs + content roles
- **Task planning**: 4 classi task, 10+ veto rules, 22 unit test
- **Tab isolation**: per-conversation via TabSessionManager

### Security — ~810 LOC

- **Auth**: PBKDF2-HMAC-SHA256 (600k iter), session cookies HMAC-signed
- **HTTPS nativo**: rustls, auto-cert 10yr, custom domain
- **Rate limiting**: auth 5/min, API 60/min, per-IP
- **2FA/TOTP**: RFC 6238, QR setup, recovery codes, 5-min lockout
- **Vault**: AES-256-GCM, OS keychain master key, zeroized memory
- **Exfiltration guard**: 20+ pattern, vault leak filter (con bypass per retrieve espliciti)
- **Instruction boundary**: trust boundaries nel system prompt (SEC-6..9)
- **E-Stop**: kill switch per agent/network/browser/MCP

### Business Autopilot (BIZ-1) — ~2,030 LOC

- **Core engine**: OODA loop, budget enforcement, autonomy levels
- **13 tool actions**, 6 tabelle DB, 10 REST endpoints
- **Web UI** `/business`

### Canali (Sprint 7) — ~478 LOC

- 7 canali funzionanti: CLI, Telegram, Discord, WhatsApp, Slack, Email, Web
- Telegram + Email = production-ready
- Discord/WhatsApp/Slack = funzionanti ma da hardening

### Hardening (Sprint 8) — ~890 LOC

- CI pipeline (11 check), tool timeout, provider health, E-Stop, service install, DB maintenance

### Release Infrastructure (parziale)

- **Dockerfile** multi-stage (~100MB) con Ollama sidecar opzionale
- **docker-compose** con Caddy reverse proxy + HTTPS auto
- **Embedding Settings UI**: filtro modelli, auto-pull, mismatch detection + rebuild

---

## Cosa MANCA — Priorita'

### P0 — Blockers per Alpha (v0.2)

| Task | Descrizione | Effort |
|------|-------------|--------|
| ~~**REL-5**~~ | ~~Health check `/health/components`~~ | ✅ DONE |
| ~~**REL-9**~~ | ~~Fix flaky CI test (`test_safe_python_version`)~~ | ✅ DONE |
| ~~**REL-8**~~ | ~~Graceful shutdown completo~~ | ✅ DONE |
| **REL-6** | README utente (non dev-oriented, quick start Docker) | 2 giorni |
| **REL-7** | Getting Started guide step-by-step con screenshot | 3 giorni |
| **REL-10** | Error states UI (toast/notification system globale) | 3 giorni |
| **REL-11** | Pre-built binaries (GitHub Actions: linux/mac/win x64+arm64) | 2 giorni |
| **REL-12** | CHANGELOG (Keep a Changelog format) | 1 giorno |

**Totale Alpha rimanente: ~11 giorni lavoro, 5 task**

### P1 — Consolidamento (pre-Beta)

| Task | Descrizione | Effort |
|------|-------------|--------|
| **AUD-2** | Documentare feature gating (embeddings non nel build default) | 1 giorno |
| **AUD-5** | Test integrazione RAG pipeline (ingest->search round-trip) | 1 giorno |
| **AUD-12** | Fix sandbox tests flaky (PoisonError, serve `#[serial]`) | 1 giorno |
| **AUTO-4** | Wizard step-by-step per automazioni semplici | 3 giorni |
| **CHAT-7** | Formalizzare E2E test suite (release-grade) | 3 giorni |
| **SEC-11** | RAG document injection detection | 2 giorni |
| **SEC-12** | Skill body injection scan | 1 giorno |

### P2 — Channel Hardening

| Task | Descrizione | Canale |
|------|-------------|--------|
| Proactive messaging | `default_channel_id` esiste ma inutilizzato | Discord |
| Events API | Sostituire polling 3s (6s latency) | Slack |
| Attachment support | Zero attachment inbound/outbound | Slack |
| Re-pairing da gateway | Solo TUI pairing attualmente | WhatsApp |
| Circuit breaker + reconnect | Monitoraggio connessione | Tutti |

### P3 — Espansione (post-Beta)

| Area | Descrizione |
|------|-------------|
| **BIZ-2..5** | Payments, Accounting, Marketing, Crypto |
| **Mobile App** | Flutter: pairing, chat, push, vault, dashboard (~2,600 LOC) |
| **Website** | Landing page, docs site, API reference, SEO |
| **Localization** | i18n: EN base, IT completo |
| **Auto-update** | Sparkle/WinSparkle + update server |
| **Voice/telephony** | Pipeline voce (non pianificato in dettaglio) |

---

## Documenti di Tracking

| File | Scopo | Aggiornamento |
|------|-------|---------------|
| `docs/ROADMAP.md` | Piano sprint con task, LOC, date | Attivo (1,940 righe) |
| `docs/IMPLEMENTATION-GAPS.md` | Gap operativi reali dal deep audit | Attivo (616 righe) |
| `docs/TASKS_DOCS.md` | Task documentazione + website | Feb 2026 (alto livello) |
| `docs/TASKS_CHANNELS.md` | Task canali futuri (Matrix, IRC, Signal...) | Feb 2026 (alto livello) |
| `docs/TASKS_ENTERPRISE.md` | Task enterprise (multi-user, marketplace) | Feb 2026 (alto livello) |
| `docs/TASKS_SECURITY.md` | Task security avanzata (VirusTotal, sandbox) | Feb 2026 (alto livello) |

**Nota**: ROADMAP + IMPLEMENTATION-GAPS sono i documenti operativi attivi.
I TASKS_*.md sono task di alto livello datati febbraio 2026, utili come riferimento
ma non aggiornati operativamente.

---

## Timeline Stimata

```
Oggi (Mar 2026) ──── Alpha v0.2 (~Mag 2026) ──── Beta v0.5 (~Set 2026) ──── v1.0 (~Gen 2027)
                     │                             │                          │
                     ├─ Docker + docs              ├─ Installer nativi        ├─ Website
                     ├─ Health checks              ├─ Setup wizard v2         ├─ Mobile app
                     ├─ Graceful shutdown           ├─ Channel hardening       ├─ Docs site
                     ├─ Pre-built binaries          ├─ UX overhaul            ├─ Localization
                     ├─ Error states UI             ├─ E2E Playwright CI      ├─ Auto-update
                     └─ README + Getting Started    └─ Ollama local flow      └─ Observability
```

---

## Vantaggi Competitivi vs OpenClaw/ZeroClaw

1. **Single binary Rust** (~35MB, no runtime Node.js/Python)
2. **MCP client integrato nativamente** (OpenClaw non ce l'ha)
3. **RAG Knowledge Base personale** con hybrid search
4. **Browser automation via MCP** con stealth + compact snapshots
5. **Exfiltration filter** (OpenClaw manca)
6. **Workflow Engine persistente** (piu' potente di Lobster)
7. **Visual automations builder** n8n-style con NLP
8. **Sandbox unificato** cross-platform (4 backend)
9. **Design system proprietario** "Olive Moss Console"
10. **Skill ecosystem** con ClawHub + OpenSkills + hot-reload
