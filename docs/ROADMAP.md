# Homun — Development Roadmap

> Last updated: 2026-03-21
> Basato su: Audit completo (`docs/AUDIT-2026-03.md`)
> Gap analysis: Homun vs OpenClaw vs ZeroClaw
> Source of truth: questo documento e' la roadmap/status operativa del progetto

---

## Status Attuale

| Metrica | Valore |
|---------|--------|
| LOC Rust | ~41,343 |
| LOC Frontend | ~8,691 |
| Test | 312 passing (verificato con `cargo test -q` il 2026-03-04) |
| Binary (full) | ~50MB |
| Provider LLM | 14 |
| Canali | 7 (CLI, Telegram, Discord*, WhatsApp*, Slack*, Email*, Web) |
| Tool built-in | ~20 (incl. knowledge, workflow, business, browser, approval, read_email) |
| Pagine Web UI | 17 (/chat, /dashboard, /setup, /channels, /browser, /automations, /workflows, /business, /skills, /mcp, /memory, /knowledge, /vault, /permissions, /approvals, /account, /logs) |
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
| 3.1 | **DM Pairing** | `security/pairing.rs` (nuovo), `agent/gateway.rs` | ~175 | ✅ DONE |
| | Senders sconosciuti ricevono un codice OTP | | | |
| | Codice valido per 5 minuti, max 3 tentativi | | | |
| | Una volta approvato, l'utente e trusted (via UserManager) | | | |
| | Config: `pairing_required = true/false` per canale | | | |
| 3.2 | **Mention gating (gruppi)** | `channels/telegram.rs`, `discord.rs`, `slack.rs` | ~100 | ✅ DONE |
| | Nei gruppi: rispondere solo quando @menzionato o reply-to-bot | | | |
| | Config: `mention_required = true/false` per canale (default true) | | | |
| | Strip menzione dal testo prima di forwarding all'agent | | | |
| 3.3 | **Typing indicators** | `channels/telegram.rs`, `discord.rs` | ~20 | ✅ DONE |
| | Telegram: sendChatAction("typing") | | | |
| | Discord: broadcast_typing() | | | |
| | Slack: nessun supporto nativo | | | |

**Sprint 3 completo: ~295 LOC**

### Checklist Nuovo Canale

Quando si aggiunge un nuovo canale, implementare sempre:

- [ ] **Pairing**: integrare `PairingManager::check_sender()` nel gateway (config: `pairing_required`)
- [ ] **Mention gating**: nei gruppi, rispondere solo se @menzionato o reply-to-bot (config: `mention_required`)
- [ ] **Typing indicator**: inviare indicatore "typing..." prima di forwardare all'agent (se la piattaforma lo supporta)
- [ ] **Web UI settings**: aggiungere card in `build_channels_cards_html()` + gestione nel JS `setup.js`

---

## Sprint 4 — Web UI Produzione + Automations (P1)

> Obiettivo: Web UI usabile per monitoring quotidiano + sistema Automations completo

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| 4.1 | **Automations — DB e backend** | `storage/db.rs`, `scheduler/automations.rs` (nuovo) | ~300 | ✅ DONE |
| | Migrazione DB: tabella `automations` (nome, prompt, schedule, enabled, stato) | | | |
| | Tabella `automation_runs` (id, automation_id, started_at, result, status) | | | |
| | Scheduler upgrade: eseguire prompt complessi (non solo messaggi) | | | |
| | Supporto cron expression + intervallo + "esegui ora" manuale | | | |
| | Salvataggio ultimo risultato + confronto con precedente (per trigger condizionali) | | | |
| 4.2 | **Automations — API e CLI** | `web/api.rs`, `main.rs` | ~200 | ✅ DONE |
| | CRUD API: GET/POST/PATCH/DELETE `/api/v1/automations` | | | |
| | GET `/api/v1/automations/:id/history` (storico esecuzioni) | | | |
| | POST `/api/v1/automations/:id/run` (esegui ora) | | | |
| | CLI: `homun automations {list,add,run,toggle,remove,history}` | | | |
| 4.3 | **Automations — Web UI** | `web/pages.rs`, `static/js/automations.js` (nuovo) | ~250 | ✅ DONE |
| | Pagina `/automations` con lista, status, prossima esecuzione | | | |
| | Form creazione: nome + prompt naturale + schedule (cron/intervallo) | | | |
| | Modifica inline, toggle on/off, pulsante "Esegui ora" | | | |
| | Storico esecuzioni con risultato di ogni run | | | |
| 4.4 | **Real-time logs (SSE)** | `web/api.rs`, `static/js/logs.js` | ~150 | ✅ DONE |
| | Endpoint GET /api/v1/logs/stream (SSE) | | | |
| | Pagina logs con auto-scroll e filtro per livello | | | |
| | tracing subscriber che forka eventi a SSE channel | | | |
| 4.5 | **Token usage dashboard (API + UI + costi)** | `web/api.rs`, `web/pages.rs`, `static/js/dashboard.js`, `static/css/style.css`, `storage/db.rs` | ~200 | ✅ DONE |
| | Endpoint GET /api/v1/usage (per giorno/modello) | | | |
| | Grafici usage nel dashboard (Chart.js o inline SVG) | | | |
| | Costo stimato per provider | | | |
| 4.6 | **Config wizard web (wizard + provider test + validazione realtime)** | `web/pages.rs`, `web/api.rs`, `static/js/setup.js`, `static/css/style.css` | ~100 | ✅ DONE |
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
| 5.1 | **MCP Setup Guidato** | `tools/mcp.rs`, `skills/mcp_registry.rs` (nuovo), `web/api.rs`, `web/pages.rs`, `static/js/mcp.js` | ~600 | ✅ DONE |
| | Registry di MCP server noti (Gmail, Calendar, GitHub, Notion, etc.) | | | |
| | `homun mcp setup gmail` — scarica server, guida OAuth, testa connessione | | | |
| | Web UI: pagina MCP con "Connect" one-click per servizi noti | | | |
| | Auto-discovery: suggerire MCP server in base al contesto ("vuoi che legga le email? Posso collegarmi a Gmail") | | | |
| | Gestione credenziali OAuth → vault | | | |
| 5.2 | **Skill Creator (agente)** | `skills/creator.rs` (nuovo), `tools/skill_create.rs` (nuovo) | ~400 | ✅ DONE |
| | Tool `create_skill` — l'agent crea nuove skill da prompt naturale | | | |
| | Analizza skill esistenti per riusare pattern/pezzi utili | | | |
| | Genera SKILL.md (frontmatter YAML + body) + script (Python/Bash/JS) | | | |
| | Composizione: combinare logica da piu' skill in una nuova | | | |
| | Test automatico: esegue la skill creata e verifica il risultato | | | |
| | Installazione automatica in `~/.homun/skills/` | | | |
| 5.3 | **Creazione automation da chat** | `agent/context.rs`, `tools/automation.rs` (nuovo) | ~200 | ✅ DONE |
| | Tool `create_automation` — l'agent crea automations dalla conversazione | | | |
| | "Ogni mattina controllami le email" → automation creata + confermata | | | |
| | Suggerimento proattivo: "Vuoi che lo faccia ogni giorno?" dopo task ripetitivi | | | |
| 5.4 | **Skill Adapter (ClawHub → Homun)** | `skills/adapter.rs` (nuovo) | ~200 | ✅ DONE |
| | Parsing formato OpenClaw (SKILL.toml / manifest.json) | | | |
| | Conversione automatica a formato Homun (SKILL.md + YAML frontmatter) | | | |
| | Mapping path script: `src/` → `scripts/`, adattamento entry point | | | |
| | Gestione dipendenze: npm → warning, pip → requirements.txt auto-install | | | |
| 5.5 | **Skill Shield (sicurezza pre-install)** | `skills/security.rs` | ~250 | ✅ DONE |
| | Analisi statica: regex pattern sospetti (reverse shell, crypto mining, `eval`, `rm -rf`, network calls non dichiarate) | | | |
| | VirusTotal API: upload hash script → check reputation (free tier: 4 req/min) | | | |
| | Report di sicurezza pre-installazione con risk score | | | |
| | Blocco automatico se risk > threshold, override manuale con `--force` | | | |
| | Cache risultati VirusTotal per evitare re-check su skill gia' verificate | | | |

**Stima totale Sprint 5: ~1,350 LOC**

### 5.1 Stato Dettagliato (MCP Setup Guidato)

- ✅ Catalogo MCP multi-sorgente attivo in Web UI:
  - Official MCP Registry (`registry.modelcontextprotocol.io`)
  - Top 100 MCPMarket (`/leaderboards`, con fallback locale)
  - Preset curati (`skills/mcp_registry.rs`)
- ✅ Installazione guidata in MCP page:
  - prefill automatico form manuale (`command/args/url/env`)
  - supporto `vault://...` per secret
  - Quick Add disponibile per utenti avanzati
- ✅ Install Assistant con endpoint dedicato:
  - `POST /api/v1/mcp/install-guide`
  - guida LLM + fallback strutturato per env vars
  - loading state esplicito in UI
