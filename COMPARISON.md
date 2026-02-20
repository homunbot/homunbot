# Homun — Analisi Competitiva

> Ultimo aggiornamento: 2026-02-17
> Stato: Homun Phase 5+ (14.449 LOC, 161 test, 47 file sorgente)

## Panoramica

| | **Homun** | **Nanobot** | **TinyClaw** | **OpenClaw** |
|---|---|---|---|---|
| **Linguaggio** | **Rust** | Python | TS/JS + Bash | TS/JS (monorepo) |
| **LOC** | **14.449** | ~10.161 | ~2.800 | 430.000+ |
| **GitHub Stars** | nuovo | nuovo (~2 settimane) | 1.934 | 180.000+ |
| **Binary** | **Single binary, 0 deps** | Python + pip | Node.js | Node.js 22+ pnpm |
| **Test** | **161** | non dichiarati | minimali | suite completa |
| **Licenza** | MIT | MIT | MIT | MIT |
| **Approccio** | Privacy-first, skill-powered | Lightweight personal agent | Multi-agent framework | Full-featured enterprise |

**Nota:** Il creatore di OpenClaw (Peter Steinberger) e' entrato in OpenAI il 15 feb 2026. Il progetto continuera' come fondazione open source supportata da OpenAI.

---

## Supporto Canali

| Canale | Homun | Nanobot | TinyClaw | OpenClaw |
|---------|:--------:|:------:|:--------:|:--------:|
| CLI | ✅ | ✅ | ✅ | ✅ |
| Telegram | ✅ | ✅ | ✅ | ✅ |
| WhatsApp | ✅ (nativo Rust) | ✅ | ✅ | ✅ (Baileys JS) |
| Discord | ✅ | ✅ | ✅ | ✅ |
| Slack | — | ✅ | — | ✅ |
| Email (IMAP/SMTP) | — | ✅ | — | ✅ (Gmail Pub/Sub) |
| Feishu/Lark | — | ✅ | — | — |
| DingTalk | — | ✅ | — | — |
| QQ | — | ✅ | — | — |
| Mochat (Claw IM) | — | ✅ | — | — |
| Signal | — | — | — | ✅ (signal-cli) |
| iMessage | — | — | — | ✅ (BlueBubbles) |
| Microsoft Teams | — | — | — | ✅ |
| Matrix | — | — | — | ✅ |
| Google Chat | — | — | — | ✅ |
| Zalo | — | — | — | ✅ |
| WebChat | — | — | — | ✅ |
| **Web UI** | **🔜 (in sviluppo)** | — | — | — |
| **Totale** | **4** | **10** | **3** | **13** |

**Analisi:** OpenClaw domina con 13 canali (inclusi Teams, Signal, iMessage, Zalo). Nanobot sulle piattaforme asiatiche. Homun copre i 4 canali personali principali + web UI in arrivo. WhatsApp e' implementato nativamente in Rust — unico progetto senza bridge JS.

---

## Provider LLM

| Provider | Homun | Nanobot | TinyClaw | OpenClaw |
|----------|:--------:|:------:|:--------:|:--------:|
| Anthropic (nativo) | ✅ | ✅ | ✅ | ✅ |
| OpenAI | ✅ | ✅ | ✅ | ✅ |
| OpenRouter | ✅ | ✅ | — | — |
| Ollama (locale) | ✅ | ✅ | — | — |
| DeepSeek | ✅ | ✅ | — | — |
| Groq | ✅ | ✅ | — | — |
| Gemini | ✅ | ✅ | — | — |
| Minimax | ✅ | ✅ | — | — |
| AiHubMix | ✅ | ✅ | — | — |
| DashScope (Qwen) | ✅ | ✅ | — | — |
| Moonshot (Kimi) | ✅ | ✅ | — | — |
| Zhipu (GLM) | ✅ | ✅ | — | — |
| vLLM (locale) | ✅ | ✅ | — | — |
| Custom endpoint | ✅ | ✅ | — | — |
| OpenAI Codex (OAuth) | — | ✅ | — | ✅ |
| LiteLLM (100+ modelli) | — | ✅ | — | — |
| **Totale** | **14** | **15+** | **2** | **2** |

**Analisi:** Homun implementa tool calling nativo per ogni provider (nessuna dipendenza LiteLLM). OpenClaw supporta solo 2 provider ma quelli piu' usati (Anthropic + OpenAI). Il nostro vantaggio: Ollama-native per operazione 100% offline.

