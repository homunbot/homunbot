# Homun — Development Roadmap

> Last updated: 2026-03-03
> Basato su: Audit completo (`docs/AUDIT-2026-03.md`)
> Gap analysis: Homun vs OpenClaw vs ZeroClaw

---

## Status Attuale

| Metrica | Valore |
|---------|--------|
| LOC Rust | ~41,343 |
| LOC Frontend | ~8,691 |
| Test | 348 passing |
| Binary (full) | ~50MB |
| Provider LLM | 14 |
| Canali | 6 (CLI, Telegram, Discord*, WhatsApp*, Slack*, Email*) |
| Tool built-in | 11 |
| Pagine Web UI | 10+ |
| Feature flags | 12 |

*\* = parziale/stub*

---

## Priorita

- **P0 — Critico**: Affidabilita e robustezza in produzione
- **P1 — Alto**: Feature competitive, production viability
- **P2 — Medio**: Feature parity, espansione
- **P3 — Basso**: Polish, nice-to-have

---

## Sprint 1 — Robustezza Agent (P0)

> Obiettivo: rendere l'agent loop affidabile per uso quotidiano

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| 1.1 | **Provider failover** | `provider/reliable.rs`, `provider/factory.rs` | ~296 | ✅ DONE |
| | Multi auth profiles per provider | | | |
| | Round-robin + "last good" tracking | | | |
| | Cooldown su errori (backoff per profile) | | | |
| | Fallback automatico al prossimo provider | | | |
| 1.2 | **Session compaction** | `agent/memory.rs`, `storage/db.rs` | ~170 | ✅ DONE |
| | Trigger su threshold (es. >50 messaggi) | | | |
| | LLM summarization dei messaggi vecchi | | | |
| | Preserva: system prompt + ultimi N + summary | | | |
| | Fallback: truncation se summary fallisce | | | |
| 1.3 | **Token counting** | `storage/db.rs`, `agent/agent_loop.rs`, `web/api.rs` | ~128 | ✅ DONE |
| | Estrarre usage.input/output_tokens dalle risposte | | | |
| | Salvare in DB per session/model | | | |
| | Esporre via API GET /api/v1/usage | | | |

**Sprint 1 completo: ~594 LOC**

---

## Sprint 2 — Memory Search Attiva (P1)

> Obiettivo: le memorie vengono cercate e iniettate ad ogni conversazione

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| 2.1 | **Attivare hybrid search nel loop** | `agent/agent_loop.rs` | ~50 | TODO |
| | Prima di ogni chiamata LLM: cercare memorie rilevanti | | | |
| | Iniettare come "Relevant memories" nel context | | | |
| | Usare query = ultimi messaggi utente | | | |
| 2.2 | **Embedding API provider** | `agent/embeddings.rs` | ~100 | TODO |
| | Supporto OpenAI text-embedding-3-small via API | | | |
| | Fallback su fastembed locale se non configurato | | | |
| | Cache LRU per evitare chiamate duplicate | | | |
| 2.3 | **Web UI: memory search** | `static/js/memory.js`, `web/api.rs` | ~80 | TODO |
| | Endpoint GET /api/v1/memory/search?q=... | | | |
| | UI per testare search e vedere risultati con score | | | |

**Stima totale Sprint 2: ~230 LOC**

---

## Sprint 3 — Sicurezza Canali (P1)

> Obiettivo: sicurezza base per uso multi-utente

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| 3.1 | **DM Pairing** | `security/pairing.rs` (nuovo) | ~150 | TODO |
| | Senders sconosciuti ricevono un codice OTP | | | |
| | Codice valido per 5 minuti | | | |
| | Una volta approvato, l'utente e trusted | | | |
| | Config: `pairing_required = true/false` per canale | | | |
| 3.2 | **Mention gating (gruppi)** | `channels/*.rs` | ~80 | TODO |
| | Nei gruppi: rispondere solo quando menzionato | | | |
| | Config: `mention_required = true/false` per canale | | | |
| | Supporto: @homun, /homun, nome bot | | | |
| 3.3 | **Typing indicators** | `channels/*.rs` | ~60 | TODO |
| | Inviare "typing..." durante elaborazione | | | |
| | Telegram: sendChatAction("typing") | | | |
| | Discord: channel.broadcast_typing() | | | |

**Stima totale Sprint 3: ~290 LOC**

---

## Sprint 4 — Web UI Produzione (P1)

> Obiettivo: Web UI usabile per monitoring quotidiano

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| 4.1 | **Real-time logs (SSE)** | `web/api.rs`, `static/js/logs.js` | ~150 | TODO |
| | Endpoint GET /api/v1/logs/stream (SSE) | | | |
| | Pagina logs con auto-scroll e filtro per livello | | | |
| | tracing subscriber che forka eventi a SSE channel | | | |
| 4.2 | **Token usage dashboard** | `web/api.rs`, `static/js/dashboard.js` | ~200 | TODO |
| | Endpoint GET /api/v1/usage (per giorno/modello) | | | |
| | Grafici usage nel dashboard (Chart.js o inline SVG) | | | |
| | Costo stimato per provider | | | |
| 4.3 | **Config wizard web** | `static/js/setup.js` | ~100 | TODO |
| | Completare il wizard di setup iniziale | | | |
| | Test connessione provider | | | |
| | Validazione config in real-time | | | |