- ✅ Gestione server MCP completa via Web UI:
  - list/add/test/toggle/remove
  - test connessione con sandbox condivisa
- ✅ Auto-discovery proattiva nel loop conversazionale:
  - suggerimento MCP nel prompt quando il task richiede Gmail/Calendar/GitHub/etc. e il server non e' ancora configurato
- ✅ OAuth Google assistito end-to-end:
  - consent URL + callback page + code exchange + salvataggio secret nel Vault + test immediato post-setup
- ✅ OAuth GitHub assistito end-to-end:
  - consent URL + callback page + code exchange + salvataggio token nel Vault + wiring automatico in `GITHUB_PERSONAL_ACCESS_TOKEN`
- ✅ UX installazione/permessi molto piu' guidata:
  - wizard MCP coerente, helper OAuth integrato, preset sandbox chiari e recommendation panel
- ✅ Provider OAuth multipli supportati nel wizard:
  - Google (Gmail, Calendar) + GitHub con callback provider-aware in Web UI

### 5.5 Stato Dettagliato (Skill Shield)

- ✅ Analisi statica estesa:
  - scan di `SKILL.md` + script/package files (`scripts/`, shell/python/js/etc.)
  - pattern sospetti: reverse shell, pipe-to-shell, obfuscation, sudo/SUID, accesso secret/system files, network activity non dichiarata
- ✅ Report strutturato con risk score:
  - `risk_score` 0-100, `score` normalizzato, count file scansionati, findings ordinati per severita'
- ✅ Reputation check opzionale:
  - lookup hash script su VirusTotal se `VIRUSTOTAL_API_KEY` e' presente
  - nessun hard failure se la reputation API non e' disponibile
- ✅ Cache locale:
  - cache persistente per report package + reputazione hash in `~/.homun/skill-security-cache.json`
- ✅ Enforcement installazione:
  - preflight remoto su `SKILL.md`
  - full scan post-download su package estratto
  - blocco automatico sopra threshold
  - override manuale via `homun skills add ... --force`

### 5.2 Stato Dettagliato (Skill Creator)

- ✅ Tool `create_skill` registrato nell'agent loop:
  - genera una skill installata in `~/.homun/skills/<name>/`
  - crea `SKILL.md` + script starter (`python|bash|javascript`)
- ✅ Riuso pattern locale:
  - cerca skill esistenti correlate, ne carica workflow/tools/scripts e le include come pattern di composizione
- ✅ Composizione da piu' skill:
  - genera `references/composition.md` con i pattern riusati
  - fonde `allowed-tools` dalle skill correlate quando disponibili
- ✅ Validazione automatica iniziale:
  - parse frontmatter, syntax-check script, scan sicurezza package
- ✅ Smoke test automatico:
  - esegue lo script generato con `--smoke-test` e verifica il marker `homun_skill_smoke_ok`

### 5.4 Stato Dettagliato (Skill Adapter)

- ✅ Modulo adapter legacy introdotto:
  - parsing `SKILL.toml` / `manifest.json`
  - generazione automatica `SKILL.md`
  - mapping script `src/`/entrypoint -> `scripts/`
  - `requirements.txt` auto-generato da dipendenze pip quando possibile
- ✅ Integrazione completa sugli installer supportati:
  - fallback a manifest legacy se `SKILL.md` manca
  - adattamento automatico post-download prima del security scan finale
  - supporto attivo su GitHub, ClawHub e Open Skills
- ✅ Note di compatibilita' esplicite:
  - dipendenze pip convertite quando possibile
  - dipendenze npm/runtime non Python lasciate come note operative nella skill adattata

---

## Programma Trasversale — Sandbox Unificata (P0/P1)

> Obiettivo: eseguire Shell, MCP stdio e script skill in un runtime coerente, sicuro e multi-piattaforma.

### Stato ad oggi (2026-03-06)

- ✅ **Fondazioni implementate (milestone 1, macOS-first)**
  - Config unica sandbox (`security.execution_sandbox`) con backend `auto|docker|none` + `strict`.
  - Runtime wrapper condiviso (`src/tools/sandbox_exec.rs`) usato da:
    - Shell tool (`src/tools/shell.rs`)
    - MCP stdio (`src/tools/mcp.rs`)
    - Skill executor (`src/skills/executor.rs`)
  - API Web dedicate:
    - `GET/PUT /api/v1/security/sandbox`
    - `GET /api/v1/security/sandbox/status`
    - `GET /api/v1/security/sandbox/presets`
    - `GET /api/v1/security/sandbox/image`
    - `POST /api/v1/security/sandbox/image/pull`
    - `GET /api/v1/security/sandbox/events`
  - UI Permissions con sezione Execution Sandbox (stato runtime, backend, limiti CPU/RAM, network, readonly rootfs, mount workspace, preset rapidi, runtime image status/pull, recent events).
  - Badge/runtime status in Skills e MCP pages + link rapido a Permissions.
- ✅ **Comportamento attuale robusto su macOS**
  - Se Docker non e' disponibile e backend=`auto`, fallback controllato a native.
  - Con `strict=true`, blocco esecuzione quando backend richiesto non disponibile.
- ✅ **Osservabilita' e operativita'**
  - Event log recente delle decisioni sandbox condiviso tra shell, MCP e skill scripts.
  - Stato immagine runtime Docker ispezionabile dalla UI con pull manuale del runtime configurato.
- ⚠️ **In corso / parziale**
  - Lifecycle immagine presente a livello operativo, ma manca ancora versioning/policy di update piu' rigorosa.

### Milestone Sandbox — Dove siamo

| Milestone | Scope | Stato |
|-----------|-------|-------|
| SBX-1 | Backend unificato + wiring su Shell/MCP/Skills + API/UI runtime status | ✅ DONE |
| SBX-2 | Hard isolation backend Linux (namespaces/seccomp/cgroups) oltre Docker fallback | TODO |
| SBX-3 | Backend Windows nativo (Job Objects/AppContainer o equivalente) | TODO |
| SBX-4 | Runtime image gestita (template immagine/toolchain per skill+MCP) + lifecycle/versioning | ⚠️ PARTIAL |
| SBX-5 | UX finale Permissions/Sandbox semplificata (onboarding guidato + spiegazioni contestuali) | ✅ DONE |
| SBX-6 | Test E2E cross-platform (macOS/Linux/Windows) e policy hardening finale | TODO |

### Cosa manca per chiudere il cerchio Sandbox

- Implementare backend hardened nativi per Linux e Windows (non solo strategia Docker/none).
- Completare versioning/update policy dell'immagine runtime standard per skill/MCP.
- Aggiungere policy di rete piu' granulari (es. allowlist host/domain per runtime isolato).
- Chiudere hardening finale con test E2E cross-platform e verifiche sui fallback reali.

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

## Programma Trasversale — Chat Web UI (P1)

> Obiettivo: portare la chat Web UI da "funzionante" a esperienza primaria, persistente e robusta.

### Stato ad oggi (2026-03-06)

- ✅ **Fondazioni UX e loop migliorate**
  - Chat shell ridisegnata con composer sticky, model picker minimale, timeline tool/reasoning piu' leggibile.
  - Prompt/tool routing corretto: per ricerca informativa il sistema preferisce `web_search`/`web_fetch` prima del browser.
  - Finalizzazione best-effort quando il loop esaurisce le iterazioni, per evitare `max iterations reached without final response`.
  - Stop end-to-end con cancel propagation reale su provider streaming e tool lunghi.
- ✅ **Persistenza e multi-chat**
  - Sessioni multiple vere con sidebar conversazioni, rename/archive/delete e ricerca.
  - Run web persistiti su DB con `run_id`, stato, prompt utente, risposta parziale, eventi tool ed `effective_model`.
  - Restore corretto dopo page switch e dopo restart del processo, con run interrotti marcati come tali.
- ✅ **Composer `+` e allegati**
  - Upload immagini e documenti end-to-end dal composer.
  - Ingressi MCP reali dal composer, persistiti nella history della chat.
  - Auto-scroll affidabile sul fondo chat durante history load, streaming e tool activity.
- ✅ **Routing multimodale e BYOK capability-based**
  - Il turno usa il modello chat attivo se supporta input immagine, altrimenti `vision_model`, altrimenti fallback MCP capability-based.
  - Supporto multimodale nativo nel provider layer per modelli compatibili (incl. OpenAI-compatible, Anthropic e Ollama vision).
  - Capability per modello configurabili dalla UI (`multimodal`, `image_input`, `native tool calls`), con prefill automatico per modelli noti e override manuale per custom/BYOK.
- ⚠️ **Parziale / da chiudere**
  - Mancano ancora i test E2E completi della chat (streaming, stop, resume, multi-sessione, attachment flow).
  - Il supporto documento resta ibrido: testo locale quando possibile, altrimenti vision/MCP; il passaggio a document input nativo provider-specific e' da espandere.
  - Resta del polish UI finale da consolidare, ma non blocca l'uso primario della chat.

### Milestone Chat — Dove siamo

