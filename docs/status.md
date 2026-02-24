# Homun — Real Functional Status & Gap Analysis

> Last updated: 2026-02-24
> Confronto con: OpenClaw (Node.js), ZeroClaw (Rust)
> Status: Phase 7 (Security & Stability) — 28K LOC, 225 tests

## Stato Reale dei Sottosistemi

| Area | Stato | Note |
|------|-------|------|
| Agent loop (ReAct, multi-iterazione) | ✅ FUNZIONA | Max 20 iterazioni, tool calling, XML fallback |
| Agent loop (streaming) | ✅ FUNZIONA | Provider streaming → WebSocket/channels |
| **Exfiltration prevention** | ✅ **NUOVO** | Pattern detection + redaction su output LLM |
| **Vault leak prevention** | ✅ **NUOVO** | Redact vault values da memory files + LLM output |
| Shell tool | ✅ FUNZIONA | Sandboxing, deny list, timeout |
| File tools (read/write/edit/list) | ✅ FUNZIONA | Path expansion, blocklist sensitive paths |
| Web search (Brave) | ✅ FUNZIONA | Richiede API key |
| Web fetch | ✅ FUNZIONA | HTML stripping basico |
| Message tool (proactive send) | ✅ FUNZIONA | Cross-channel routing |
| Cron tool | ✅ FUNZIONA | Solo in gateway mode |
| MCP tool | ✅ FUNZIONA | Solo stdio transport |
| **Vault tool** | ✅ FUNZIONA | Encrypted secrets storage (store/retrieve/list/delete) |
| **Vault 2FA (TOTP)** | ✅ FUNZIONA | Authenticator app, 5-min session, recovery codes |
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

### 8. Exfiltration Prevention (T-SEC-02)

**File**: `src/security/exfiltration.rs`

Rileva e redact automaticamente i secrets nell'output dell'LLM prima che raggiungano l'utente.

**Pattern supportati (15+)**:
- **OpenAI keys**: `sk-proj-...`, `sk-svcacct-...`
- **Anthropic keys**: `sk-ant-api03-...`
- **OpenRouter keys**: `sk-or-...`
- **AWS keys**: `AKIA...`, connection strings
- **Telegram tokens**: `1234567890:ABC...`
- **Discord tokens**: `MN...`
- **GitHub PATs**: `github_pat_...`, `ghp_...`
- **JWT tokens**: `eyJ...`
- **Private keys**: `-----BEGIN RSA PRIVATE KEY-----`
- **Connection strings**: `postgresql://user:pass@...`

**Configurazione** (`config.toml`):
```toml
[security.exfiltration]
enabled = true
block_on_detection = false  # true = block output, false = redact only
log_attempts = true
custom_patterns = []  # Regex personalizzate
```

**Integrazione**: Il filtro viene applicato automaticamente in `agent_loop.rs` su ogni risposta LLM.

### 9. Vault Leak Prevention

**File**: `src/security/vault_leak.rs`, `src/agent/memory.rs`, `src/agent/agent_loop.rs`

Previene che i secrets del vault vengano leakati tramite memory files o output LLM.

**Due layer di protezione**:

1. **Memory Consolidation**: Durante la consolidazione, tutti i valori del vault vengono redatti dai file di memoria (HISTORY.md, MEMORY.md, daily files, DB chunks) e sostituiti con `vault://key_name` references.

2. **LLM Output**: Prima di restituire una risposta all'utente, l'output viene scansionato per valori del vault e redatti automaticamente.

**Flusso**:
```
LLM Response → Regex redact (API keys, tokens) → Vault value redact → User
                     │
Memory Consolidation → Save to vault → Load all vault values → Redact from text → Write to files
```

**Test**: 6 test dedicati in `vault_leak.rs` + integrazione verificata in memory consolidation.

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

### 1. ~~EXFILTRATION PREVENTION~~ ✅ COMPLETATO (P0 — Critical)

**Stato attuale**: ✅ Implementato in `src/security/exfiltration.rs`

**Completato**:
- [x] Pattern matching su API keys, tokens, passwords nell'output LLM
- [x] Redaction automatica prima dell'invio all'utente
- [x] Logging degli attempt per audit
- [x] 15+ pattern regex per OpenAI, Anthropic, AWS, Telegram, Discord, GitHub, JWT, ecc.
- [x] Configurabile via `config.toml` (sezione `[security.exfiltration]`)

### 2. CI PIPELINE (P0 — Critical)

**Stato attuale**: Nessuna automazione CI/CD.

**Cosa serve**:
- [ ] GitHub Actions workflow
- [ ] `cargo check`, `cargo test`, `cargo clippy` automatizzati
- [ ] Build artifacts per release

### 3. SUBAGENT / BACKGROUND TASKS (P1 — Alta)

**Stato attuale**: `SpawnTool` esiste ma NON è registrato nel tool registry.

**Cosa serve**:
- [ ] Registrare `SpawnTool` nel tool registry del gateway
- [ ] Gestire `subagent_result_rx` (non scartare i risultati)
- [ ] Routing risultato subagent al canale originale

### 4. BROWSER AUTOMATION (P2 — Media)

**Stato attuale**: Non implementato.

