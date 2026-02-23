# Homun — Real Functional Status & Gap Analysis

> Last updated: 2026-02-21
> Confronto con: OpenClaw (Node.js), ZeroClaw (Rust)

## Stato Reale dei Sottosistemi

| Area | Stato | Note |
|------|-------|------|
| Agent loop (ReAct, multi-iterazione) | ✅ FUNZIONA | Max 20 iterazioni, tool calling, XML fallback |
| Agent loop (streaming) | ✅ FUNZIONA | Provider streaming → WebSocket/channels |
| Shell tool | ✅ FUNZIONA | Sandboxing, deny list, timeout |
| File tools (read/write/edit/list) | ✅ FUNZIONA | Path expansion, blocklist sensitive paths |
| Web search (Brave) | ✅ FUNZIONA | Richiede API key |
| Web fetch | ✅ FUNZIONA | HTML stripping basico |
| Message tool (proactive send) | ✅ FUNZIONA | Cross-channel routing |
| Cron tool | ✅ FUNZIONA | Solo in gateway mode |
| MCP tool | ✅ FUNZIONA | Solo stdio transport |
| **Vault tool** | ✅ **NUOVO** | Encrypted secrets storage (store/retrieve/list/delete) |
| Context builder | ✅ FUNZIONA | Skills + memory + bootstrap files |
| CLI channel | ✅ FUNZIONA | REPL + one-shot |
| Telegram channel | ✅ FUNZIONA | Long polling, media limitato |
| Discord channel | ✅ FUNZIONA | Serenity, comandi base |
| WhatsApp channel | ✅ FUNZIONA | Richiede pairing TUI prima |
| WebSocket chat | ✅ FUNZIONA | Streaming, sessioni isolate |
| Skill loading + attivazione | ✅ FUNZIONA | Progressive disclosure |
| Cron scheduler | ✅ FUNZIONA | No step values (*/5) |
| TUI (ratatui) | ✅ FUNZIONA | WhatsApp pairing, chat, skills |
| **Web UI — Dashboard** | ✅ FUNZIONA | Stats, channels, quick actions |
| **Web UI — Chat** | ✅ FUNZIONA | WebSocket streaming, markdown |
| **Web UI — Skills** | ✅ FUNZIONA | ClawHub + OpenSkills search, install |
| **Web UI — Memory** | ✅ **NUOVO** | Editor MEMORY.md, search, instructions, history, daily files |
| **Web UI — Vault** | ✅ **NUOVO** | Manage encrypted secrets, reveal modal |
| **Web UI — Settings/Setup** | ✅ FUNZIONA | Provider config, channels config |
| **Web UI — Logs** | ✅ FUNZIONA | Real-time log viewer |
| **Memoria — caricamento MEMORY.md** | ✅ FUNZIONA | Letto all'avvio, iniettato nel context |
| **Memoria — consolidamento V2** | ✅ FUNZIONA | LLM classifica: history, facts, instructions, secrets |
| **Memoria — vector search (USearch)** | ✅ FUNZIONA | HNSW index, O(log N), ~/.homun/memory.usearch |
| **Memoria — FTS5 keyword search** | ✅ FUNZIONA | BM25 scoring, trigger auto-sync |
| **Memoria — hybrid search (RRF)** | ✅ FUNZIONA | Reciprocal Rank Fusion: vector + keyword |
| **Memoria — context injection** | ✅ FUNZIONA | Layer 3.5 "Relevant Past Context" |
| **Skill security scanner** | ✅ **NUOVO** | Pre-install security scan (malware, rm -rf, pipe to shell, base64 exec, reverse shell) |
| **Open Skills integration** | ✅ **NUOVO** | Search/install from besoeasy/open-skills repo |
| **Ollama native provider** | ✅ **NUOVO** | /api/chat, think: false per cloud models, NDJSON streaming |
| **XML tool dispatcher** | ✅ **NUOVO** | Fallback per modelli senza function calling |
| **REST API v1** | ✅ **NUOVO** | 30+ endpoints: config, skills, memory, vault, channels, providers |
| **Skill executor (scripts)** | ⚠️ PARZIALE | Implementato ma raramente usato |
| **Subagent (spawn)** | ❌ NON REGISTRATO | SpawnTool non nel registry, risultati scartati |
| **Browser tool** | ❌ MANCANTE | Non implementato |
| **Media handling** | ❌ MANCANTE | Solo testo, niente immagini/audio |

---

## Nuove Funzionalità Implementate (Febbraio 2026)

### 1. Vault Tool + Web UI

**Tool**: `src/tools/vault.rs`
**API**: `POST/GET/DELETE /api/v1/vault`, `POST /api/v1/vault/{key}/reveal`
**Web UI**: `/vault` — pagina completa per gestire secrets

Features:
- Store/retrieve/list/delete encrypted secrets
- AES-256-GCM encryption via OS keychain
- `vault://key_name` references in memory (never plaintext)
- Reveal modal con auto-hide countdown (10s)
- Copy to clipboard

### 2. Memory Web UI