| Milestone | Scope | Stato |
|-----------|-------|-------|
| CHAT-1 | Refresh UI chat (composer sticky, reasoning/tool timeline, stop base, minimal shell) | ✅ DONE |
| CHAT-2 | Run web persistente in memoria con resume/background dopo page switch | ✅ DONE |
| CHAT-3 | Sessioni multiple vere + sidebar/history conversazioni | ✅ DONE |
| CHAT-4 | Persistenza run su DB + restore dopo restart processo | ✅ DONE |
| CHAT-5 | Composer `+` completo (immagini, documenti, ingressi MCP reali) + routing multimodale capability-based | ✅ DONE |
| CHAT-6 | Stop profondo / cancellation propagation su provider e tool lunghi | ✅ DONE |
| CHAT-7 | Test E2E Playwright per streaming/stop/resume/multi-sessione | TODO |

### Cosa manca per chiudere davvero la Chat

- Completare **CHAT-7 / test E2E**:
  - invio messaggio
  - streaming
  - stop
  - cambio pagina durante run
  - restore run attivo
  - multi-sessione
  - upload allegati + MCP context
- Estendere il **multimodale oltre il v1 attuale**:
  - input documento nativo dove il provider/model lo supporta chiaramente
  - OCR / pipeline documento binario piu' robusta
  - fallback MCP multipli con policy piu' ricca e reporting migliore
- Fare **polish finale streaming/layout**:
  - stabilita' layout durante risposta in corso
  - gestione robusta di error/offline/reconnect
  - cleanup del vecchio codice UI residuo
- Coprire anche la **UX dei model capability settings**:
  - deep-link Settings dal composer per i badge capability
  - verifica capability per modello custom/BYOK

### Ordine consigliato per chiuderla

1. CHAT-7 test E2E completi
2. hardening multimodale/document pipeline
3. polish finale streaming/layout

---

## Programma Trasversale — Browser Automation (P1)