---

## Tool

| Tool | Homun | Nanobot | TinyClaw | OpenClaw |
|------|:--------:|:------:|:--------:|:--------:|
| Shell/Exec | ✅ | ✅ | ✅ | ✅ |
| File read | ✅ | ✅ | ✅ | ✅ |
| File write | ✅ | ✅ | ✅ | ✅ |
| File edit | ✅ | ✅ | — | ✅ |
| List directory | ✅ | ✅ | — | ✅ |
| Web search | ✅ (Brave) | ✅ (Brave/Tavily) | — | ✅ |
| Web fetch | ✅ | ✅ | — | ✅ |
| Cron scheduler | ✅ | ✅ | — | ✅ |
| Spawn subagent | ✅ | ✅ | ✅ | — |
| Send message | ✅ | ✅ | — | ✅ |
| MCP tool wrapper | ✅ | ✅ | — | — |
| Browser control (CDP) | — | — | — | ✅ |
| Webhook | — | — | — | ✅ |
| Gmail Pub/Sub | — | — | — | ✅ |
| Node actions (camera, screen) | — | — | — | ✅ |
| **Totale** | **11** | **12** | **3** | **13+** |

---

## Feature a Confronto

| Feature | Homun | Nanobot | TinyClaw | OpenClaw |
|---------|:--------:|:------:|:--------:|:--------:|
| Agent loop (ReAct) | ✅ | ✅ | ✅ | ✅ |
| Memory consolidation (LLM) | ✅ | ✅ | ✅ | ✅ |
| Heartbeat (wake-up proattivo) | ✅ | ✅ | ✅ | — |
| Cron scheduler | ✅ | ✅ | — | ✅ |
| Subagent (task in background) | ✅ | ✅ | ✅ | — |
| Agent Skills (open spec) | ✅ | ✅ | — | ✅ (51+ bundled) |
| ClawHub marketplace | ✅ (client) | — | — | ✅ (3.286 skill) |
| Skill installer (GitHub) | ✅ | ✅ | — | ✅ |
| Skill executor (scripts) | ✅ | ✅ | — | ✅ |
| Skill hot-reload (file watcher) | ✅ | — | — | — |
| Bootstrap files (SOUL/USER.md) | ✅ | ✅ | — | — |
| MCP protocol (client) | ✅ | ✅ | — | — |
| **TUI dashboard (ratatui)** | **✅** | — | — | — |
| **WhatsApp nativo (no bridge)** | **✅** | — | — | — |
| **Single binary** | **✅** | — | — | — |
| **Ollama-native (LLM locale)** | **✅** | ✅ | — | — |
| **Type-safe (compile time)** | **✅** | — | — | — |
| **Shell sandboxing** | **✅** | parziale | — | parziale |
| Web UI/Dashboard | **🔜** | — | — | — |
| Browser control (CDP) | **🔜** | — | — | ✅ |
| Voice (Whisper) | — | ✅ | — | ✅ (ElevenLabs) |
| Visual Canvas (A2UI) | — | — | — | ✅ |
| Mobile app (iOS/Android) | — | — | — | ✅ |
| Multi-agent routing | — | — | — | ✅ |
| OAuth provider login | — | ✅ | — | ✅ |
| Pairing/security mode (DM) | — | — | — | ✅ |

---

## Architettura a Confronto

### Homun (Rust, 14.449 LOC)
```
src/
├── agent/        # Core: loop, context, gateway, heartbeat, memory, subagent
├── bus/          # Message bus (mpsc channels)
├── channels/     # CLI + Telegram + Discord + WhatsApp (nativo Rust)
├── config/       # Config TOML + dotpath editing, 14 provider
├── provider/     # OpenAI-compatible + Anthropic nativo
├── scheduler/    # Cron custom (every/cron/at)
├── session/      # SQLite session management
├── skills/       # Loader + Installer (GitHub+ClawHub) + Executor + Watcher
├── storage/      # SQLite (sessions, messages, memories, cron)
├── tools/        # 11 tool (shell, file×4, web×2, cron, spawn, message, MCP)
├── tui/          # TUI interattiva ratatui (settings, providers, whatsapp, skills, MCP)
└── web/          # 🔜 Web UI (axum + htmx, dashboard + skill manager)
```

