# Homun — Analisi Competitiva

> Ultimo aggiornamento: 2026-02-23
> Stato: Homun Phase 6+ (28.318 LOC, 211 test, 71 file sorgente)

## Panoramica

| | **Homun** | **ZeroClaw** | **Nanobot** | **OpenClaw** |
|---|---|---|---|---|
| **Linguaggio** | **Rust** | **Rust** | Python | TS/JS (monorepo) |
| **LOC** | **28.318** | Medium | ~10.161 | 430.000+ |
| **GitHub Stars** | nuovo | growing | nuovo | 180.000+ |
| **Binary** | **47MB, 0 deps** | **3-8MB, 0 deps** | Python + pip | Node.js 22+ pnpm |
| **Test** | **211** | ? | non dichiarati | suite completa |
| **Licenza** | MIT | Open source | MIT | MIT |
| **Approccio** | Privacy-first, Web UI embedded | Edge/embedded, swap anything | Lightweight personal agent | Full-featured enterprise |

**Nota:** Il creatore di OpenClaw (Peter Steinberger) e' entrato in OpenAI il 15 feb 2026. ZeroClaw e' il nostro competitor piu' diretto: stesso linguaggio, stesso obiettivo, ma senza Web UI.

---

## Supporto Canali

| Canale | Homun | ZeroClaw | Nanobot | OpenClaw |
|---------|:--------:|:------:|:------:|:--------:|
| CLI | ✅ | ✅ | ✅ | ✅ |
| **Web UI** | **✅ Completa** | — | — | ✅ |
| Telegram | ✅ | ✅ | ✅ | ✅ |
| WhatsApp | ✅ (nativo Rust) | ✅ | ✅ | ✅ (Baileys JS) |
| Discord | ✅ | ✅ | ✅ | ✅ |
| Slack | — | ✅ | ✅ | ✅ |
| Email (IMAP/SMTP) | — | ✅ | ✅ | ✅ (Gmail Pub/Sub) |
| Matrix | — | ✅ | — | ✅ |
| Signal | — | ✅ | — | ✅ (signal-cli) |
| iMessage | — | ✅ | — | ✅ (BlueBubbles) |
| Feishu/Lark | — | — | ✅ | — |
| DingTalk | — | — | ✅ | — |
| QQ | — | — | ✅ | — |
| Microsoft Teams | — | — | — | ✅ |
| Google Chat | — | — | — | ✅ |
| Zalo | — | — | — | ✅ |
| Webhook | — | ✅ | — | ✅ |
| **Totale** | **4 + Web UI** | **16+** | **10** | **13** |

**Analisi:** ZeroClaw ha piu' canali (16+) ma nessuna Web UI. Homun ha Web UI completa (8 pagine) + 4 canali. WhatsApp nativo Rust — unico senza bridge JS (come ZeroClaw).

---

## Provider LLM

| Provider | Homun | ZeroClaw | Nanobot | OpenClaw |
|----------|:--------:|:------:|:------:|:--------:|
| Anthropic (nativo) | ✅ | ✅ | ✅ | ✅ |
| OpenAI | ✅ | ✅ | ✅ | ✅ |
| OpenRouter | ✅ | ✅ | ✅ | — |
| Ollama (locale) | ✅ | ✅ | ✅ | — |
| Ollama Cloud | ✅ | ? | — | — |
| DeepSeek | ✅ | ✅ | ✅ | — |
| Groq | ✅ | ✅ | ✅ | — |
| Gemini | ✅ | ✅ | ✅ | — |
| Mistral | ✅ | ✅ | ✅ | — |
| xAI (Grok) | ✅ | ✅ | — | — |
| Together | ✅ | ✅ | — | — |
| Fireworks | ✅ | ✅ | — | — |
| Perplexity | ✅ | ✅ | — | — |
| Cohere | ✅ | ✅ | — | — |
| Venice | ✅ | ? | — | — |
| AiHubMix | ✅ | ? | ✅ | — |
| DashScope (Qwen) | ✅ | ? | ✅ | — |
| Moonshot (Kimi) | ✅ | ? | ✅ | — |
| Zhipu (GLM) | ✅ | ? | ✅ | — |
| Minimax | ✅ | ? | ✅ | — |
| vLLM (locale) | ✅ | ✅ | ✅ | — |
| Custom endpoint | ✅ | ✅ | ✅ | — |
| AWS Bedrock | ✅ | ? | — | — |
| Cloudflare AI | ✅ | ? | — | — |
| Vercel AI | ✅ | ? | — | — |
| GitHub Copilot | ✅ | ? | — | — |
| **Totale** | **27** | **30+** | **15+** | **2** |