**Stima totale Sprint 4: ~450 LOC**

---

## Sprint 5 — Canali Phase 2 (P2)

> Obiettivo: completare i canali esistenti, aggiungerne di nuovi

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| 5.1 | **Completare Discord** | `channels/discord.rs` | ~100 | TODO |
| | Test end-to-end | | | |
| | Reaction ACKs | | | |
| | Thread support | | | |
| 5.2 | **Completare Slack** | `channels/slack.rs` | ~200 | TODO |
| | Implementazione completa Bolt-style | | | |
| | Slash commands | | | |
| | Thread support | | | |
| 5.3 | **Completare Email** | `channels/email.rs` | ~200 | TODO |
| | IMAP polling + SMTP sending | | | |
| | HTML parsing | | | |
| | Attachment handling | | | |
| 5.4 | **WhatsApp stabilizzazione** | `channels/whatsapp.rs` | ~100 | TODO |
| | Reconnect robusto | | | |
| | Group support | | | |

**Stima totale Sprint 5: ~600 LOC**

---

## Sprint 6 — Hardening (P2)

> Obiettivo: produzione-ready

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| 6.1 | **CI Pipeline** | `.github/workflows/ci.yml` | ~80 | TODO |
| | cargo fmt, clippy, test | | | |
| | Multi-feature matrix | | | |
| | Release binaries | | | |
| 6.2 | **Tool abort/timeout** | `tools/registry.rs`, `agent/agent_loop.rs` | ~80 | TODO |
| | Timeout configurabile per tool (default 60s) | | | |
| | Abort signal propagation | | | |
| 6.3 | **Provider health monitoring** | `provider/health.rs` (nuovo) | ~100 | TODO |
| | Track latency, error rate per provider | | | |
| | Auto-disable provider temporaneamente su errori | | | |
| 6.4 | **E-Stop** | `security/estop.rs` (nuovo) | ~80 | TODO |
| | Kill all tool execution | | | |
| | Network disable | | | |
| | Web UI button | | | |
| 6.5 | **Service install** | `service/launchd.rs`, `service/systemd.rs` | ~200 | TODO |
| | `homun service install` (macOS/Linux) | | | |
| | Auto-start on boot | | | |

**Stima totale Sprint 6: ~540 LOC**

---

## Sprint 7+ — Future (P3)

| Task | Priorita | Note |
|------|----------|------|
| Extended thinking (Anthropic) | P2 | Claude --thinking mode |
| Prometheus metrics | P2 | Per monitoring infra |
| Signal channel | P3 | signal-cli bridge |
| Matrix channel | P3 | matrix-sdk-rs |
| Lobster-style workflows | P3 | Multi-turn context isolation |
| Pre-built binaries | P2 | GitHub Releases |
| Docker image | P2 | Multi-arch |
| Homebrew formula | P3 | `brew install homun` |
| Documentation site | P2 | docs.homun.dev |
| OpenTelemetry | P3 | Distributed tracing |
| Mobile companion | P3 | iOS/Android |

---

## Ordine di Implementazione

```
Sprint 1: Robustezza Agent (P0)
  1.1 Provider failover
  1.2 Session compaction
  1.3 Token counting
    |
Sprint 2: Memory Search (P1)
  2.1 Hybrid search nel loop
  2.2 Embedding API provider
  2.3 Web UI memory search
    |
Sprint 3: Sicurezza Canali (P1)
  3.1 DM Pairing
  3.2 Mention gating
  3.3 Typing indicators
    |
Sprint 4: Web UI Produzione (P1)
  4.1 Real-time logs (SSE)
  4.2 Token usage dashboard
  4.3 Config wizard web
    |
Sprint 5: Canali Phase 2 (P2)
  5.1-5.4 Discord, Slack, Email, WhatsApp
    |
Sprint 6: Hardening (P2)
  6.1-6.5 CI, timeout, health, E-Stop, service
    |
Sprint 7+: Future (P3)
  Extended thinking, Prometheus, nuovi canali, distribuzione
```

**Stima totale Sprint 1-6: ~2,560 LOC**

---

## Documenti di Riferimento

| Documento | Contenuto |
|-----------|-----------|
| `docs/AUDIT-2026-03.md` | Audit completo codebase + gap analysis |
| `docs/competitors/COMPARISON.md` | Matrice comparativa dettagliata |
| `docs/competitors/openclaw.md` | Analisi OpenClaw |
| `docs/competitors/zeroclaw.md` | Analisi ZeroClaw |
| `docs/architecture/` | Diagrammi architetturali |
| `CLAUDE.md` | Istruzioni sviluppo |
| `PROJECT.md` | Visione e filosofia |

---

## Vantaggi Competitivi Homun

1. **MCP client nativo** — ne OpenClaw ne ZeroClaw
2. **Browser CDP diretto** — senza Playwright/Node.js
3. **Exfiltration filter** — OpenClaw non ce l'ha
4. **Web UI ricca** — 10+ pagine embedded
5. **Skill ecosystem** — ClawHub + OpenSkills + hot-reload
6. **Single binary Rust** — ~50MB, no runtime
7. **XML fallback auto** — supporta modelli senza function calling
8. **Prompt modulare** — sezioni componibili per mode