### OpenClaw (TS/JS, 430K+ LOC)
```
openclaw/          # pnpm monorepo, Node.js 22+
├── packages/
│   ├── clawdbot/  # Core engine
│   └── moltbot/   # Estensioni
├── skills/        # 51 skill bundled
├── gateway/       # WebSocket control plane (ws://127.0.0.1:18789)
├── canvas/        # A2UI visual workspace (porta 18793)
├── clients/       # CLI, macOS, iOS, Android, Web
└── tests/         # Unit, integration, e2e
```

### Nanobot (Python, ~10K LOC)
```
nanobot/
├── agent/        # loop, context, memory, skills, subagent, tools/
├── channels/     # 10 implementazioni canale
├── providers/    # Registry + implementazioni
├── cli/          # CLI basata su typer
├── config/       # Config Pydantic
├── cron/         # APScheduler-based
├── heartbeat/    # Servizio proattivo file-based
├── session/      # Sessioni conversazione
├── bus/          # Event bus + queue
└── skills/       # 7 skill bundled
```

---

## Dove Homun Vince

1. **Single Rust binary** — Zero dipendenze, startup <50ms, ~10MB, deploy ovunque (Raspberry Pi, VPS, container scratch, embedded)
2. **14 provider con tool calling nativo** — NON dipende da LiteLLM; ogni provider ha implementazione funzionante. OpenClaw ne supporta solo 2
3. **WhatsApp nativo Rust** — Libreria `whatsapp-rust` vendored, connessione diretta a WhatsApp Web senza bridge Node.js
4. **TUI + Web dashboard** — Pannello di controllo dual-mode: TUI (ratatui) per server headless, Web UI per esperienza visuale completa
5. **Type safety** — Errori catturati a compile time vs runtime. `Result<T>` ovunque
6. **Performance** — ~5MB RAM idle vs ~200MB (OpenClaw). Nessuna GC pause. Ordini di grandezza migliore per processi long-running
7. **161 test** — Suite di test robusta dal primo giorno
8. **Shell sandboxing** — Filtri sofisticati (fork bomb, rm -rf, pipe injection, base64 obfuscation)
9. **SQLite storage** — Persistenza affidabile con migrazioni embedded
10. **Privacy-first** — Tutto locale di default, Ollama-native per operazione completamente offline
11. **Sicurezza skill** — Dopo l'incidente ClawHavoc (feb 2026, 2.419 skill malevole rimosse da ClawHub), il nostro approccio "skill verificate + standard aperto" e' piu' sicuro di un marketplace aperto

## Dove Homun e' Indietro

| Gap | Priorita' | Sforzo | Note |
|-----|-----------|--------|------|
| ~~MCP protocol~~ | ~~ALTA~~ | ~~Medio~~ | ✅ Implementato (rmcp, stdio transport) |
| ~~Discord channel~~ | ~~MEDIA~~ | ~~Basso~~ | ✅ Implementato (serenity) |
| ~~WhatsApp channel~~ | ~~MEDIA~~ | ~~Medio~~ | ✅ Implementato (nativo Rust) |
| ~~MessageTool~~ | ~~MEDIA~~ | ~~Basso~~ | ✅ Implementato (send_message) |
| ~~ClawHub client~~ | ~~ALTA~~ | ~~Medio~~ | ✅ Implementato (search + install) |
| **Web UI/Dashboard** | **CRITICA** | **Alto** | La UX e' il gap piu' grande. Serve dashboard web per config, skill, monitoring |
| **Browser control (CDP)** | **ALTA** | Medio | Killer feature per automazione web. Crate: `chromiumoxide` |
| **Skill ecosystem** | **ALTA** | Alto | 2 bundled vs 51+ (OpenClaw). Servono 10+ skill di qualita' |
| **Deploy semplificato** | **ALTA** | Medio | Dockerfile, GitHub Actions, `curl \| sh` installer |
| **Canali** (Slack, Email) | MEDIA | Basso | Crate Rust esistenti (slack-morphism, lettre + imap) |
| **Voice (Whisper)** | MEDIA | Medio | API Groq per messaggi vocali Telegram |
| **Webhook/API** | MEDIA | Basso | REST API per integrazioni esterne |
| **Multi-agent routing** | MEDIA | Medio | Session-based agent isolation |
| Visual Canvas (A2UI) | BASSA | Alto | Non core per personal agent |
| Mobile app | BASSA | Alto | TG/WA sono le nostre "app" |

---

## Roadmap — Phase 6: UX & Polish