**API**: 
- `GET/PUT /api/v1/memory/content` — MEMORY.md editor
- `GET /api/v1/memory/search` — hybrid search UI
- `GET /api/v1/memory/instructions` — learned instructions
- `GET /api/v1/memory/history` — history chunks
- `GET /api/v1/memory/daily` — daily log files

**Web UI**: `/memory` — pagina con:
- Editor per MEMORY.md con save/reload
- Ricerca ibrida (vector + FTS5) con risultati live
- Lista instructions apprese (add/remove)
- History entries con load more
- Daily files browser (YYYY-MM-DD.md)

### 3. Skill Security Scanner

**File**: `src/skills/security.rs`

Scansiona SKILL.md prima dell'installazione per:
- **Critical patterns**: `rm -rf /`, fork bomb, malware, keylogger, ransomware
- **Pipe to shell**: `curl | bash`, `wget | sh`
- **Base64 exec**: `base64 -d | bash`
- **Reverse shell**: `nc -e /bin/sh`, `/dev/tcp/`
- **Warning patterns**: `sudo`, `/etc/passwd`, `~/.ssh/`

Score: 0.0 (blocked) → 1.0 (clean)

### 4. Open Skills Integration

**File**: `src/skills/openskills.rs`

Seconda fonte di skill oltre a ClawHub:
- Repo: `besoeasy/open-skills` su GitHub
- Catalog cache con refresh 24h
- Install con: `openskills:skill-name`
- Integrato nella Web UI Skills page

### 5. Ollama Native Provider

**File**: `src/provider/ollama.rs`

Provider nativo (non OpenAI-compatible shim):
- Endpoint: `/api/chat` (NDJSON streaming)
- **think: false** per cloud models (`:cloud` suffix) — disabilita reasoning 30-120s → 2-8s
- Tool calls con arguments come JSON objects (no string parsing)
- Bearer token auth per Ollama cloud direct
- Normalizzazione tool calls annidati (`tool_call`, `tool.call`)

### 6. XML Tool Dispatcher

**File**: `src/provider/xml_dispatcher.rs`

Fallback per LLM senza function calling nativo:
- Inietta tool definitions come XML nel system prompt
- Parsa `<tool_call&gt;` tags dalla risposta
- Auto-attivato quando il provider rifiuta tool specs

### 7. REST API v1 Completa

**30+ endpoints**:

```
/api/health
/api/v1/status
/api/v1/config (GET, PATCH)
/api/v1/skills (GET, DELETE)
/api/v1/skills/search
/api/v1/skills/install
/api/v1/skills/catalog/status
/api/v1/skills/catalog/refresh
/api/v1/providers
/api/v1/providers/configure
/api/v1/providers/activate
/api/v1/providers/ollama/models
/api/v1/channels/{name}
/api/v1/channels/configure
/api/v1/channels/whatsapp/pair (WebSocket)
/api/v1/memory/stats
/api/v1/memory/content (GET, PUT)
/api/v1/memory/search
/api/v1/memory/history
/api/v1/memory/instructions (GET, PUT)
/api/v1/memory/daily
/api/v1/vault (GET, POST)
/api/v1/vault/{key}/reveal
/api/v1/vault/{key} (DELETE)
/api/v1/chat/history
```

---

## ✅ Memoria — COMPLETAMENTE FUNZIONANTE

La memoria ora funziona end-to-end con architettura ibrida:

### Stack Tecnico
- **Embedding**: `fastembed` (ONNX locale, modello `AllMiniLML6V2Q`, 384 dim, multilingue)
- **Vector Index**: `usearch` (HNSW, O(log N), file `~/.homun/memory.usearch`)
- **Keyword Search**: SQLite FTS5 con BM25 scoring
- **Hybrid Merge**: Reciprocal Rank Fusion (RRF)

### File Coinvolti
| File | Ruolo |
|------|-------|
| `src/agent/embeddings.rs` | Embedding engine (fastembed + USearch) |
| `src/agent/memory_search.rs` | Hybrid searcher (vector + FTS5 → RRF) |
| `src/agent/memory.rs` | Consolidation V2 con classificazione |
| `src/agent/agent_loop.rs` | Integrazione: search → context → consolidate |
| `src/agent/context.rs` | Layer 3.5 "Relevant Past Context" |
| `migrations/002_memory_chunks.sql` | Schema memory_chunks + FTS5 triggers |
| `src/storage/db.rs` | Operazioni DB per chunks e FTS5 |

### Confronto con Competitor

| Aspetto | OpenClaw | ZeroClaw | **Homun** |
|---------|----------|----------|-----------|
| Storage testo | Markdown files | SQLite | **Markdown + SQLite** |
| Vector index | N/A | SQLite BLOB (brute-force) | **USearch HNSW O(log N)** |
| Embedding | N/A | API esterna | **fastembed locale (ONNX)** |
| Keyword search | N/A | FTS5 BM25 | **FTS5 BM25** |
| Hybrid merge | N/A | 0.7/0.3 weighting | **RRF (Reciprocal Rank Fusion)** |
| Offline | Sì | No (richiede API) | **Sì (tutto locale)** |
| Web UI | No | No | **Sì (Memory page)** |
| Leggibilità | Sì (markdown) | No | **Sì (MEMORY.md + memory/YYYY-MM-DD.md)** |