**Cosa serve**:
- [ ] Tool browser basato su `chromiumoxide` o `headless_chrome`
- [ ] navigate, click, type, screenshot, extract, evaluate
- [ ] Session management (crea/chiudi browser)

### 5. MEDIA HANDLING (P2 — Media)

**Stato attuale**: solo testo.

**Cosa serve**:
- [ ] Ricezione immagini da Telegram/Discord/WhatsApp
- [ ] Invio a LLM con vision (Anthropic/OpenAI supportano già)
- [ ] Voice messages → transcription (Groq Whisper)
- [ ] Document handling (PDF, text files)

### 6. SERVICE / DAEMON MODE (P1 — Media)

**Stato attuale**: `homun gateway` gira in foreground.

**Cosa serve**:
- [ ] `homun service install` — genera e installa unit file (systemd/launchd)
- [ ] `homun service start/stop/status`
- [ ] Auto-restart on crash

### 7. TUNNEL / ACCESSO REMOTO (P2 — Bassa)

**Stato attuale**: solo localhost.

**Cosa serve**:
- [ ] Integrazione tunnel trait (Cloudflare / Tailscale / ngrok)
- [ ] `homun tunnel start` per accesso remoto one-click

---

## Cosa Ci Rende Unici (Vantaggi da Preservare)

1. **Agent Skills standard** — compatibile con skills.sh + ClawHub + OpenSkills
2. **Web UI completa** — 8 pagine: Dashboard, Chat, Skills, Memory, Vault, Permissions, Settings, Logs
3. **TUI ricca** — ratatui con WhatsApp pairing, chat, skill management
4. **27 LLM providers** — keyword resolution, gateway/local fallback, Ollama nativo
5. **Skill security consapevole** — pre-install security scan con 20+ pattern
6. **Encrypted vault + 2FA** — AES-256-GCM, OS keychain, TOTP authenticator
7. **Exfiltration prevention** — automatic secret redaction in LLM output
8. **Vault leak prevention** — redact vault values from memory files + LLM output
9. **Rust single binary** — come ZeroClaw, ma con Web UI inclusa
10. **Memoria ibrida locale** — fastembed + USearch HNSW + FTS5, tutto offline
11. **XML tool dispatcher** — fallback per modelli senza function calling
12. **30+ REST API** — completa integrazione programmatica

---

## Roadmap Aggiornata

### ✅ Completato (Phase 1-6 + Phase 7 parziale)
1. **Core agent** — ReAct loop, tool calling, XML fallback
2. **Memoria completa** — consolidamento V2 + vector search + hybrid RRF + context injection
3. **Vault tool + Web UI** — encrypted secrets management
4. **Vault 2FA** — TOTP authenticator, recovery codes, session management
5. **Shell sandboxing** — 5-layer protection (allowlist, workspace, timeout, etc.)
6. **Graceful shutdown** — ctrl_c(), abort, grace period, DB flush
7. **Memory Web UI** — editor, search, instructions, history, daily files
8. **Skill security scanner** — pre-install malware detection
9. **Open Skills integration** — seconda fonte di skill
10. **Ollama native provider** — /api/chat, think: false, NDJSON streaming
11. **XML tool dispatcher** — fallback per modelli senza function calling
12. **REST API v1** — 30+ endpoints
13. **Web UI completa** — 8 pagine
14. **Exfiltration prevention** — secret pattern detection + redaction in LLM output
15. **Vault leak prevention** — redact vault values from memory files + LLM output

### 🔄 In Corso (Phase 7 — Security & Stability)

#### P0 — Critical
- [x] **Exfiltration prevention** — bloccare secret patterns nell'output LLM ✅ DONE
- [x] **Vault leak prevention** — redact vault values from memory/output ✅ DONE
- [ ] **CI Pipeline** — GitHub Actions (check, test, clippy)

#### P1 — Production
- [ ] **Rate limiting** — per-channel, per-user limits
- [ ] **Token/cost tracking** — usage per session/model
- [ ] **Service install** — `homun service install`, systemd/launchd
- [ ] **Slack channel** — Socket Mode
- [ ] **Email channel** — IMAP + SMTP

#### P2 — Feature Parity
- [ ] **Browser tool** — CDP per web automation (chromiumoxide)
- [ ] **Git tool** — operazioni git sicure
- [ ] **Pre-built binaries** — GitHub Releases
- [ ] **Docker image** — multi-arch

### 📅 Future (Phase 8-11)

| Phase | Focus | Timeline |
|-------|-------|----------|
| Phase 8 | Channels Expansion (Matrix, IRC, Signal) | Q2 2026 |
| Phase 9 | User System + Marketplace | Q3 2026 |
| Phase 10 | Workflow Engine (n8n-style) | Q4 2026 |
| Phase 11 | Distribution + Docs | Q1 2027 |

---

## Statistiche Progetto

| Metrica | Valore |
|---------|--------|
| File sorgente | 71 |
| Linee di codice (Rust) | ~28,000 |
| Dipendenze crate | ~50 |
| Test | 211 passing |
| REST API endpoints | 30+ |
| Web UI pages | 8 |
| Provider LLM | 27 |
| Canali | 5 (CLI, Telegram, Discord, WhatsApp, Web UI) |
| Tool built-in | 11 |
| Skill sources | 3 (GitHub, ClawHub, OpenSkills) |
| Binary size (release) | ~47MB |
