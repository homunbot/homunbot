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
| 2.1 | **Attivare hybrid search nel loop** | `agent/agent_loop.rs`, `agent/memory_search.rs` | ~450 (pre-existing) | ✅ DONE |
| | Prima di ogni chiamata LLM: cercare memorie rilevanti | | | |
| | Iniettare come "Relevant memories" nel context | | | |
| | Usare query = ultimi messaggi utente | | | |
| 2.2 | **Embedding API provider** | `agent/embeddings.rs`, `config/schema.rs` | ~180 | ✅ DONE |
| | EmbeddingProvider trait (local + OpenAI backends) | | | |
| | OpenAI text-embedding-3-small with dimensions=384 | | | |
| | Fallback su fastembed locale se non configurato | | | |
| | LRU cache (512 entries) per evitare chiamate duplicate | | | |
| 2.3 | **Web UI: memory search** | `web/api.rs`, `web/server.rs`, `static/js/memory.js` | ~60 | ✅ DONE |
| | Hybrid search (vector + FTS5) nell'endpoint API | | | |
| | MemorySearcher condiviso tra agent loop e web server | | | |
| | UI con score badge colorati per ogni risultato | | | |

**Sprint 2 completo: ~240 LOC (nuove) + ~450 LOC pre-existing**

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

## Sprint 4 — Web UI Produzione + Automations (P1)

> Obiettivo: Web UI usabile per monitoring quotidiano + sistema Automations completo

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| 4.1 | **Automations — DB e backend** | `storage/db.rs`, `scheduler/automations.rs` (nuovo) | ~300 | TODO |
| | Migrazione DB: tabella `automations` (nome, prompt, schedule, enabled, stato) | | | |
| | Tabella `automation_runs` (id, automation_id, started_at, result, status) | | | |
| | Scheduler upgrade: eseguire prompt complessi (non solo messaggi) | | | |
| | Supporto cron expression + intervallo + "esegui ora" manuale | | | |
| | Salvataggio ultimo risultato + confronto con precedente (per trigger condizionali) | | | |
| 4.2 | **Automations — API e CLI** | `web/api.rs`, `main.rs` | ~200 | TODO |
| | CRUD API: GET/POST/PATCH/DELETE `/api/v1/automations` | | | |
| | GET `/api/v1/automations/:id/history` (storico esecuzioni) | | | |
| | POST `/api/v1/automations/:id/run` (esegui ora) | | | |
| | CLI: `homun automations {list,add,run,toggle,remove,history}` | | | |
| 4.3 | **Automations — Web UI** | `web/pages.rs`, `static/js/automations.js` (nuovo) | ~250 | TODO |
| | Pagina `/automations` con lista, status, prossima esecuzione | | | |
| | Form creazione: nome + prompt naturale + schedule (cron/intervallo) | | | |
| | Modifica inline, toggle on/off, pulsante "Esegui ora" | | | |
| | Storico esecuzioni con risultato di ogni run | | | |
| 4.4 | **Real-time logs (SSE)** | `web/api.rs`, `static/js/logs.js` | ~150 | TODO |
| | Endpoint GET /api/v1/logs/stream (SSE) | | | |
| | Pagina logs con auto-scroll e filtro per livello | | | |
| | tracing subscriber che forka eventi a SSE channel | | | |
| 4.5 | **Token usage dashboard** | `web/api.rs`, `static/js/dashboard.js` | ~200 | TODO |
| | Endpoint GET /api/v1/usage (per giorno/modello) | | | |
| | Grafici usage nel dashboard (Chart.js o inline SVG) | | | |
| | Costo stimato per provider | | | |
| 4.6 | **Config wizard web** | `static/js/setup.js` | ~100 | TODO |
| | Completare il wizard di setup iniziale | | | |
| | Test connessione provider | | | |
| | Validazione config in real-time | | | |

**Stima totale Sprint 4: ~1,200 LOC**

### Esempi Automations

| Nome | Prompt | Schedule |
|------|--------|----------|
| Email digest | "Vai su Gmail, leggi le email non lette, fammi un riassunto" | `0 9 * * *` |
| Price tracker | "Cerca su Amazon 'AirPods Pro', controlla il prezzo. Se e' cambiato avvisami" | `0 */6 * * *` |
| Volo tracker | "Cerca il volo piu' economico Roma-Londra per il 15 aprile" | `0 8 * * *` |
| Backup check | "Controlla che il backup sia andato a buon fine, leggi i log" | `0 7 * * *` |
| News briefing | "Cerca le notizie principali su Rust e AI, riassumi le top 5" | `0 8 * * 1-5` |

---

## Sprint 5 — Ecosistema: MCP Setup + Skill Creator (P1)