---

## Gap Critici Rimanenti vs Competitor

### 1. SUBAGENT / BACKGROUND TASKS (Priorità ALTA)

**Stato attuale**: `SpawnTool` esiste ma NON è registrato nel tool registry. In `main.rs`:
```rust
let (subagent_result_tx, _subagent_result_rx) = tokio::sync::mpsc::channel(50);
// _subagent_result_rx → risultati scartati
// TODO: register SpawnTool in the tool_registry
```

**Cosa serve**:
- [ ] Registrare `SpawnTool` nel tool registry del gateway
- [ ] Gestire `subagent_result_rx` (non scartare i risultati)
- [ ] Routing risultato subagent al canale originale

### 2. BROWSER AUTOMATION (Priorità ALTA)

**Stato attuale**: Non implementato.

**Cosa serve**:
- [ ] Tool browser basato su `chromiumoxide` o `headless_chrome`
- [ ] navigate, click, type, screenshot, extract, evaluate
- [ ] Session management (crea/chiudi browser)

### 3. MEDIA HANDLING (Priorità MEDIA)

**Stato attuale**: solo testo.

**Cosa serve**:
- [ ] Ricezione immagini da Telegram/Discord/WhatsApp
- [ ] Invio a LLM con vision (Anthropic/OpenAI supportano già)
- [ ] Voice messages → transcription (Groq Whisper)
- [ ] Document handling (PDF, text files)

### 4. SERVICE / DAEMON MODE (Priorità MEDIA)

**Stato attuale**: `homun gateway` gira in foreground.

**Cosa serve**:
- [ ] `homun service install` — genera e installa unit file (systemd/launchd)
- [ ] `homun service start/stop/status`
- [ ] Auto-restart on crash

### 5. TUNNEL / ACCESSO REMOTO (Priorità BASSA)

**Stato attuale**: solo localhost.

**Cosa serve**:
- [ ] Integrazione tunnel trait (Cloudflare / Tailscale / ngrok)
- [ ] `homun tunnel start` per accesso remoto one-click

---

## Cosa Ci Rende Unici (Vantaggi da Preservare)

1. **Agent Skills standard** — compatibile con skills.sh + ClawHub + OpenSkills
2. **Web UI completa** — 7 pagine: Dashboard, Chat, Skills, Memory, Vault, Settings, Logs
3. **TUI ricca** — ratatui con WhatsApp pairing, chat, skill management
4. **15 LLM providers** — keyword resolution, gateway/local fallback, Ollama nativo
5. **Skill security consapevole** — pre-install security scan con 20+ pattern
6. **Encrypted vault** — AES-256-GCM, OS keychain, accessible via tool + Web UI
7. **Rust single binary** — come ZeroClaw, ma con Web UI inclusa
8. **Memoria ibrida locale** — fastembed + USearch HNSW + FTS5, tutto offline
9. **XML tool dispatcher** — fallback per modelli senza function calling
10. **30+ REST API** — completa integrazione programmatica

---

## Roadmap Aggiornata

### ✅ Completato
1. **Memoria completa** — consolidamento V2 + vector search + hybrid RRF + context injection
2. **Vault tool + Web UI** — encrypted secrets management
3. **Memory Web UI** — editor, search, instructions, history, daily files
4. **Skill security scanner** — pre-install malware detection
5. **Open Skills integration** — seconda fonte di skill
6. **Ollama native provider** — /api/chat, think: false, NDJSON streaming
7. **XML tool dispatcher** — fallback per modelli senza function calling
8. **REST API v1** — 30+ endpoints

### Priorità 1 — Core functionality
1. **Subagent** — registrare SpawnTool, gestire risultati, routing
2. **Browser tool** — CDP per web automation (chromiumoxide)

### Priorità 2 — UX completeness
3. **Media handling** — immagini (vision LLM) + voice (Whisper)
4. **Service install** — daemon mode su systemd/launchd
5. **Webhook inbound** — trigger l'agente da servizi esterni

### Priorità 3 — Feature parity
6. **Tunnel** — accesso remoto one-click
7. **Più canali** — Slack, Email, Matrix
8. **apply_patch tool** — modifica file strutturata (come OpenClaw)
9. **Observability** — metrics/tracing traits

---

## Statistiche Progetto

| Metrica | Valore |
|---------|--------|
| File sorgente | ~55 |
| Linee di codice (Rust) | ~18.000+ |
| Dipendenze crate | ~45 |
| Test | 200+ |
| REST API endpoints | 30+ |
| Web UI pages | 7 |
| Provider LLM | 15 |
| Canali | 4 (+ Web) |
| Tool built-in | 12 |
| Skill sources | 3 (GitHub, ClawHub, OpenSkills) |