**Analisi:** Homun ha 27 provider con tool calling nativo. ZeroClaw ne ha 30+. OpenClaw solo 2. Il nostro vantaggio: Ollama-native + Ollama Cloud per operazione 100% offline o cloud senza bridge.

---

## Feature a Confronto

| Feature | Homun | ZeroClaw | Nanobot | OpenClaw |
|---------|:--------:|:------:|:------:|:--------:|
| Agent loop (ReAct) | ✅ | ✅ | ✅ | ✅ |
| Memory consolidation (LLM) | ✅ | ✅ | ✅ | ✅ |
| Vector embeddings (SQLite) | ✅ | ✅ | — | — |
| Hybrid search (vector + FTS5) | ✅ | ✅ | — | — |
| Heartbeat (wake-up proattivo) | ✅ | ✅ | ✅ | — |
| Cron scheduler | ✅ | ✅ | ✅ | ✅ |
| Subagent (task in background) | ✅ | ✅ | ✅ | — |
| Agent Skills (open spec) | ✅ | — | ✅ | ✅ (51+ bundled) |
| ClawHub marketplace | ✅ (client) | — | — | ✅ (3.286 skill) |
| Skill installer (GitHub) | ✅ | — | ✅ | ✅ |
| Skill executor (scripts) | ✅ | — | ✅ | ✅ |
| Skill hot-reload (file watcher) | ✅ | — | — | — |
| Bootstrap files (SOUL/USER.md) | ✅ | AIEOS | ✅ | — |
| MCP protocol (client) | ✅ | — | ✅ | — |
| **Web UI (8 pagine)** | **✅** | — | — | ✅ |
| **TUI dashboard (ratatui)** | **✅** | — | — | — |
| **WhatsApp nativo (no bridge)** | **✅** | ✅ | — | — |
| **Single binary** | **✅** | **✅** | — | — |
| **Ollama-native (LLM locale)** | **✅** | ✅ | ✅ | — |
| **Type-safe (compile time)** | **✅** | **✅** | — | — |
| **Shell sandboxing** | **✅** | ✅ | parziale | parziale |
| Browser control (CDP) | — | ✅ | — | ✅ |
| Voice (Whisper) | — | — | ✅ | ✅ (ElevenLabs) |
| Visual Canvas (A2UI) | — | — | — | ✅ |
| Mobile app (iOS/Android) | — | — | — | ✅ |
| Multi-agent routing | — | — | — | ✅ |
| Tunnel support (CF/Tailscale) | — | ✅ | — | — |
| Service management | — | ✅ | — | — |

---

## Architettura a Confronto

### Homun (Rust, 28.318 LOC)
```
src/
├── agent/        # Core: loop, context, gateway, heartbeat, memory, subagent
├── bus/          # Message bus (mpsc channels)
├── channels/     # CLI + Telegram + Discord + WhatsApp (nativo Rust)
├── config/       # Config TOML + dotpath editing, 27 provider
├── provider/     # OpenAI-compatible + Anthropic nativo + Ollama Cloud
├── scheduler/    # Cron custom (every/cron/at)
├── session/      # SQLite session management
├── skills/       # Loader + Installer (GitHub+ClawHub) + Executor + Watcher
├── storage/      # SQLite (sessions, messages, memories, cron, vault)
├── tools/        # 11 tool (shell, file×4, web×2, cron, spawn, message, MCP)
├── tui/          # TUI interattiva ratatui
└── web/          # ✅ Web UI completa (axum, 8 pagine)
```