### Sprint 1: Web UI Foundation (CRITICA)
> L'obiettivo: rendere Homun accessibile quanto OpenClaw, ma con la leggerezza di un singolo binario

1. **Web server embedded** — axum integrato nel binary, porta 18080
2. **Dashboard home** — stato agente, sessioni attive, canali connessi, risorse
3. **Skill manager UI** — browse ClawHub, install one-click, gestisci skill locali, hot-reload live
4. **Config wizard** — setup guidato: provider, canali, tool. Zero editing TOML manuale
5. **Chat UI** — interfaccia web per chattare con l'agente (WebSocket)
6. **Log viewer** — streaming log real-time con filtri per livello/modulo

**Stack tecnico:** axum + tower + htmx + TailwindCSS. Server-side rendering, zero JS framework. Il CSS e le pagine sono embedded nel binary (rust-embed). L'utente apre `http://localhost:18080` e ha tutto.

### Sprint 2: Browser Control & Tool Power
7. **Browser tool (CDP)** — navigazione, click, screenshot, estrazione dati
8. **Webhook tool** — ricevi/invia webhook HTTP per integrazioni
9. **REST API** — `/api/v1/chat`, `/api/v1/skills`, `/api/v1/config` per integrazioni programmatiche

### Sprint 3: Canali & Voice
10. **Slack channel** — Socket Mode via `slack-morphism`
11. **Email channel** — IMAP receive + SMTP send via `lettre` + `async-imap`
12. **Voice transcription** — Groq Whisper per messaggi vocali Telegram

### Sprint 4: Skill Ecosystem
13. **10 skill bundled** — daily-briefing, market-monitor, email-digest, habit-tracker, reading-list, weather, github-notify, system-monitor, expense-tracker, note-taker
14. **Skill creator wizard** — genera SKILL.md + scripts/ da descrizione testuale
15. **Skill testing** — `homun skills test <name>` per validare skill localmente

### Sprint 5: Deploy & Distribution
16. **Dockerfile** — `FROM scratch` + binary. Container <15MB
17. **GitHub Actions** — build multi-arch (linux-x86_64, linux-aarch64, macos-x86_64, macos-aarch64)
18. **Installer script** — `curl -fsSL https://homun.dev/install | sh`
19. **Homebrew formula** — `brew install homun`
20. **crates.io** — `cargo install homun`

---

## Positioning Statement

> **Homun e' l'unico agente personale AI single-binary, privacy-first e skill-powered.**
> Scritto in Rust, compila in un singolo eseguibile con zero dipendenze.
> Supporta lo standard aperto Agent Skills e il marketplace ClawHub (3.286+ skill),
> funziona con 14 provider LLM (incluso funzionamento completamente offline via Ollama),
> e si gestisce via Web UI, Telegram, WhatsApp (nativo Rust), Discord, o TUI.
> Include un'interfaccia web per configurazione, gestione skill, e monitoring.
> Progettato per sviluppatori e power user
> che vogliono un assistente AI che gira 24/7 in modo affidabile sul proprio hardware.

### La Nicchia

- **OpenClaw** = il re della categoria. 180K+ stars, ClawHub (3.286 skill), creatore entrato in OpenAI. Il progetto da battere. Ma: 430K LOC, Node.js, ~200MB RAM, incidente ClawHavoc (2.419 skill malevole)
- **Nanobot** = agente Python leggero (stessa filosofia, richiede Python runtime, no tool calling nativo)
- **TinyClaw** = framework multi-agente (sperimentale, meno feature)
- **Homun** = **agente Rust single-binary per chi vuole il controllo totale**. Un binario, il tuo hardware, i tuoi dati. Web UI per la UX, TUI per i puristi, 14 provider, privacy assoluta

---

## Numeri

| Metrica | Homun | Nanobot | TinyClaw | OpenClaw |
|---------|----------|--------|----------|----------|
| File sorgente | 47 | ~50 | ~15 | 500+ |
| Linee di codice | 14.449 | ~10.161 | ~2.800 | 430.000+ |
| Dipendenze | ~40 (Rust crates) | ~26 (Python) | ~10 (npm) | 200+ (npm) |
| Dimensione binary | ~10MB (release) | N/A (interpretato) | N/A | N/A |
| Runtime richiesto | Nessuno | Python 3.11+ | Node.js 18+ | Node.js 22+ |
| Tempo startup | <50ms | ~2s | ~1s | ~5s |
| Memoria (idle) | ~5MB | ~50MB | ~30MB | ~200MB |
| Provider | 14 | 15+ | 2 | 2 |
| Canali | 4 (+web UI) | 10 | 3 | 13 |
| Tool | 11 | 12 | 3 | 13+ |
| Skill bundled | 2 | 7 | 0 | 51 |
| ClawHub skill | 3.286 (client) | — | — | 3.286 (nativo) |
| Test | 161 | — | — | ampia suite |