> Obiettivo: rendere Homun auto-espandibile — si connette a servizi esterni da solo
> e crea le proprie skill su misura per l'utente

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| 5.1 | **MCP Setup Guidato** | `tools/mcp.rs`, `skills/mcp_registry.rs` (nuovo) | ~300 | TODO |
| | Registry di MCP server noti (Gmail, Calendar, GitHub, Notion, etc.) | | | |
| | `homun mcp setup gmail` — scarica server, guida OAuth, testa connessione | | | |
| | Web UI: pagina MCP con "Connect" one-click per servizi noti | | | |
| | Auto-discovery: suggerire MCP server in base al contesto ("vuoi che legga le email? Posso collegarmi a Gmail") | | | |
| | Gestione credenziali OAuth → vault | | | |
| 5.2 | **Skill Creator (agente)** | `skills/creator.rs` (nuovo), `tools/skill_create.rs` (nuovo) | ~400 | TODO |
| | Tool `create_skill` — l'agent crea nuove skill da prompt naturale | | | |
| | Analizza skill esistenti per riusare pattern/pezzi utili | | | |
| | Genera SKILL.md (frontmatter YAML + body) + script (Python/Bash/JS) | | | |
| | Composizione: combinare logica da piu' skill in una nuova | | | |
| | Test automatico: esegue la skill creata e verifica il risultato | | | |
| | Installazione automatica in `~/.homun/skills/` | | | |
| 5.3 | **Creazione automation da chat** | `agent/context.rs`, `tools/automation.rs` (nuovo) | ~200 | TODO |
| | Tool `create_automation` — l'agent crea automations dalla conversazione | | | |
| | "Ogni mattina controllami le email" → automation creata + confermata | | | |
| | Suggerimento proattivo: "Vuoi che lo faccia ogni giorno?" dopo task ripetitivi | | | |
| 5.4 | **Skill Adapter (ClawHub → Homun)** | `skills/adapter.rs` (nuovo) | ~200 | TODO |
| | Parsing formato OpenClaw (SKILL.toml / manifest.json) | | | |
| | Conversione automatica a formato Homun (SKILL.md + YAML frontmatter) | | | |
| | Mapping path script: `src/` → `scripts/`, adattamento entry point | | | |
| | Gestione dipendenze: npm → warning, pip → requirements.txt auto-install | | | |
| 5.5 | **Skill Shield (sicurezza pre-install)** | `skills/shield.rs` (nuovo) | ~250 | TODO |
| | Analisi statica: regex pattern sospetti (reverse shell, crypto mining, `eval`, `rm -rf`, network calls non dichiarate) | | | |
| | VirusTotal API: upload hash script → check reputation (free tier: 4 req/min) | | | |
| | Report di sicurezza pre-installazione con risk score | | | |
| | Blocco automatico se risk > threshold, override manuale con `--force` | | | |
| | Cache risultati VirusTotal per evitare re-check su skill gia' verificate | | | |

**Stima totale Sprint 5: ~1,350 LOC**

### Come funziona lo Skill Creator

```
Tu: "Creami una skill che controlla i prezzi su Amazon e li salva in un CSV"

Homun:
  1. Cerca skill esistenti simili (web scraping, CSV, price tracking)
  2. Analizza i pattern utili (parsing HTML, formato output)
  3. Genera SKILL.md:
     ---
     name: amazon-price-tracker
     description: Track product prices on Amazon and log to CSV
     version: 1.0.0
     scripts:
       - scripts/track.py
     ---
  4. Genera scripts/track.py (usando pezzi da skill esistenti)
  5. Testa: esegue con un URL di esempio
  6. Installa in ~/.homun/skills/amazon-price-tracker/

"Skill 'amazon-price-tracker' creata e testata. Vuoi che la esegua periodicamente?"
Tu: "si, ogni 6 ore"
→ Crea automation automaticamente
```

### Come funziona MCP Setup Guidato

```
Tu: "Voglio che tu possa leggere le mie email"

Homun:
  1. Cerca nel registry MCP: "email" → @anthropic/mcp-gmail
  2. "Posso collegarmi a Gmail via MCP. Procedo con il setup?"
  Tu: "si"
  3. Scarica/installa il server MCP
  4. Guida OAuth: "Apri questo link per autorizzare l'accesso..."
  5. Salva credenziali nel vault
  6. Testa: "Ho letto 3 email non lette. Funziona!"
  7. Aggiunge a config.toml automaticamente

"Gmail collegato. Ora posso leggere, cercare e riassumere le tue email."
```

### Come funziona lo Skill Shield