> Obiettivo: browser automation robusta, usabile anche da modelli deboli (Ollama, DeepSeek).
> Riferimento architetturale: [agent-browser.dev](https://github.com/vercel-labs/agent-browser) (Vercel Labs)

### Architettura

```
config.toml [browser]
       │
       ▼
mcp_bridge.rs ─── genera McpServerConfig per @playwright/mcp
       │
       ▼
McpPeer (persistente) ─── connessione stdio al server MCP Playwright
       │
       ▼
BrowserTool ─── tool unificato "browser" con ~17 azioni
       │          │
       │          ├── inject_stealth() ─── anti-bot detection (addInitScript)
       │          ├── wait_for_stable_snapshot() ─── attesa SPA con stability check
       │          ├── compact_browser_snapshot() ─── compaction tree (agent-browser style)
       │          ├── extract_autocomplete_suggestions() ─── auto-detect dopo type
       │          └── normalize_ref() ─── fix ref malformati da modelli deboli
       │
       ▼
agent_loop.rs ─── browser_task_plan (veto/guard), execution_plan, supersede context
```

### Stato ad oggi (2026-03-08)

- ✅ **Migrazione da custom Playwright sidecar a MCP**
  - Eliminati `src/browser/{actions,manager,snapshot,tool}.rs` (~4,500 LOC rimossi)
  - Browser gestito come MCP server `@playwright/mcp` via `npx`
  - Connessione persistente (peer sopravvive tra tool call)
  - Supporto profili persistenti con `--user-data-dir`
  - Config: `[browser] enabled/headless/browser_type/executable`
- ✅ **Tool unificato `browser`** (`src/tools/browser.rs`)
  - ~40 tool MCP individuali → 1 tool con enum `action`
  - Azioni: `navigate`, `snapshot`, `click`, `type`, `fill`, `select_option`,
    `press_key`, `hover`, `scroll`, `drag`, `tab_*`, `evaluate`, `wait`, `close`
  - Schema piatto (no `anyOf`) — compatibile con tutti i provider
  - Ref normalization: `"ref=e42"`, `"42"`, `"e42"` → `"e42"`
- ✅ **Stealth anti-bot detection**
  - `addInitScript` iniettato prima della prima navigazione via `browser_run_code`
  - Patch: `navigator.webdriver=false`, `window.chrome.runtime`, `navigator.plugins`,
    `navigator.permissions.query`
  - Equivalente a `playwright-extra-plugin-stealth` senza dipendenza npm
  - Nota: agent-browser.dev NON fa stealth di default (lo delega a cloud provider Kernel)
- ✅ **Snapshot compaction** (ispirato a agent-browser.dev `compact_tree`)
  - Tree-preserving: mantiene gerarchia con indentazione
  - Tiene: elementi con `[ref=]`, content roles (`heading`, `cell`, `listitem`), value text
  - Ricostruisce antenati per contesto (bottone dentro dialog, risultato dentro lista)
  - Max 50K chars (configurabile via `HOMUN_BROWSER_MAX_OUTPUT`)
- ✅ **Orchestrazione intelligente nel tool**
  - Auto-snapshot dopo `navigate` con stability check (count elementi stabilizzato, fino a 5 retry)
  - Auto-snapshot dopo `click` (fix stale refs post-autocomplete)
  - Auto-snapshot dopo `type` con autocomplete detection
  - Consecutive snapshot guard (blocca snapshot doppi senza azione intermedia)
  - DOM manipulation guard su `evaluate` (blocca `.click()`, `.focus()`, `scrollTo()` etc.)
  - Form plan injection (istruzioni per compilazione form)
- ✅ **Browser task planning** (`src/agent/browser_task_plan.rs`)
  - Veto system: blocca azioni non-selection quando autocomplete e' aperto
  - Blocca cambio sorgente prima di estrarre risultati correnti
  - Tracciamento stato form (campi compilati, autocomplete attivo)
- ✅ **Execution plan** (`src/agent/execution_plan.rs`)
  - Piano strutturato per task browser complessi
  - Hinting form fields dal snapshot

### Cosa manca / miglioramenti futuri

- ⬚ **Stealth avanzato**: wrapper script Chrome con `--disable-blink-features=AutomationControlled`
  (piu' robusto di `addInitScript` per anti-bot C++ level)
- ⬚ **CDP endpoint mode**: lanciare Chrome separatamente e connettere via `--cdp-endpoint`
  (profilo utente reale, nessun flag automazione)
- ⬚ **Screenshot/vision fallback**: quando il modello ha `image_input`, inviare screenshot
  per pagine dove lo snapshot accessibilita' non basta
- ⬚ **Caching refs cross-action**: evitare snapshot ridondanti tracciando quali refs sono ancora validi
- ⬚ **Test E2E browser**: test automatizzati del flow completo (navigate → fill → submit → extract)
- ⬚ **Rate limiting per sito**: delay configurabile tra azioni per evitare ban
- ⬚ **Cookie consent auto-dismiss**: detect e click automatico sui banner cookie
  (senza usare `evaluate` — via `click` su ref riconosciuto)

### Differenze da agent-browser.dev

| Aspetto | agent-browser.dev | Homun |
|---------|-------------------|-------|
| Stealth | No (delega a Kernel cloud) | Si (`addInitScript` built-in) |
| Snapshot | `compact_tree` con tutti i content roles | Stessa logica, adattata |
| Auto-snapshot | Solo su snapshot esplicito | Dopo navigate, click, type |
| Stability check | No (snapshot singolo) | Si (retry + count stabilizzato) |
| Ref normalization | No (modello deve mandare ref esatto) | Si (fix `"42"` → `"e42"`) |
| Form planning | No | Si (istruzioni pre-fill iniettate) |
| DOM guard | No | Si (blocca evaluate mutanti) |
| Tool design | Azioni come comandi separati | Tool singolo con enum action |

---

## Programma Trasversale — Design System "Olive Moss Console" (P1)

> Obiettivo: passare da una palette generica a un design system proprietario,
> con neutrali caldi fissi (olive/moss + stone) e accento selezionabile dall'utente.

### Stato ad oggi (2026-03-09)

- ✅ **Design token architecture**
  - `:root` (light) e `.dark` token set completi: accent, surface, text, border, semantic (ok/warn/err/info)
  - Palette neutrali: warm stone (`#F3F1EB`/`#ECE8DE` light, `#1D1C18`/`#262520` dark)
  - Accent di default: olive saturo `#628A4A` (light), lifted `#82A868` (dark)
  - Tutte le inline `rgba()` allineate alla palette (base `44,41,36`, accent `111,123,87`)
  - Zero colori hardcoded: ogni valore cromatico passa per `var(--token)`
- ✅ **Accent picker system**
  - 4 preset: Moss (default), Terracotta (`#B85C38`), Plum (`#7A5C68`), Stone (`#7A7268`)
  - Ogni preset ha varianti light + dark via `[data-accent="name"]` CSS selectors
  - Custom color picker (`<input type="color">`) con derivazione HSL completa
  - `deriveAccentFamily(hex)`: da un singolo hex genera 9 proprietà (hover, active, light, border, text, focus-ring, selection-bg, chart-primary)
  - Persistenza in `localStorage` + restore senza flash (inline `<head>` script)
  - Config backend: `UiConfig.accent` salvato in `config.toml` via API
- ✅ **Semantic color tokenization**
  - Famiglie semantiche: `--ok`/`--ok-bg`, `--warn`/`--warn-bg`, `--err`/`--err-bg`, `--info`/`--info-bg`
  - Usate ovunque: toast, badge, test results, ACL entries, MCP status, e-stop
  - `--text-on-accent` per tutti i testi su sfondi colorati (sostituisce `#fff` hardcoded)
- ✅ **Typography**
  - Dual-font: Geist (UI/body) + Plus Jakarta Sans (display headings)
  - Scale tipografica coerente via token
- ✅ **Settings UI**
  - Sezione Appearance in Settings con swatch picker + color input
  - Live preview: cambio accento istantaneo senza reload

### File principali modificati

| File | Modifiche |
|------|-----------|
| `static/css/style.css` | Token `:root`/`.dark`, accent variants, accent picker CSS, semantic colors |
| `src/web/pages.rs` | Accent picker HTML, `<head>` inline script per flash prevention |
| `src/config/schema.rs` | `UiConfig.accent` field |
| `static/js/setup.js` | `applyAccent()`, `deriveAccentFamily()`, `hexToHSL()`/`hslToHex()` |
| `static/js/theme.js` | Theme toggle (light/dark) con persistence |

---

## Sprint 6 — RAG: Knowledge Base Personale (P1)

> Obiettivo: Homun puo' cercare nei tuoi documenti, file, e dati cloud.
> "Cerca nei miei documenti..." diventa naturale come "cerca su Google...".
> Feature differenziante #1: ne' OpenClaw ne' ZeroClaw hanno RAG personale.

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| 6.1 | **DB + migrazione RAG** | `migrations/011_rag_knowledge.sql`, `storage/db.rs` | ~150 | ✅ DONE |
| | Tabella `rag_sources` (id, file_path, file_name, file_hash SHA-256, doc_type, file_size, chunk_count, status, error_message, source_channel, created_at, updated_at) | | | |
| | Tabella `rag_chunks` (id, source_id FK, chunk_index, heading, content, token_count, created_at) | | | |
| | Tabella FTS5 `rag_fts` con trigger di sincronizzazione (INSERT/DELETE/UPDATE) | | | |
| | Metodi CRUD: insert/find/update/delete source, insert/load/update chunk, fts5_search, count | | | |
| 6.2 | **Chunker modulare** | `rag/mod.rs`, `rag/chunker.rs` | ~460 | ✅ DONE |
| | `DocChunk { index, heading, content, token_count }` + `ChunkOptions { max_tokens: 512, overlap: 50 }` | | | |
| | Algoritmi: chunk_markdown (split su heading), chunk_code (double-blank), chunk_html (strip tags), chunk_plain_text (paragrafi) | | | |
| | Estensioni supportate: md, txt, log, rs, py, js, ts, go, java, c, cpp, h, hpp, toml, yaml, yml, json, html, htm, css, sh, bash, zsh, sql, xml, csv, ini, cfg, conf, env, dockerfile, makefile | | | |
| | Unit test: detect_doc_type, is_supported, estimate_tokens, chunk sizes, markdown headings, html strip | | | |
| 6.3 | **RAG Engine** | `rag/engine.rs` | ~370 | ✅ DONE |
| | `RagEngine::ingest_file()` — SHA-256 dedup → chunk → embed (filename+content) → HNSW + FTS5 | | | |
| | `RagEngine::ingest_directory()` — batch ingestion con filtro estensioni | | | |
| | `RagEngine::search()` — ibrido vector (HNSW cosine) + FTS5 keyword + RRF merge | | | |
| | Filename in heading (FTS5 matching per nome file) + filename in embedding (vector matching) | | | |
| | Auto-reindex all'avvio: `reindex_if_needed()` ricostruisce HNSW se DB ha chunk ma indice e' vuoto | | | |
| | Persist HNSW dopo ogni ingestion (non solo auto-save ogni 50) | | | |
| | `reindex_all()` con fix heading orfani + embedding filename+content | | | |
| | `remove_source()`, `list_sources()`, `stats()`, `save_index()` | | | |
| 6.4 | **Tool LLM `knowledge`** | `tools/knowledge.rs`, `tools/mod.rs` | ~120 | ✅ DONE |
| | Azioni: `search` (query → chunk text con attribuzione file), `ingest` (file/dir), `list`, `remove` | | | |
| | Condivide `Arc<Mutex<RagEngine>>` con agent loop e web server | | | |
| | Descrizione ottimizzata: enfatizza che search restituisce il contenuto reale, non solo nomi file | | | |
| 6.5 | **Config + EmbeddingEngine RAG** | `config/schema.rs`, `agent/embeddings.rs` | ~40 | ✅ DONE |
| | `KnowledgeConfig { enabled, chunk_max_tokens, chunk_overlap_tokens, results_per_query }` | | | |
| | `EmbeddingEngine::with_provider_and_path()` — indice HNSW separato (`rag.usearch`) | | | |
| 6.6 | **Wiring startup** | `main.rs`, `lib.rs` | ~60 | ✅ DONE |
| | `try_create_rag_engine()` — crea engine + auto-reindex | | | |
| | Registrazione KnowledgeTool + passaggio handle a agent/web | | | |
| | Feature-gated sotto `local-embeddings` (nel feature set `gateway`) | | | |
| 6.7 | **Integrazione agent loop** | `agent/agent_loop.rs`, `agent/context.rs` | ~50 | ✅ DONE |
| | RAG search automatica ad ogni messaggio (inietta chunk nel system prompt) | | | |
| | Formato: `[RAG: filename (chunk N)] contenuto` | | | |
| | `ContextBuilder::set_rag_knowledge()` + sezione dopo relevant_memories | | | |
| 6.8 | **Web API** | `web/api.rs`, `web/server.rs` | ~200 | ✅ DONE |
| | `GET /api/v1/knowledge/stats` — source_count, chunk_count, vector_count | | | |
| | `GET /api/v1/knowledge/sources` — lista sorgenti + `DELETE` per rimozione | | | |
| | `GET /api/v1/knowledge/search?q=...&limit=5` — ricerca ibrida | | | |
| | `POST /api/v1/knowledge/ingest` — upload file multipart + ingestion | | | |
| | `AppState.rag_engine` condiviso con gateway | | | |
| 6.9 | **Web UI `/knowledge`** | `web/pages.rs`, `static/js/knowledge.js`, `static/css/style.css` | ~470 | ✅ DONE |
| | Card statistiche (sorgenti, chunk, vettori) | | | |
| | Upload zone drag & drop + file picker | | | |
| | Tabella sorgenti con nome, tipo, chunk, size, status, data, delete | | | |
| | Search con risultati attribuiti (file, score, heading, contenuto) | | | |
| | Design Braun-inspired coerente con il resto della UI | | | |
| 6.10 | **Telegram file → RAG** | `channels/telegram.rs`, `agent/gateway.rs`, `bus/queue.rs` | ~90 | ✅ DONE |
| | Download documento via Telegram API → file temporaneo | | | |
| | Auto-ingestion nel RAG engine (dedup via SHA-256) | | | |
| | Routing intelligente: file senza caption → skip agent (solo conferma), file con caption → hint per knowledge tool | | | |
| | Conferma utente con source_id e chunk count | | | |
| | Cleanup file temporaneo dopo ingestion | | | |
| 6.11 | **Formati file avanzati (PDF, DOCX)** | `rag/chunker.rs`, `rag/parsers.rs`, `Cargo.toml` | ~150 | DONE |
| | Parser PDF (`pdf-extract` o `lopdf` + `pdf_text`) — estrazione testo, page-aware chunking | | | |
| | Parser DOCX (`docx-rs`) — estrazione testo strutturato | | | |
| | Parser XLSX/CSV avanzato — tabelle → chunk per foglio/sezione | | | |
| | Aggiungere estensioni: pdf, docx, xlsx, xls, pptx, rtf, odt | | | |
| 6.12 | **Indicizzazione cartelle da Web UI e CLI** | `web/api.rs`, `web/pages.rs`, `static/js/knowledge.js`, `main.rs` | ~200 | DONE |
| | Web UI: campo path + checkbox recursive + bottone "Index Folder" | | | |
| | API: `POST /api/v1/knowledge/ingest-directory` — ingest da path server-side | | | |
| | CLI: `homun knowledge add ~/Documents --recursive` | | | |
| | Progress reporting per ingestion grandi (numero file processati / totale) | | | |
| 6.13 | **Protezione dati sensibili (vault-gated access + 2FA)** | `rag/sensitive.rs`, `rag/engine.rs`, `tools/knowledge.rs`, `web/api.rs`, `storage/db.rs` | ~200 | DONE |
| | Classificazione automatica: detect pattern sensibili nel contenuto (API key, token, password, recovery key, codice fiscale, IBAN) | | | |
| | Marcatura chunk come `sensitive = true` in DB (colonna o flag su `rag_chunks`) | | | |
| | L'LLM puo' vedere che il chunk esiste e il suo heading, ma il contenuto e' mascherato | | | |
| | Per mostrare il contenuto: richiedere auth token (vault PIN, Telegram OTP, o web session token) | | | |
| | Dopo autenticazione: contenuto visibile per la durata della sessione | | | |
| | Tool knowledge: azione `search` restituisce `[REDACTED — auth required]` per chunk sensibili | | | |
| | Web UI: risultati sensibili con lucchetto, click per sbloccare con auth | | | |
| 6.14 | **Directory watcher** | `rag/watcher.rs`, `rag/engine.rs`, `config/schema.rs`, `main.rs` | ~140 | DONE |
| | Watcher su cartelle configurate (`knowledge.watch_dirs` in config) | | | |
| | Auto-ingest su file nuovo/modificato (via notify crate, gia' usato per skills) | | | |
| | Debounce per evitare re-ingestion durante salvataggio | | | |
| | Re-hash e re-chunk se file modificato | | | |
| 6.15 | **Sorgenti cloud via MCP (framework)** | `rag/cloud.rs`, `tools/mcp.rs`, `config/schema.rs`, `main.rs` | ~180 | DONE |
| | Google Drive via MCP server → file sincronizzati in locale → indicizzati | | | |
| | Notion via MCP → pagine esportate → indicizzate | | | |
| | Qualsiasi MCP server che espone file → pipeline automatica | | | |

**Sprint 6 completato: ~2,830 LOC (6.1-6.15 tutti DONE)**

### 6.1-6.10 Stato Dettagliato (Core RAG — Completato)

- ✅ **Architettura**: modulo separato `src/rag/` (chunker.rs + engine.rs), tabelle DB dedicate (`rag_sources` + `rag_chunks`), indice HNSW separato (`rag.usearch`)
- ✅ **Ingestion pipeline completa**: file → SHA-256 dedup → chunk (per tipo documento) → embed (fastembed local o OpenAI) → HNSW + FTS5
- ✅ **Ricerca ibrida**: vector cosine (HNSW) + keyword (FTS5) + RRF merge — filename incluso in heading e embedding per matching per nome file
- ✅ **Auto-recovery**: reindex automatico all'avvio se HNSW vuoto ma DB ha chunk (sopravvive a restart)
- ✅ **30+ estensioni supportate**: md, txt, log, codice (rs/py/js/ts/go/java/c/cpp/h), config (toml/yaml/json/xml/csv/ini), html, shell scripts
- ✅ **Telegram end-to-end**: invia file → auto-download → ingestion → conferma → query via chat → risposta con contenuto
- ✅ **Web UI completa**: pagina /knowledge con upload drag&drop, tabella sorgenti, search con risultati attribuiti, stats card
- ✅ **Tool LLM**: `knowledge` tool con search/ingest/list/remove — l'agent lo usa automaticamente per domande sui documenti
- ✅ **Context injection**: RAG search automatica nel system prompt ad ogni messaggio (come per le memorie)

### 6.13 Design: Vault-Gated Access per Dati Sensibili

```
# Il sistema indicizza un file con una recovery key
Tu (Telegram): [invii MEGA-CHIAVEDIRECUPERO.txt]
Homun: "File indicizzato (1 chunk). Rilevato contenuto sensibile (recovery key)."

# Quando chiedi il contenuto...
Tu: "Qual e' la chiave di recupero di MEGA?"
Homun: "Ho trovato il file MEGA-CHIAVEDIRECUPERO.txt nella knowledge base.
        Il contenuto e' classificato come sensibile.
        Per visualizzarlo, inserisci il PIN del vault o conferma da Telegram."

# Dopo autenticazione
Tu: [conferma PIN/OTP]
Homun: "Chiave di recupero MEGA: icLTS4lgw7YBfkIccHo-kQ"
```

Pattern sensibili riconosciuti:
- API key / token (formato `sk-...`, `ghp_...`, `xoxb-...`, base64 lunghi)
- Password / secret (keyword match)
- Recovery key / seed phrase
- Codici fiscali, IBAN, numeri carta
- File con nome suggestivo (contiene "password", "secret", "key", "token", "recovery")

### Come funziona il RAG

```
# Aggiungere una cartella alla knowledge base (CLI — 6.12)
homun knowledge add ~/Documents/lavoro --recursive
  Scanning... 142 files found
  Indexing... 847 chunks created (384-dim vectors)
  Done. Knowledge base: 847 chunks from 142 files.

# In chat, la ricerca e' trasparente
Tu: "Cosa diceva il contratto con Acme Corp sulla clausola di rinnovo?"
Homun:
  1. Cerca nel RAG: "contratto Acme Corp clausola rinnovo"
  2. Trova chunk rilevante da ~/Documents/lavoro/contratto-acme.pdf
  3. Risponde con il contenuto + citazione del file sorgente

# File via Telegram (gia' funzionante)
Tu (Telegram): [invii fattura.pdf]
Homun: "File indicizzato nella knowledge base (source_id=7, 3 chunk).
        Chiedimi qualsiasi cosa sul contenuto."
Tu: "Quanto devo pagare?"
Homun: "La fattura e' di 1.250€ da Fornitore XYZ per servizi consulenza,
        scadenza 30/04/2026."
```

---

## Programma Workflow Engine — Autonomia Multi-Step (P1)

> Obiettivo: orchestrazione persistente di task multi-step che sopravvivono ai restart,
> passano contesto tra step, supportano approval gates, e possono essere collegati ad automazioni e cron.

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| WF-1 | **Schema DB + tipi** | `migrations/013_workflows.sql`, `workflows/mod.rs` | ~280 | ✅ DONE |
| | Tabelle `workflows` e `workflow_steps` con status, context JSON, retry count | | | |
| | Enums: WorkflowStatus (6 stati), StepStatus (5 stati) | | | |
| | Structs: Workflow, WorkflowStep, WorkflowCreateRequest, StepDefinition | | | |
| | WorkflowEvent enum per notifiche (step completed, approval needed, etc.) | | | |
| WF-2 | **DB layer** | `workflows/db.rs` | ~330 | ✅ DONE |
| | CRUD: insert_workflow, load_workflow, list_workflows | | | |
| | Status updates: update_workflow_status, update_step_status | | | |
| | Context: update_workflow_context, update_workflow_step_idx | | | |
| | Resume: load_resumable_workflows (running/pending on boot) | | | |
| | Retry: increment_step_retry, cancel_pending_steps | | | |
| WF-3 | **Engine (orchestratore)** | `workflows/engine.rs` | ~490 | ✅ DONE |
| | create_and_start() — valida, persiste, avvia esecuzione | | | |
| | run_workflow_loop() — esegue step sequenziali via AgentLoop | | | |
| | Approval gates — pausa + notifica + resume su conferma utente | | | |
| | Retry logic — retry_count < max_retries, poi fail workflow | | | |
| | Inter-step context — risultati precedenti iniettati nel prompt | | | |
| | resume_on_startup() — riprende workflow interrotti al boot | | | |
| WF-4 | **Tool LLM** | `tools/workflow.rs` | ~310 | ✅ DONE |
| | 5 azioni: create, list, status, approve, cancel | | | |
| | OnceCell late-binding (stesso pattern di SpawnTool) | | | |
| | deliver_to per routing notifiche al canale corretto | | | |
| WF-5 | **Wiring gateway** | `main.rs`, `agent/gateway.rs`, `tools/mod.rs` | ~80 | ✅ DONE |
| | WorkflowEngine init con DB + AgentLoop + event channel | | | |
| | Event loop nel gateway per routing notifiche ai canali | | | |
| | Resume automatico workflow al boot del gateway | | | |
| WF-6 | **Web UI workflows** | `web/pages.rs`, `web/api.rs`, `static/js/workflows.js` | ~640 | ✅ DONE |
| | Pagina /workflows con stats grid, create form, lista, detail panel | | | |
| | 5 API endpoints (list, create, get, approve, cancel) | | | |
| | Step builder dinamico + step timeline con stato/risultato | | | |
| | Auto-refresh 15s, approve/cancel da UI | | | |
| WF-7 | **Trigger da automazioni/cron** | `scheduler/cron.rs`, `storage/db.rs`, `web/api.rs`, `static/js/automations.js` | ~180 | ✅ DONE |
| | Colonna `workflow_steps_json` su automations (migrazione 014) | | | |
| | CronScheduler con WorkflowEngine via OnceCell (late-binding) | | | |
| | Se automation ha steps → crea workflow, altrimenti prompt singolo (fallback) | | | |
| | Toggle "Execute as workflow" nel form automazioni + step builder | | | |

**Completato: WF-1..7 (~2,310 LOC) — Workflow Engine completo**

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
| 7.3 | **Completare Email** | `channels/email.rs`, `web/pages.rs` | ~200 | TODO |
| | IMAP polling + SMTP sending | | | |
| | HTML parsing | | | |
| | Attachment handling | | | |
| | Web UI: card Email nei settings (IMAP/SMTP/credentials) | | | |
| 7.4 | **WhatsApp stabilizzazione** | `channels/whatsapp.rs` | ~100 | TODO |
| | Reconnect robusto | | | |
| | Group support | | | |

**Stima totale Sprint 7: ~600 LOC**

---

## Sprint 8 — Hardening (P2)

> Obiettivo: produzione-ready

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| 8.1 | **CI Pipeline** | `.github/workflows/ci.yml` | ~80 | ✅ DONE |
| | cargo fmt, clippy, test | | | |
| | Multi-feature matrix | | | |
| | Release binaries | | | |
| 8.2 | **Tool abort/timeout** | `agent/agent_loop.rs`, `config/schema.rs` | ~30 | ✅ DONE |
| | Generic timeout wrapper in agent loop (tokio::select!) | | | |
| | Per-tool timeout override via config | | | |
| | Default 120s, 0 = disable | | | |
| 8.3 | **Provider health monitoring** | `provider/health.rs` (nuovo), `provider/reliable.rs` | ~220 | ✅ DONE |
| | Circular buffer circuit breaker (WINDOW_SIZE=20) | | | |
| | Auto-skip Down providers (>80% error rate) | | | |
| | EMA latency tracking, REST API `/api/v1/providers/health` | | | |
| 8.4 | **E-Stop** | `security/estop.rs` (nuovo), `web/api.rs` | ~110 | ✅ DONE |
| | Kill agent loop, network offline, browser close | | | |
| | MCP shutdown, subagent cancel | | | |
| | Web UI button + resume endpoint | | | |
| 8.5 | **Service install** | `service/launchd.rs`, `service/systemd.rs` | ~200 | ✅ DONE |
| | `homun service install` (macOS/Linux) | | | |
| | Auto-start on boot | | | |

**Stima totale Sprint 8: ~540 LOC**

---

## Programma Trasversale — Skill Runtime Parity (P0/P1)

> Obiettivo: portare il runtime skill a parita 1:1 con ClawHub/OpenClaw.
> Se le skill ClawHub funzionano su Homun, anche Felix (business autopilot) diventa una skill installabile.
> Riferimento: `~/Projects/openclaw/src/agents/skills/` per implementazione OpenClaw.

### Contesto

OpenClaw ha un sistema skill maturo con:
- **Eligibility gating** a load-time (bins, env, config, os)
- **Invocation policy** (user-invocable, disable-model-invocation)
- **Tool policy** runtime (allow/deny per agent/context, hard enforcement)
- **Env/secret injection** per skill (apiKey → process.env)
- **Security scanner** pre-install (static analysis for suspicious patterns)
- **Lobster** (workflow DSL) — ma e' un plugin tool separato, NON parte delle skill

Homun ha gia':
- ✅ Workflow Engine (~2,310 LOC) — piu' potente di Lobster (DB, retry, resume, Web UI, cron)
- ✅ Skill Shield (security scanner pre-install)
- ✅ Sandbox unificata (Docker/native) per script skill
- ✅ Context header con path, scripts, references (SKL-1)
- ✅ Slash command dispatch `/skill-name args` (SKL-1)
- ✅ Binary dependency check con warning (SKL-1)
- ✅ Variable substitution per compatibilita Claude Code skills (SKL-1)

### Milestone

| # | Scope | Priorita | LOC stimate | Stato |
|---|-------|----------|-------------|-------|
| SKL-1 | **Context header + slash commands** | P0 | ~256 | ✅ DONE |
| | Activation header: skill dir, scripts, references, run instructions | | | |
| | Slash command `/skill-name args` → system message injection | | | |
| | `substitute_skill_variables()` ($ARGUMENTS, ${SKILL_DIR}, $USER_NAME) | | | |
| | `extract_required_bins()` + warning se mancanti | | | |
| | `list_skill_references()` + `build_skill_activation_header()` | | | |
| SKL-2 | **Eligibility gating completa** | P1 | ~100 | ✅ DONE |
| | `SkillRequirements` struct: bins, any_bins, env, config, os | | | |
| | `extract_requirements()` + `check_eligibility()` | | | |
| | `eligible: bool` su `Skill`, `check_all_eligibility()`, `list_eligible()` | | | |
| | Skill non eleggibili escluse dal prompt e tool registration | | | |
| | 5 test unitari | | | |
| SKL-3 | **Invocation policy** | P1 | ~60 | ✅ DONE |
| | `user-invocable: false` — skill nascosta da slash commands | | | |
| | `disable-model-invocation: true` — skill esclusa dal prompt LLM | | | |
| | `list_for_model()` filtra eligible + model-invocable | | | |
| | 3 test unitari | | | |
| SKL-4 | **Tool policy per-skill (hard enforcement)** | P0 | ~130 | ✅ DONE |
| | `parse_allowed_tools()` con alias mapping (Web, Bash, Read, etc.) | | | |
| | `skill_allowed_tools: Option<HashSet<String>>` in agent loop | | | |
| | Defense in depth: soft (filtra tool_defs) + hard (runtime block) | | | |
| | Skills sempre callable (bypass policy) — backward compatible | | | |
| | 5 test unitari | | | |
| SKL-5 | **Skill env/secret injection** | P1 | ~110 | ✅ DONE |
| | `SkillsConfig` + `SkillEntryConfig` in config/schema.rs | | | |
| | `resolve_skill_env()` con vault:// resolution | | | |
| | `skill_env` su `ToolContext` → iniettato in Shell subprocess | | | |
| | `execute_skill_script_with_env()` per script execution | | | |
| | 3 test unitari | | | |
| SKL-6 | **Skill audit logging** | P2 | ~80 | ✅ DONE |
| | Migration 016, `insert_skill_audit()` + `list_skill_audits()` | | | |
| | Fire-and-forget audit (tool-call + slash command) | | | |
| | API endpoint `GET /api/v1/skills/audit?limit=N` | | | |
| SKL-7 | **E2E test suite** | P1 | ~100 | ✅ DONE |
| | `test_backward_compatibility_no_new_fields` | | | |
| | `test_full_lifecycle_eligibility_and_invocation` (4 skills, policy combos) | | | |
| | `test_tool_policy_parsing_complex` | | | |
| | `test_scan_with_eligibility` (scan → eligibility → filtering) | | | |
| | 41 test totali nel modulo loader (tutti passing) | | | |

**Programma SKL completato: ~580 LOC effettive (SKL-1..7)**

### Differenze architetturali vs OpenClaw

| Aspetto | OpenClaw | Homun | Note |
|---------|---------|-------|------|
| **Caricamento skill** | LLM legge SKILL.md via `read` tool | Tool-call interception + header | Homun e' piu efficiente (1 round-trip in meno) |
| **Workflow runtime** | Lobster (DSL plugin, opzionale) | Workflow Engine (DB, retry, resume, UI) | Homun ha di piu — Lobster e' solo piping + approval |
| **Tool restriction** | Per-agent allow/deny (runtime) | `skill_allowed_tools` hard enforcement + tool_defs filtering | ✅ Parita — defense in depth (soft+hard) |
| **Secret injection** | process.env prima del turno LLM | `resolve_skill_env()` + `ToolContext.skill_env` → subprocess | ✅ Parita — vault:// resolution + env injection |
| **Security scan** | Warnings only, non blocca | Skill Shield (scan + VirusTotal + risk score) | Homun ha di piu (VirusTotal integration) |
| **Eligibility** | bins + env + config + os | bins + any_bins + env + os (`check_eligibility()`) | ✅ Parita — config skip (future) |
| **Invocation policy** | user-invocable + disable-model | user_invocable + disable_model_invocation | ✅ Parita |
| **Audit** | Event logging | `skill_audit` table + API endpoint | ✅ Parita |

---

## Feature Implementate — Non Tracciate in Sprint

> Queste feature sono state implementate durante lo sviluppo ma non erano pianificate come task espliciti.
> Documentate qui per completezza dell'inventario.

| Feature | File principali | Note |
|---------|----------------|------|
| **Approval system** | `tools/approval.rs`, `web/api.rs` (7 endpoint), `web/pages.rs` (/approvals), `static/js/approvals.js` | Tool + API + pagina Web UI dedicata per approvazione azioni semi-autonome |
| **2FA/TOTP** | `web/api.rs` (7 endpoint: setup/verify/status/disable/backup/validate/recover) | Autenticazione a due fattori per operazioni sensibili (vault, knowledge sensitive) |
| **Account management** | `web/pages.rs` (/account), `web/api.rs` | Pagina gestione account/identita' utente |
| **API tokens** | `web/api.rs` | Generazione e gestione token API per accesso programmatico |
| **Webhook ingress** | `web/api.rs` | Endpoint per ricezione webhook esterni (Stripe, GitHub, etc.) |
| **Email multi-account** | `channels/email.rs`, `tools/read_email.rs` | Supporto account multipli + tool `read_email_inbox` per LLM |
| **Exfiltration guard** | `security/mod.rs` | Filtro anti-esfiltrazione dati sensibili nelle risposte |
| **TUI (ratatui)** | `tui/app.rs`, `tui/ui.rs`, `tui/event.rs` | Interfaccia terminale interattiva alternativa al CLI |
| **Canale Web** | `channels/web.rs`, `web/ws.rs` | Chat via WebSocket nella Web UI — settimo canale |
| **E-Stop** | `security/estop.rs`, `web/api.rs` | Kill switch emergenza per agent loop, network, browser, MCP |
| **Provider health** | `provider/health.rs` | Circuit breaker, EMA latency, auto-skip provider down |

---

## BIZ — Business Autopilot (P1)

> Obiettivo: agente AI autonomo che trova nicchie, crea strategie, vende prodotti, traccia revenue, auto-corregge.
> Filosofia MCP-first: il core traccia contabilita e orchestrazione; integrazioni esterne (Stripe, PayPal, Twitter, email, fatturazione) via MCP server.

### BIZ-1: Core Engine (~2,030 LOC)

| # | Task | File principali | Stato |
|---|------|----------------|-------|
| BIZ-1.1 | **DB migration** | `migrations/015_business.sql` | ✅ DONE |
| | 6 tabelle: businesses, strategies, products, transactions, orders, insights | | |
| BIZ-1.2 | **Tipi domain** | `src/business/mod.rs` | ✅ DONE |
| | Enum status + struct Business, Strategy, Product, Transaction, Order, etc. | | |
| BIZ-1.3 | **DB operations** | `src/business/db.rs` | ✅ DONE |
| | CRUD per ogni entita + revenue_summary + budget tracking | | |
| BIZ-1.4 | **Engine** | `src/business/engine.rs` | ✅ DONE |
| | Lifecycle (launch/pause/resume/close), OODA prompt builder, budget enforcement | | |
| BIZ-1.5 | **Tool LLM** | `src/tools/business.rs` | ✅ DONE |
| | 13 azioni: launch, list, status, research, strategize, create_product, etc. | | |
| | Autonomia semi/budget/full, OnceCell late-binding | | |
| BIZ-1.6 | **Config** | `src/config/schema.rs` | ✅ DONE |
| | BusinessConfig: enabled, default_autonomy, currency, fiscal | | |
| BIZ-1.7 | **Wiring** | `src/main.rs`, `server.rs`, `gateway.rs` | ✅ DONE |
| BIZ-1.8 | **System prompt** | `src/agent/prompt/sections.rs` | ✅ DONE |
| BIZ-1.9 | **Web UI** | `src/web/pages.rs` | ✅ DONE |
| | Pagina /business con form, stats, lista, detail panel | | |
| BIZ-1.10 | **API REST** | `src/web/api.rs` | ✅ DONE |
| | 10 endpoint: list, create, get, pause, resume, close, strategies, products, transactions, revenue | | |
| BIZ-1.11 | **Frontend JS** | `static/js/business.js` | ✅ DONE |

**BIZ-1 completato: ~2,030 LOC**

### BIZ-2: Pagamenti (TODO, ~1,500 LOC)

| # | Task | Note |
|---|------|------|
| BIZ-2.1 | Payment trait | `PaymentProvider`: create_checkout, verify, webhook |
| BIZ-2.2 | Stripe | Checkout + Webhooks (anche via MCP) |
| BIZ-2.3 | PayPal | Orders API + IPN (anche via MCP) |
| BIZ-2.4 | Storefront | Landing page pubblica `/store/{slug}` |

### BIZ-3: Contabilita (TODO, ~400 LOC)

| # | Task | Note |
|---|------|------|
| BIZ-3.1 | Tracking IVA | Tax rate/amount su transactions, suggerimento aliquota per paese |
| BIZ-3.2 | Export CSV | Transazioni filtrabili per periodo/tipo + riepilogo IVA |

> NO fatture — l'utente le fa manualmente o via MCP (es. FattureInCloud, Stripe Invoicing)

### BIZ-4: Marketing Skills (TODO, ~600 LOC)

| # | Task | Note |
|---|------|------|
| BIZ-4.1 | X/Twitter skill | Post, thread, analytics (o via MCP) |
| BIZ-4.2 | Email marketing | Newsletter via SMTP/Resend (o via MCP) |

### BIZ-5: Crypto (TODO, ~1,000 LOC)

| # | Task | Note |
|---|------|------|
| BIZ-5.1 | Wallet ETH/SOL | Generazione, balance, receive monitoring |
| BIZ-5.2 | Token ERC-20/SPL | Deploy su Base/Ethereum/Solana |
| BIZ-5.3 | Crypto payments | Wallet address per pagamenti |

**Stima totale BIZ: ~5,530 LOC (BIZ-1 done, BIZ-2..5 TODO)**

---

## Programma Security Web (P0)

> Obiettivo: proteggere la Web UI e le API da accesso non autorizzato.
> Attualmente la Web UI e tutti gli endpoint API sono completamente aperti — chiunque con accesso alla porta puo' controllare l'agent, leggere il vault, cancellare dati.
> **Critico**: senza auth, il pairing cifrato della Mobile App e' inutile (il gateway e' gia' esposto).

| # | Task | File principali | LOC stimate | Stato |
|---|------|----------------|-------------|-------|
| SEC-1 | **Autenticazione Web UI** | `web/auth.rs` (nuovo), `web/server.rs`, `web/api.rs` | ~300 | TODO |
| | Login page con username/password (hash argon2) | | | |
| | Session token (JWT o cookie firmato) | | | |
| | Middleware auth su tutte le route (eccetto /login e /api/health) | | | |
| | Setup iniziale: primo utente crea credenziali | | | |
| SEC-2 | **HTTPS nativo** | `web/server.rs`, `config/schema.rs` | ~100 | TODO |
| | TLS via rustls (auto-generazione cert self-signed o Let's Encrypt) | | | |
| | Redirect HTTP → HTTPS | | | |
| | Config: `[web] tls_cert`, `tls_key`, `auto_tls` | | | |
| SEC-3 | **Rate limiting API** | `web/server.rs` | ~80 | TODO |
| | Rate limiter per IP (tower-governor o simile) | | | |
| | Limiti separati per auth endpoint (anti-brute-force) e API generiche | | | |
| | Config: `[web] rate_limit_per_minute` | | | |
| SEC-4 | **API key auth per accesso programmatico** | `web/auth.rs`, `web/api.rs`, `storage/db.rs` | ~120 | TODO |
| | Header `Authorization: Bearer <token>` per API REST | | | |
| | CRUD token da Web UI (/account) | | | |
| | Scoping opzionale (read-only, admin) | | | |

**Stima totale Security Web: ~600 LOC**

---

## Programma Mobile App — Homun Companion (P2)

> Obiettivo: app nativa iOS/Android che offre un'esperienza personalizzata rispetto ai canali generici (Telegram, Discord).
> Telegram funziona ma un'app dedicata consente risposte personalizzate, interazioni ricche, e UX su misura.

### Perche' un'app dedicata

- **UX personalizzata**: risposte formattate (markdown rendering, code blocks, grafici inline), non limitate al formato Telegram
- **Interazioni ricche**: bottoni inline contestuali, form, approval gates visivi, notifiche push granulari
- **Vault sicuro via pairing**: pairing crittografato diretto con l'istanza Homun — i secret vengono mostrati in chiaro nell'app senza bisogno di OTP/PIN, perche' il canale e' gia' cifrato end-to-end
- **Dashboard mobile**: stats business, revenue, workflow status, memoria — tutto accessibile dal telefono
- **Allegati nativi**: foto, documenti, audio direttamente dalla camera/gallery con pipeline ottimizzata
- **Offline cache**: ultime conversazioni consultabili anche senza rete

### Architettura

```
App (Flutter / Dart)
       │
       ├── WebSocket (streaming real-time)
       ├── REST API (gia' esistente: /api/v1/*)
       └── Pairing cifrato
               │
               ▼
       Homun Gateway
               │
               ├── Channel "app" (nuovo canale in src/channels/)
               └── Vault: secret visibili in chiaro via canale cifrato
```

### APP-1: Fondazioni (~1,200 LOC app + ~200 LOC Rust)

| # | Task | Note |
|---|------|------|
| APP-1.1 | **Pairing sicuro** | QR code / deep link → scambio chiavi (X25519 o simile), sessione cifrata |
| APP-1.2 | **Channel "app"** | Nuovo canale `src/channels/app.rs` — WebSocket + push notification routing |
| APP-1.3 | **Chat base** | Invio/ricezione messaggi, streaming, markdown rendering |
| APP-1.4 | **Push notifications** | FCM (Android) + APNs (iOS) per risposte, approval gate, alert |

### APP-2: Esperienza Ricca (~800 LOC app)

| # | Task | Note |
|---|------|------|
| APP-2.1 | **Vault mobile** | Visualizzazione secret in chiaro (pairing cifrato = trusted), generazione token |
| APP-2.2 | **Dashboard** | Stats business, revenue, workflow, memoria — mobile-first |
| APP-2.3 | **Approval inline** | Bottoni approve/deny per workflow e azioni semi-autonome |
| APP-2.4 | **Allegati nativi** | Camera, gallery, file picker → upload + RAG ingestion |

### APP-3: Polish (~400 LOC app)

| # | Task | Note |
|---|------|------|
| APP-3.1 | **Offline cache** | Conversazioni recenti consultabili offline |
| APP-3.2 | **Biometric lock** | FaceID / fingerprint per accesso app e vault |
| APP-3.3 | **Widget** | iOS widget / Android widget con stats rapide |

**Stima totale APP: ~2,600 LOC (app + backend)**

---

## Sprint 9+ — Future (P3)

| Task | Priorita | Note |
|------|----------|------|
| Extended thinking (Anthropic) | P2 | Claude --thinking mode |
| Prometheus metrics | P2 | Per monitoring infra |
| Voice (Whisper STT + TTS) | P2 | Input/output vocale |
| Signal channel | P3 | signal-cli bridge |
| Matrix channel | P3 | matrix-sdk-rs |
| ~~Lobster-style workflows~~ | ~~P3~~ | ✅ Implementato come Workflow Engine |
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
Sprint 3: Sicurezza Canali (P1)             ✅ DONE (~295 LOC)
  3.1 DM Pairing
  3.2 Mention gating
  3.3 Typing indicators
    |
Sprint 4: Web UI + Automations (P1)        ✅ DONE (~1,200 LOC)
  ✅ 4.1-4.6 Automations + logs + usage/costi + setup wizard
    |
Sprint 5: Ecosistema (P1)                  ✅ DONE (~1,350 LOC)
  ✅ 5.1 MCP Setup Guidato (catalogo + guided install + auto-discovery + Google/GitHub OAuth)
  ✅ 5.2 Skill Creator (agente)
  ✅ 5.3 Creazione automation da chat
  ✅ 5.4 Skill Adapter (ClawHub → Homun)
  ✅ 5.5 Skill Shield (sicurezza pre-install)
    |
Programma Sandbox Trasversale (P0/P1)      ⚠️ PARTIAL
  ✅ SBX-1 Fondazioni unificate (Shell/MCP/Skills + API/UI)
  TODO SBX-2 Linux hardened backend
  TODO SBX-3 Windows backend
  ⚠️ SBX-4 Runtime image + lifecycle
  ✅ SBX-5 UX finale Permissions/Sandbox
  TODO SBX-6 E2E cross-platform hardening
    |
Programma Chat Web UI (P1)                 ⚠️ PARTIAL
  ✅ CHAT-1 Refresh UI/UX base
  ✅ CHAT-2 Run in-memory con resume/background dopo page switch
  ✅ CHAT-3 Sessioni multiple vere
  ✅ CHAT-4 Persistenza run su DB
  ✅ CHAT-5 Composer + completo + routing multimodale
  ✅ CHAT-6 Stop profondo / cancel propagation
  TODO CHAT-7 Test E2E chat
  ⚠️ Hardening multimodale documenti / OCR / MCP fallback policy
    |
Programma Browser Automation (P1)          ✅ DONE
  ✅ Migrazione da custom sidecar a MCP (@playwright/mcp)
  ✅ Tool unificato "browser" (~17 azioni, schema piatto)
  ✅ Stealth anti-bot (addInitScript: webdriver, chrome, plugins)
  ✅ Snapshot compaction (compact_tree, agent-browser style)
  ✅ Orchestrazione (auto-snapshot, stability, autocomplete, veto)
  ⬚ Stealth avanzato (CDP endpoint, Chrome flags)
  ⬚ Screenshot/vision fallback
  ⬚ Test E2E browser
    |
Programma Design System (P1)               ✅ DONE
  ✅ Olive Moss Console — token architecture (light + dark)
  ✅ Accent picker (4 preset + custom color con derivazione HSL)
  ✅ Semantic color tokenization (ok/warn/err/info + text-on-accent)
  ✅ Typography (Geist + Plus Jakarta Sans)
    |
Sprint 6: RAG Knowledge Base (P1)          ✅ COMPLETE (~2,830 LOC)
  ✅ 6.1-6.10 Core RAG (DB, chunker, engine, tool, config, startup, agent loop, API, UI, Telegram)
  ✅ 6.11 Formati avanzati (PDF, DOCX, XLSX) — parsers.rs
  ✅ 6.12 Indicizzazione cartelle (Web UI + CLI) — Knowledge subcommand
  ✅ 6.13 Vault-gated access per dati sensibili + 2FA — sensitive.rs, reveal endpoint
  ✅ 6.14 Directory watcher (auto-ingest) — watcher.rs, notify crate
  ✅ 6.15 Sorgenti cloud via MCP (framework) — cloud.rs, CloudSync
    |
Programma Workflow Engine (P1)             ✅ DONE (~2,310 LOC)
  ✅ WF-1 Schema DB + tipi (workflows + workflow_steps)
  ✅ WF-2 DB layer (CRUD, status, context, resume)
  ✅ WF-3 Engine orchestratore (step runner, approval, retry, resume-on-boot)
  ✅ WF-4 Tool LLM (create/list/status/approve/cancel)
  ✅ WF-5 Wiring gateway (init, event loop, auto-resume)
  ✅ WF-6 Web UI workflows (pagina, API, JS, CSS)
  ✅ WF-7 Trigger da automazioni/cron (OnceCell, migration 014, step builder)
    |
Sprint 7: Canali Phase 2 (P2)              TODO (~600 LOC)
  7.1-7.4 Discord, Slack, Email, WhatsApp
    |
Sprint 8: Hardening (P2)                   ✅ COMPLETE (~360 LOC)
  ✅ 8.1 CI Pipeline
  ✅ 8.2 Tool timeout (generic wrapper in agent loop)
  ✅ 8.3 Provider health monitoring (circuit breaker + REST API)
  ✅ 8.4 E-Stop (kill switch + Web UI button)
  ✅ 8.5 Service install
    |
BIZ: Business Autopilot (P1)               ⚠️ PARTIAL
  ✅ BIZ-1 Core Engine (DB, tipi, engine, tool, config, wiring, web UI, API, JS)
  TODO BIZ-2 Pagamenti (Stripe, PayPal, storefront)
  TODO BIZ-3 Contabilita (tracking IVA, export CSV)
  TODO BIZ-4 Marketing (X/Twitter, Email — skills o MCP)
  TODO BIZ-5 Crypto (wallet, token deploy, pagamenti)
    |
Programma Skill Runtime Parity (P0/P1)   ✅ COMPLETE (~580 LOC)
  ✅ SKL-1 Context header + slash commands + bins check + variable substitution
  ✅ SKL-2 Eligibility gating (env, any_bins, os, check_eligibility)
  ✅ SKL-3 Invocation policy (user-invocable, disable-model-invocation, list_for_model)
  ✅ SKL-4 Tool policy per-skill (parse_allowed_tools, hard enforcement, defense in depth)
  ✅ SKL-5 Skill env/secret injection (SkillsConfig, vault://, ToolContext.skill_env)
  ✅ SKL-6 Skill audit logging (migration 016, fire-and-forget, API endpoint)
  ✅ SKL-7 E2E test suite (41 test nel modulo loader, tutti passing)
    |
Programma Security Web (P0)              TODO (~600 LOC)
  TODO SEC-1 Autenticazione Web UI (login, session, middleware)
  TODO SEC-2 HTTPS nativo (rustls, auto-cert)
  TODO SEC-3 Rate limiting API
  TODO SEC-4 API key auth per accesso programmatico
    |
Programma Mobile App (P2)                 TODO (~2,600 LOC)
  TODO APP-1 Fondazioni (pairing, channel, chat, push)
  TODO APP-2 Esperienza ricca (vault mobile, dashboard, approval, allegati)
  TODO APP-3 Polish (offline, biometric, widget)
    |
Sprint 9+: Future (P3)
  Voice, Extended thinking, Prometheus, distribuzione
```

**Completato: Sprint 1-6 + Sprint 8 + SBX-1/5 + CHAT-1..6 + Browser + Design System + Workflow Engine + BIZ-1 + SKL-1..7 + feature orfane (approval, 2FA, account, e-stop, health, TUI, etc.)**
**Rimanente: Security Web (P0), SBX-2..4/6, CHAT-7, Browser E2E, Sprint 7, BIZ-2..5, Mobile App, Sprint 9+**

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
| [agent-browser.dev](https://github.com/vercel-labs/agent-browser) | Riferimento browser: compact_tree, snapshot, architettura |

---

## Vantaggi Competitivi Homun

1. **MCP client nativo** — ne OpenClaw ne ZeroClaw
2. **RAG Knowledge Base personale** — ne OpenClaw ne ZeroClaw hanno ingestion + ricerca ibrida sui documenti utente
3. **Browser via MCP Playwright** — tool unificato con stealth anti-bot, compact_tree, auto-snapshot
4. **Exfiltration filter** — OpenClaw non ce l'ha
5. **Business Autopilot** — agente autonomo per business con OODA loop, budget enforcement, MCP-first
6. **Web UI ricca** — 17 pagine embedded + design system proprietario con accent picker
7. **Skill ecosystem** — ClawHub + OpenSkills + hot-reload
8. **Mobile App con pairing cifrato** — vault secret in chiaro via canale sicuro, UX personalizzata oltre Telegram
9. **Single binary Rust** — ~50MB, no runtime
10. **XML fallback auto** — supporta modelli senza function calling
11. **Prompt modulare** — sezioni componibili per mode
12. **Browser per modelli deboli** — ref normalization, schema piatto, orchestrazione automatica