---

## Nanobot — Dettaglio Tecnico

### Punti di forza
- **10 canali** incluse piattaforme cinesi (Feishu, DingTalk, QQ, Mochat)
- **MCP support** aggiunto a febbraio 2026
- **Provider registry pattern** — aggiungere un provider richiede solo 2 step
- **LiteLLM integration** — interfaccia unificata per 100+ modelli (ma senza tool calling)
- **7 skill bundled** — Memory, Summarize, Skill Creator, GitHub, Tmux, Weather, Cron
- **Groq voice transcription** — trascrizione automatica messaggi vocali via Whisper

### Cosa Homun fa meglio
1. **Nessun Python** — nanobot richiede Python 3.11+, pip, e 26 dipendenze
2. **Tool calling nativo** — nanobot usa LiteLLM che non supporta tool calling
3. **WhatsApp nativo Rust** — zero dipendenze esterne
4. **TUI + Web dashboard** — nanobot non ha interfaccia grafica
5. **SQLite vs JSON** — persistenza affidabile vs fragile
6. **Type safety** — compile time vs runtime
7. **Concorrenza reale** — tokio con parallelismo vs asyncio single-threaded
8. **161 test** — nanobot non dichiara test
9. **Shell sandboxing** — filtri sofisticati

---

## OpenClaw — Il Progetto da Battere

OpenClaw e' il benchmark della categoria: 180K+ stars, ecosistema maturo, community enorme. Il creatore Peter Steinberger e' entrato in OpenAI (feb 2026), il progetto sara' supportato come fondazione open source.

### Punti di forza
- **180K+ stars** — il progetto piu' popolare nella categoria
- **ClawHub marketplace** — 3.286 skill con 1.5M download. Il vero vantaggio competitivo
- **51+ skill bundled** — produttivita', media, automazione
- **Deploy semplificato** — Docker, Nix, one-click su Railway/Render
- **Visual Canvas (A2UI)** — workspace visuale agent-driven (porta 18793)
- **Voice Wake + Talk Mode** — ElevenLabs integration
- **App mobile** (iOS/Android) — azioni device-specific
- **Browser control** — Chrome DevTools Protocol (CDP)
- **13 canali** — inclusi iMessage, Signal, Teams, Matrix, Zalo
- **Gateway WebSocket** — control plane su ws://127.0.0.1:18789
- **Multi-agent** — session routing, agent-to-agent coordination
- **Pairing mode** — sicurezza DM con codice approvazione

### Vulnerabilita' di OpenClaw
- **430K LOC** — complessita' enorme, difficile da mantenere/contribuire
- **Node.js 22+** — runtime pesante, ~200MB RAM idle
- **ClawHavoc** (feb 2026) — 2.419 skill malevole scoperte nel marketplace. Problema di sicurezza strutturale
- **Creatore uscito** — Peter Steinberger in OpenAI. Il progetto dipendera' dalla community
- **Prompt injection** — CrowdStrike ha segnalato rischi di sicurezza significativi

### Dove Homun si differenzia

| | OpenClaw | Homun |
|---|---|---|
| Target | Tutti (accessibile) | Sviluppatori/power user |
| Deploy | Docker, one-click cloud | Single binary + `curl \| sh` |
| Complessita' | 430K LOC, pnpm, monorepo | 14.4K LOC, single crate |
| Runtime | Node.js 22+ | **Nessuno** |
| Skill ecosystem | ClawHub (3.286, ma con rischi sicurezza) | Agent Skills spec + ClawHub client (curato) |
| Privacy | Cloud-oriented | **Local-first, Ollama-native** |
| Risorse | ~200MB RAM | **~5MB RAM** |
| WhatsApp | Baileys (JS) | **Nativo Rust (zero deps)** |
| UI | Web + mobile + Canvas | **Web UI + TUI** |
| Sicurezza skill | Marketplace aperto (ClawHavoc) | **Skill verificate + sandboxing** |
| Provider LLM | 2 (Anthropic + OpenAI) | **14 con tool calling nativo** |