```
homun skills add clawhub:user/data-scraper

[1/3] Downloading skill...
[2/3] Security scan:
  Static analysis:
    ✅ No shell injection patterns
    ⚠️  Network call: requests.get() — declared in SKILL.md
    ✅ No filesystem access outside workspace
    ✅ No crypto mining patterns
  VirusTotal:
    ✅ 0/72 engines flagged scripts/scrape.py
  Risk score: LOW (2/10)
[3/3] Adapting to Homun format...
  Converted SKILL.toml → SKILL.md
  Mapped src/scrape.py → scripts/scrape.py

Skill 'data-scraper' installed. Ready to use.
```

---

## Sprint 6 — RAG: Knowledge Base Personale (P1)

> Obiettivo: Homun puo' cercare nei tuoi documenti, file, e dati cloud.
> "Cerca nei miei documenti..." diventa naturale come "cerca su Google...".

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| 6.1 | **File ingestion pipeline** | `agent/rag.rs` (nuovo), `agent/chunking.rs` (nuovo) | ~400 | TODO |
| | Watcher su cartelle configurate (notify crate, gia' usato per skills) | | | |
| | Parser: Markdown, TXT, codice sorgente (nativi) | | | |
| | Parser: PDF (`pdf-extract`), DOCX (`docx-rs`), HTML (gia' presente) | | | |
| | Chunking intelligente: rispetta paragrafi/sezioni, overlap configurable | | | |
| | De-duplicazione: skip chunk se gia' indicizzato (hash check) | | | |
| 6.2 | **Indice RAG** | `agent/embeddings.rs`, `storage/db.rs` | ~200 | TODO |
| | Tabella `rag_sources` (path, tipo, ultimo_scan, chunk_count) | | | |
| | Tabella `rag_chunks` (source_id, chunk_text, embedding_id, metadata) | | | |
| | Embedding + HNSW indexing (riusa EmbeddingEngine esistente) | | | |
| | FTS5 parallelo per keyword search (riusa pattern memory_search) | | | |
| | Ricerca unificata: RAG + memory nella stessa query | | | |
| 6.3 | **Config e UI** | `config/schema.rs`, `web/api.rs`, `static/js/rag.js` (nuovo) | ~250 | TODO |
| | Config: `rag.sources = [{ path = "~/Documents", recursive = true }]` | | | |
| | CLI: `homun rag add ~/Documents`, `homun rag status`, `homun rag rebuild` | | | |
| | Web UI: pagina `/knowledge` — sorgenti, statistiche, ricerca dedicata | | | |
| | API: GET/POST `/api/v1/rag/sources`, GET `/api/v1/rag/search` | | | |
| 6.4 | **File via Telegram → RAG** | `channels/telegram.rs` | ~150 | TODO |
| | Download file inviati via Telegram in `~/.homun/inbox/` | | | |
| | Auto-parsing e analisi nel contesto della conversazione | | | |
| | Opzione: indicizzare nel RAG per ricerche future | | | |
| | Supporto: PDF, immagini (→ vision model), testo, codice | | | |
| 6.5 | **Sorgenti cloud via MCP** | `tools/mcp.rs` | ~100 | TODO |
| | Google Drive via MCP server → file sincronizzati in locale → indicizzati | | | |
| | Notion via MCP → pagine esportate → indicizzate | | | |
| | Qualsiasi MCP server che espone file → pipeline automatica | | | |

**Stima totale Sprint 6: ~1,100 LOC**

### Come funziona il RAG

```
# Aggiungere una cartella alla knowledge base
homun rag add ~/Documents/lavoro --recursive
  Scanning... 142 files found
  Indexing... 847 chunks created (384-dim vectors)
  Done. Knowledge base: 847 chunks from 142 files.

# In chat, la ricerca e' trasparente
Tu: "Cosa diceva il contratto con Acme Corp sulla clausola di rinnovo?"
Homun:
  1. Cerca nel RAG: "contratto Acme Corp clausola rinnovo"
  2. Trova chunk rilevante da ~/Documents/lavoro/contratto-acme.pdf
  3. Risponde con il contenuto + citazione del file sorgente

# File via Telegram
Tu (Telegram): [invii fattura.pdf]
Homun: "Ho ricevuto fattura.pdf (2 pagine). E' una fattura di 1.250€ da
        Fornitore XYZ per servizi consulenza. Vuoi che la salvi nella
        knowledge base per riferimento futuro?"
Tu: "si"
→ Indicizzata e ricercabile
```

---

## Sprint 7 — Canali Phase 2 (P2)

> Obiettivo: completare i canali esistenti, aggiungerne di nuovi

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| 7.1 | **Completare Discord** | `channels/discord.rs` | ~100 | TODO |
| | Test end-to-end | | | |
| | Reaction ACKs | | | |
| | Thread support | | | |
| 7.2 | **Completare Slack** | `channels/slack.rs` | ~200 | TODO |
| | Implementazione completa Bolt-style | | | |
| | Slash commands | | | |
| | Thread support | | | |
| 7.3 | **Completare Email** | `channels/email.rs` | ~200 | TODO |
| | IMAP polling + SMTP sending | | | |
| | HTML parsing | | | |
| | Attachment handling | | | |
| 7.4 | **WhatsApp stabilizzazione** | `channels/whatsapp.rs` | ~100 | TODO |
| | Reconnect robusto | | | |
| | Group support | | | |

**Stima totale Sprint 7: ~600 LOC**

---

## Sprint 8 — Hardening (P2)

> Obiettivo: produzione-ready

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| 8.1 | **CI Pipeline** | `.github/workflows/ci.yml` | ~80 | TODO |
| | cargo fmt, clippy, test | | | |
| | Multi-feature matrix | | | |
| | Release binaries | | | |
| 8.2 | **Tool abort/timeout** | `tools/registry.rs`, `agent/agent_loop.rs` | ~80 | TODO |
| | Timeout configurabile per tool (default 60s) | | | |
| | Abort signal propagation | | | |
| 8.3 | **Provider health monitoring** | `provider/health.rs` (nuovo) | ~100 | TODO |
| | Track latency, error rate per provider | | | |
| | Auto-disable provider temporaneamente su errori | | | |
| 8.4 | **E-Stop** | `security/estop.rs` (nuovo) | ~80 | TODO |
| | Kill all tool execution | | | |
| | Network disable | | | |
| | Web UI button | | | |
| 8.5 | **Service install** | `service/launchd.rs`, `service/systemd.rs` | ~200 | TODO |
| | `homun service install` (macOS/Linux) | | | |
| | Auto-start on boot | | | |

**Stima totale Sprint 8: ~540 LOC**

---

## Sprint 9+ — Future (P3)

| Task | Priorita | Note |
|------|----------|------|
| Extended thinking (Anthropic) | P2 | Claude --thinking mode |
| Prometheus metrics | P2 | Per monitoring infra |
| Voice (Whisper STT + TTS) | P2 | Input/output vocale |
| Signal channel | P3 | signal-cli bridge |
| Matrix channel | P3 | matrix-sdk-rs |
| Lobster-style workflows | P3 | Multi-turn context isolation |
| Pre-built binaries | P2 | GitHub Releases |
| Docker image | P2 | Multi-arch |
| Homebrew formula | P3 | `brew install homun` |
| Documentation site | P2 | docs.homun.dev |
| OpenTelemetry | P3 | Distributed tracing |

---

## Ordine di Implementazione

```
Sprint 1: Robustezza Agent (P0)            ✅ DONE (~594 LOC)
  1.1 Provider failover
  1.2 Session compaction
  1.3 Token counting
    |
Sprint 2: Memory Search (P1)               ✅ DONE (~240 LOC)
  2.1 Hybrid search nel loop
  2.2 Embedding API provider
  2.3 Web UI memory search
    |
Sprint 3: Sicurezza Canali (P1)             TODO (~290 LOC)
  3.1 DM Pairing
  3.2 Mention gating
  3.3 Typing indicators
    |
Sprint 4: Web UI + Automations (P1)        TODO (~1,200 LOC)
  4.1-4.3 Automations (DB + API + Web UI)
  4.4 Real-time logs (SSE)
  4.5 Token usage dashboard
  4.6 Config wizard web
    |
Sprint 5: Ecosistema (P1)                  TODO (~1,350 LOC)
  5.1 MCP Setup Guidato
  5.2 Skill Creator (agente)
  5.3 Creazione automation da chat
  5.4 Skill Adapter (ClawHub → Homun)
  5.5 Skill Shield (sicurezza pre-install)
    |
Sprint 6: RAG Knowledge Base (P1)          TODO (~1,100 LOC)
  6.1 File ingestion pipeline
  6.2 Indice RAG (embedding + HNSW + FTS5)
  6.3 Config e UI
  6.4 File via Telegram → RAG
  6.5 Sorgenti cloud via MCP
    |
Sprint 7: Canali Phase 2 (P2)              TODO (~600 LOC)
  7.1-7.4 Discord, Slack, Email, WhatsApp
    |
Sprint 8: Hardening (P2)                   TODO (~540 LOC)
  8.1-8.5 CI, timeout, health, E-Stop, service
    |
Sprint 9+: Future (P3)
  Voice, Extended thinking, Prometheus, distribuzione
```

**Completato: Sprint 1-2 (~834 LOC)**
**Rimanente: Sprint 3-8 (~5,080 LOC)**

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