### ZeroClaw (Rust, medium codebase)
```
zeroclaw/
├── providers/    # 30+ provider trait implementations
├── channels/     # 16+ channel trait implementations
├── memory/       # SQLite + vector embeddings + FTS5
├── tools/        # Shell, file, git, browser, HTTP, etc.
├── identity/     # AIEOS v1.1 spec
└── tunnel/       # Cloudflare, Tailscale, ngrok abstraction
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

1. **Web UI completa (8 pagine)** — Dashboard, Chat, Skills, Memory, Vault, Permissions, Logs, Settings. Unico tra i competitor Rust
2. **27 provider con tool calling nativo** — NON dipende da LiteLLM; ogni provider ha implementazione funzionante
3. **WhatsApp nativo Rust** — Zero dipendenze esterne (come ZeroClaw)
4. **TUI + Web dashboard** — Pannello di controllo dual-mode
5. **Type safety** — Errori catturati a compile time
6. **211 test** — Suite di test robusta
7. **Shell sandboxing** — Filtri sofisticati (fork bomb, rm -rf, pipe injection)
8. **SQLite storage** — Persistenza affidabile con migrazioni embedded
9. **Privacy-first** — Tutto locale di default, Ollama-native per operazione offline
10. **Agent Skills standard** — Supporto spec aperto + ClawHub client
11. **Vector embeddings + FTS5** — Ricerca ibrida in SQLite (come ZeroClaw)

## Dove Homun e' Indietro

| Gap | Priorita' | Sforzo | Note |
|-----|-----------|--------|------|
| ~~Web UI/Dashboard~~ | ~~CRITICA~~ | ~~Alto~~ | ✅ **Completa** (8 pagine) |
| ~~27 provider~~ | ~~ALTA~~ | ~~Alto~~ | ✅ **Fatto** (era 14) |
| **Binary size** | MEDIA | Medio | 47MB vs ZeroClaw 3-8MB |
| **Canali** (Slack, Email, Matrix) | MEDIA | Basso | ZeroClaw ne ha 16+ |
| **Browser control (CDP)** | ALTA | Medio | Killer feature. ZeroClaw ce l'ha |
| **Tunnel support** | MEDIA | Basso | Cloudflare, Tailscale (ZeroClaw ce l'ha) |
| **Service management** | MEDIA | Basso | `homun service install` |
| **Skill ecosystem** | ALTA | Alto | 2 bundled vs 51+ (OpenClaw) |
| **Deploy semplificato** | ALTA | Medio | Dockerfile, GitHub Actions, installer |
| **Voice (Whisper)** | BASSA | Medio | API Groq per messaggi vocali |
| Visual Canvas (A2UI) | BASSA | Alto | Non core per personal agent |
| Mobile app | BASSA | Alto | TG/WA sono le nostre "app" |

---

## Roadmap — Phase 6: UX & Polish

### ✅ Sprint 1: Web UI Foundation — COMPLETO
1. ✅ Web server embedded — axum integrato, porta 18080
2. ✅ Dashboard home — stato agente, canali, risorse
3. ✅ Skill manager UI — browse ClawHub, install, gestisci skill
4. ✅ Chat UI — WebSocket streaming
5. ✅ Memory page — browse/edit USER.md, MEMORY.md
6. ✅ Vault page — encrypted secrets management
7. ✅ Permissions page — channel auth, ACL rules
8. ✅ Settings page — 27 provider config

### Sprint 2: Production Hardening
1. **Browser tool (CDP)** — navigazione, click, screenshot
2. **Tunnel support** — Cloudflare, Tailscale
3. **Service install** — systemd/launchd
4. **GitHub Actions** — build multi-arch
5. **Dockerfile** — `FROM scratch` + binary

### Sprint 3: Canali & Voice
6. **Slack channel** — Socket Mode via `slack-morphism`
7. **Email channel** — IMAP receive + SMTP send
8. **Matrix channel** — via `matrix-sdk`

### Sprint 4: Skill Ecosystem
9. **10 skill bundled** — daily-briefing, weather, github-notify, etc.
10. **Skill creator wizard** — genera SKILL.md da descrizione

---

## Positioning Statement

> **Homun e' l'unico agente personale Rust con Web UI embedded, 27 provider, e supporto Agent Skills.**
> Scritto in Rust, compila in un singolo eseguibile con zero dipendenze.
> Supporta lo standard aperto Agent Skills e il marketplace ClawHub (3.286+ skill),
> funziona con 27 provider LLM (incluso funzionamento completamente offline via Ollama),
> e si gestisce via Web UI, Telegram, WhatsApp (nativo Rust), Discord, o TUI.
> Progettato per sviluppatori e power user
> che vogliono un assistente AI che gira 24/7 in modo affidabile sul proprio hardware.

### La Nicchia

- **OpenClaw** = il re della categoria. 180K+ stars, ClawHub (3.286 skill), creatore entrato in OpenAI. Ma: 430K LOC, Node.js, ~200MB RAM
- **ZeroClaw** = competitor Rust diretto. Piu' leggero (3-8MB), piu' canali, ma **nessuna Web UI**
- **Nanobot** = agente Python leggero (richiede runtime, no tool calling nativo)
- **Homun** = **unico con Web UI embedded + 27 provider + Agent Skills**. Il ponte tra semplicita' (ZeroClaw) e accessibilita' (OpenClaw)

---

## Numeri

| Metrica | Homun | ZeroClaw | Nanobot | OpenClaw |
|---------|----------|----------|--------|----------|
| File sorgente | 71 | ~50 | ~50 | 500+ |
| Linee di codice | 28.318 | Medium | ~10.161 | 430.000+ |
| Dipendenze | ~40 (Rust crates) | ~30 | ~26 (Python) | 200+ (npm) |
| Dimensione binary | ~47MB (release) | ~3-8MB | N/A | N/A |
| Runtime richiesto | Nessuno | Nessuno | Python 3.11+ | Node.js 22+ |
| Tempo startup | ~100ms | <10ms | ~2s | ~5s |
| Memoria (gateway) | ~100MB | <5MB | ~50MB | ~200MB |
| Provider | **27** | 30+ | 15+ | 2 |
| Canali | 4 + Web UI | 16+ | 10 | 13 |
| Tool | 11 | 13+ | 12 | 13+ |
| Skill bundled | 2 | 0 | 7 | 51 |
| ClawHub skill | 3.286 (client) | — | — | 3.286 (nativo) |
| Test | 211 | ? | — | ampia suite |
| Web UI | **✅ 8 pagine** | — | — | ✅ |

---

## ZeroClaw — Dettaglio Tecnico

### Punti di forza
- **30+ provider** — piu' di Homun
- **16+ canali** — Matrix, Signal, iMessage, Slack, Email
- **Binary 3-8MB** — molto piu' piccolo di Homun
- **RAM <5MB** — 20x piu' leggero
- **Tunnel abstraction** — Cloudflare, Tailscale, ngrok built-in
- **Service management** — `zeroclaw service install`
- **Browser control (CDP)** — automazione web
- **AIEOS identity** — spec portabile per agent personas

### Cosa Homun fa meglio
1. **Web UI completa** — ZeroClaw non ha nessuna interfaccia web
2. **Agent Skills standard** — ZeroClaw usa AIEOS proprietario
3. **ClawHub integration** — ZeroClaw non ha marketplace
4. **TUI** — ZeroClaw e' solo CLI

---

## OpenClaw — Il Progetto da Battere

OpenClaw e' il benchmark della categoria: 180K+ stars, ecosistema maturo.

### Punti di forza
- **180K+ stars** — il progetto piu' popolare
- **ClawHub marketplace** — 3.286 skill
- **51+ skill bundled** — produttivita', automazione
- **Visual Canvas (A2UI)** — workspace visuale
- **App mobile** (iOS/Android)
- **Browser control (CDP)**

### Vulnerabilita' di OpenClaw
- **430K LOC** — complessita' enorme
- **Node.js 22+** — runtime pesante, ~200MB RAM
- **ClawHavoc** (feb 2026) — 2.419 skill malevoli nel marketplace
- **Solo 2 provider** — Anthropic + OpenAI

### Dove Homun si differenzia

| | OpenClaw | Homun |
|---|---|---|
| Target | Tutti | Sviluppatori/power user |
| Deploy | Docker, cloud | Single binary |
| Complessita' | 430K LOC | 28K LOC |
| Runtime | Node.js 22+ | **Nessuno** |
| Provider | 2 | **27** |
| Risorse | ~200MB RAM | ~100MB RAM |
| WhatsApp | Baileys (JS) | **Nativo Rust** |
| Sicurezza skill | ClawHavoc | **Agent Skills + ClawHub curato** |
