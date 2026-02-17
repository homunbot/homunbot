# HomunBot — Analisi Competitiva

> Ultimo aggiornamento: 2026-02-17
> Stato: HomunBot Phase 5+ (11.919 LOC, 161 test, 47 file sorgente)

## Panoramica

| | **HomunBot** | **Nanobot** | **TinyClaw** | **OpenClaw** |
|---|---|---|---|---|
| **Linguaggio** | **Rust** | Python | TS/JS + Bash | TS/JS (monorepo) |
| **LOC** | **11.919** | ~10.161 | ~2.800 | 430.000+ |
| **GitHub Stars** | nuovo | nuovo (~2 settimane) | 1.934 | 200.987 |
| **Binary** | **Single binary, 0 deps** | Python + pip | Node.js | Node.js + pnpm |
| **Test** | **161** | non dichiarati | minimali | suite completa |
| **Licenza** | MIT | MIT | MIT | MIT |
| **Approccio** | Privacy-first, skill-powered | Lightweight personal agent | Multi-agent framework | Full-featured enterprise |

---

## Supporto Canali

| Canale | HomunBot | Nanobot | TinyClaw | OpenClaw |
|---------|:--------:|:------:|:--------:|:--------:|
| CLI | ✅ | ✅ | ✅ | ✅ |
| Telegram | ✅ | ✅ | ✅ | ✅ |
| WhatsApp | ✅ | ✅ | ✅ | ✅ |
| Discord | ✅ | ✅ | ✅ | ✅ |
| Slack | — | ✅ | — | ✅ |
| Email (IMAP/SMTP) | — | ✅ | — | ✅ (Gmail) |
| Feishu/Lark | — | ✅ | — | — |
| DingTalk | — | ✅ | — | — |
| QQ | — | ✅ | — | — |
| Mochat (Claw IM) | — | ✅ | — | — |
| Signal | — | — | — | ✅ |
| iMessage | — | — | — | ✅ |
| Microsoft Teams | — | — | — | ✅ |
| Matrix | — | — | — | ✅ |
| Google Chat | — | — | — | ✅ |
| **Totale** | **4** | **10** | **3** | **11** |

**Analisi:** Nanobot domina sulle piattaforme asiatiche (Feishu, DingTalk, QQ). OpenClaw sui canali enterprise (Teams, Signal, iMessage). HomunBot copre i quattro canali personali principali (CLI + Telegram + Discord + WhatsApp). WhatsApp e' implementato nativamente in Rust via libreria `whatsapp-rust` (vendored) — nessun bridge Node.js necessario.

---

## Provider LLM

| Provider | HomunBot | Nanobot | TinyClaw | OpenClaw |
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

**Analisi:** HomunBot e Nanobot offrono entrambi supporto provider esteso. La differenza chiave: HomunBot implementa tool calling nativo per ogni provider (nessuna dipendenza LiteLLM), mentre Nanobot usa LiteLLM che NON supporta tool calling. L'approccio di HomunBot e' piu' affidabile per l'agent loop.

---

## Tool

| Tool | HomunBot | Nanobot | TinyClaw | OpenClaw |
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
| Browser control | — | — | — | ✅ |
| **Totale** | **11** | **12** | **3** | **10+** |

---

## Feature a Confronto

| Feature | HomunBot | Nanobot | TinyClaw | OpenClaw |
|---------|:--------:|:------:|:--------:|:--------:|
| Agent loop (ReAct) | ✅ | ✅ | ✅ | ✅ |
| Memory consolidation (LLM) | ✅ | ✅ | ✅ | ✅ |
| Heartbeat (wake-up proattivo) | ✅ | ✅ | ✅ | — |
| Cron scheduler | ✅ | ✅ | — | ✅ |
| Subagent (task in background) | ✅ | ✅ | ✅ | — |
| Agent Skills (open spec) | ✅ | ✅ | — | ✅ (51+ bundled + ClawHub marketplace) |
| Skill installer (GitHub) | ✅ | ✅ | — | ✅ |
| Skill executor (scripts) | ✅ | ✅ | — | ✅ |
| Bootstrap files (SOUL/USER.md) | ✅ | ✅ | — | — |
| MCP protocol | ✅ | ✅ | — | — |
| **TUI dashboard (ratatui)** | **✅** | — | — | — |
| **WhatsApp nativo (no bridge)** | **✅** | — | — | — |
| Voice (Whisper) | — | ✅ | — | ✅ |
| Visual Canvas/UI | — | — | — | ✅ |
| Mobile app (iOS/Android) | — | — | — | ✅ |
| OAuth provider login | — | ✅ | — | ✅ |
| **Single binary** | **✅** | — | — | — |
| **Ollama-native (LLM locale)** | **✅** | ✅ | — | — |
| **Type-safe (compile time)** | **✅** | — | — | — |
| **Shell sandboxing** | **✅** | parziale | — | parziale |

---

## Architettura a Confronto

### HomunBot (Rust, 11.919 LOC)
```
src/
├── agent/        # Core: loop, context, gateway, heartbeat, memory, subagent
├── bus/          # Message bus (mpsc channels)
├── channels/     # CLI + Telegram + Discord + WhatsApp (nativo Rust)
├── config/       # Config TOML + dotpath editing, 14 provider
├── provider/     # OpenAI-compatible + Anthropic nativo
├── scheduler/    # Cron custom (every/cron/at)
├── session/      # SQLite session management
├── skills/       # Loader + Installer (GitHub) + Executor (Py/Bash/JS) + Search
├── storage/      # SQLite (sessions, messages, memories, cron)
├── tools/        # 11 tool (shell, file×4, web×2, cron, spawn, message, MCP)
└── tui/          # TUI interattiva ratatui (settings, providers, whatsapp, skills, MCP)
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

### TinyClaw (TS/Bash, ~2.8K LOC)
```
tinyclaw/
├── agents/       # Config agenti + workspace
├── channels/     # Discord, Telegram, WhatsApp
├── queue/        # File-based (incoming → processing → outgoing)
└── scripts/      # Orchestrazione Bash + tmux
```

### OpenClaw (TS/JS, 430K+ LOC)
```
openclaw/          # pnpm monorepo
├── packages/
│   ├── clawdbot/  # Core engine
│   └── moltbot/   # Estensioni
├── skills/        # 51 skill bundled
├── gateway/       # WebSocket control plane
├── clients/       # CLI, macOS, iOS, Android, Web
└── tests/         # Unit, integration, e2e
```

---

## Dove HomunBot Vince

1. **Single Rust binary** — Zero dipendenze, startup istantaneo, ~10MB, deploy ovunque (Raspberry Pi, VPS, container scratch, embedded)
2. **14 provider con tool calling nativo** — NON dipende da LiteLLM (che non supporta tool calling); ogni provider ha un'implementazione funzionante
3. **WhatsApp nativo Rust** — Libreria `whatsapp-rust` vendored, connessione diretta a WhatsApp Web senza bridge Node.js. Pairing via TUI, self-message support, LID addressing
4. **TUI dashboard** — Pannello di controllo terminale (ratatui) con 5 tab: Settings, Providers, WhatsApp, Skills, MCP. Config editing, provider management, WhatsApp pairing interattivo
5. **Type safety** — Errori catturati a compile time vs runtime. `Result<T>` ovunque
6. **Performance e memoria** — Ordini di grandezza migliore di Python/Node per processi long-running. Nessuna GC pause
7. **161 test** — Suite di test robusta inclusa dal primo giorno
8. **Shell sandboxing** — Filtri di sicurezza sofisticati (fork bomb, rm -rf, pipe injection, base64 obfuscation)
9. **SQLite storage** — Storage persistente affidabile vs file JSON (nanobot) o code basate su file (TinyClaw)
10. **Privacy-first** — Tutto locale di default, Ollama-native per operazione completamente offline

## Dove HomunBot e' Indietro

| Gap | Priorita' | Sforzo | Note |
|-----|-----------|--------|------|
| ~~MCP protocol~~ | ~~ALTA~~ | ~~Medio~~ | ✅ Implementato (rmcp, stdio transport) |
| ~~Discord channel~~ | ~~MEDIA~~ | ~~Basso~~ | ✅ Implementato (serenity, EventHandler + outbound loop) |
| ~~WhatsApp channel~~ | ~~MEDIA~~ | ~~Medio~~ | ✅ Implementato (nativo Rust via whatsapp-rust, pairing TUI, self-message, LID support) |
| **Canali** (Slack, Email) | MEDIA | Basso ciascuno | Esistono crate Rust (slack-morphism, lettre) |
| ~~MessageTool~~ (invia msg all'utente) | ~~MEDIA~~ | ~~Basso~~ | ✅ Implementato (send_message tool) |
| **Voice (Whisper)** | MEDIA | Medio | Via API Groq per messaggi vocali Telegram |
| **OpenAI Codex (OAuth)** | BASSA | Medio | Provider di nicchia |
| **Skill marketplace** | ALTA | Alto | OpenClaw ha ClawHub (clawhub.ai) con installazione one-click e community. Il gap piu' grande |
| **Deploy semplificato** | MEDIA | Medio | OpenClaw ha one-click deploy su Railway/Render. HomunBot richiede scp + systemd |
| **Piu' skill bundled** | MEDIA | Basso | 2 vs 7 (nanobot) vs 51+ (OpenClaw) |
| **Provider registry pattern** | BASSA | Basso | Refactor per aggiungere provider piu' facilmente |

---

## Roadmap Suggerita (per impatto)

### Breve termine (alto impatto)
1. ~~**MCP Client**~~ — ✅ Implementato con rmcp (stdio transport, auto-discovery tool)
2. ~~**MessageTool**~~ — ✅ Implementato (send_message tool con routing proattivo)
3. ~~**Discord channel**~~ — ✅ Implementato via crate `serenity` (access control, outbound loop, message splitting)
4. **Voice transcription** — API Groq Whisper per messaggi vocali da Telegram

### Medio termine
5. ~~**WhatsApp nativo**~~ — ✅ Implementato: libreria `whatsapp-rust` vendored, connessione diretta a WhatsApp Web, pairing via TUI, self-message support, LID addressing, access control
6. **Slack channel** — Via WebSocket (Socket Mode)
7. **Piu' skill bundled** — market-monitor, email-digest, habit-tracker
8. **Email channel** — IMAP receive + SMTP send

### Lungo termine
9. ~~**TUI dashboard**~~ — ✅ Implementato: ratatui con 5 tab (Settings, Providers, WhatsApp pairing, Skills, MCP), navigazione tab, editing inline, popup form
10. **Visual UI (web)** — Dashboard web per monitorare lo stato dell'agente
11. **Mobile companion** — App Flutter per comunicazione diretta
12. **MCP Server** — Esporre le capability di HomunBot ad altri agenti
13. **Plugin system** — Plugin Rust compilati via dynamic loading

---

## Positioning Statement

> **HomunBot e' l'unico agente personale AI single-binary, privacy-first e skill-powered.**
> Scritto in Rust, compila in un singolo eseguibile con zero dipendenze.
> Supporta lo standard aperto Agent Skills, funziona con 14 provider LLM
> (incluso funzionamento completamente offline via Ollama),
> e si gestisce da remoto via Telegram, WhatsApp (nativo Rust) o Discord.
> Include una TUI interattiva per configurazione e WhatsApp pairing.
> Progettato per sviluppatori e power user
> che vogliono un assistente AI che gira 24/7 in modo affidabile sul proprio hardware.

### La Nicchia

- **OpenClaw** = il re della categoria. Marketplace, community, deploy facile. Il progetto da battere
- **Nanobot** = agente Python leggero (stessa filosofia di HomunBot, ma richiede Python runtime)
- **TinyClaw** = framework multi-agente (sperimentale, meno feature)
- **HomunBot** = **agente Rust single-binary per chi vuole il controllo totale**. Niente cloud, niente Node.js, niente Docker. Un binario, il tuo hardware, i tuoi dati

---

## Numeri

| Metrica | HomunBot | Nanobot | TinyClaw | OpenClaw |
|---------|----------|--------|----------|----------|
| File sorgente | 47 | ~50 | ~15 | 500+ |
| Linee di codice | 11.919 | ~10.161 | ~2.800 | 430.000+ |
| Dipendenze | ~40 (Rust crates) | ~26 (Python) | ~10 (npm) | 200+ (npm) |
| Dimensione binary | ~10MB (release) | N/A (interpretato) | N/A | N/A |
| Tempo startup | <50ms | ~2s | ~1s | ~5s |
| Memoria (idle) | ~5MB | ~50MB | ~30MB | ~200MB |
| Provider | 14 | 15+ | 2 | 2 |
| Canali | 4 | 10 | 3 | 11 |
| Tool | 11 | 12 | 3 | 10+ |
| Skill bundled | 2 | 7 | 0 | 51 |
| Test | 161 | — | — | ampia suite |

*I numeri di performance sono stime basate su caratteristiche tipiche Rust vs Python/Node.*

---

## Nanobot — Dettaglio Tecnico

### Punti di forza di Nanobot
- **10 canali** incluse piattaforme cinesi (Feishu, DingTalk, QQ, Mochat)
- **MCP support** aggiunto a febbraio 2026
- **Provider registry pattern** — aggiungere un provider richiede solo 2 step
- **LiteLLM integration** — interfaccia unificata per 100+ modelli (ma senza tool calling)
- **7 skill bundled** — Memory, Summarize, Skill Creator, GitHub, Tmux, Weather, Cron
- **Groq voice transcription** — trascrizione automatica messaggi vocali via Whisper
- **Sviluppo molto attivo** — 378 commit in 2 settimane, 7 release

### Cosa HomunBot fa meglio di Nanobot
1. **Nessun Python** — nanobot richiede Python 3.11+, pip, e 26 dipendenze
2. **Tool calling nativo** — nanobot usa LiteLLM che non supporta tool calling; HomunBot lo implementa per ogni provider
3. **WhatsApp nativo Rust** — libreria `whatsapp-rust` vendored, zero dipendenze esterne. Nanobot usa bridge Python
4. **TUI dashboard** — pannello di controllo terminale con 5 tab per config, providers, WhatsApp pairing, skills, MCP
5. **SQLite vs JSON** — nanobot usa file JSON per persistenza, piu' fragile
6. **Type safety** — Python dict everywhere vs typed Rust structs
7. **Concorrenza reale** — tokio con parallelismo reale vs asyncio single-threaded
8. **161 test** — nanobot non dichiara test nella codebase
9. **Security** — sandboxing shell piu' sofisticato (fork bomb, base64 obfuscation detection)

### Cosa Nanobot fa meglio di HomunBot
1. **10 canali** vs nostri 4
2. **7 skill bundled** vs nostre 2
3. **OAuth** — login per provider come OpenAI Codex
4. **Voice** — trascrizione automatica messaggi vocali

---

## OpenClaw — Il Progetto da Battere

OpenClaw e' il benchmark della categoria: 200K+ stars, ecosistema maturo, community enorme. E' il progetto di riferimento contro cui misurarsi.

### Punti di forza di OpenClaw
- **200K+ stars** — il progetto piu' popolare nella categoria, di gran lunga
- **ClawHub marketplace** (clawhub.ai) — marketplace di skill con installazione one-click, community-driven. Questo e' il vero vantaggio competitivo: un ecosistema di plugin che si auto-alimenta
- **51+ skill bundled** — dalla produttivita' (Notion, Trello) ai media (Spotify, Sonos), con nuove skill aggiunte dalla community ogni settimana
- **Deploy semplificato** — one-click deploy su Railway, Render, Fly.io, Docker. L'onboarding e' molto curato
- **Visual Canvas (A2UI)** — workspace visuale guidato dall'agente
- **Voice Wake + Talk Mode** — integrazione ElevenLabs
- **App mobile** (iOS/Android) — nodi companion per azioni device-specific
- **Browser control** — via Chrome DevTools Protocol
- **11 canali** — inclusi iMessage, Signal, Microsoft Teams

### Perche' OpenClaw e' attualmente superiore
OpenClaw ha un ecosistema completo: marketplace di skill, deploy one-click, community attiva, app mobile. E' la soluzione piu' matura e accessibile per chi vuole un agente AI personale. Il vantaggio non e' solo tecnico — e' l'effetto rete del ClawHub marketplace.

### Dove HomunBot si differenzia
HomunBot non compete con OpenClaw sullo stesso terreno. La proposta e' diversa:

| | OpenClaw | HomunBot |
|---|---|---|
| Target | Tutti (accessibile) | Sviluppatori/power user |
| Deploy | One-click cloud | `scp binary && ./homunbot gateway` |
| Complessita' | 430K LOC, pnpm, monorepo | 11.9K LOC, single crate |
| Skill ecosystem | ClawHub marketplace (100+) | Agent Skills spec (standard aperto) |
| Privacy | Cloud-oriented | **Local-first, Ollama-native** |
| Risorse | ~200MB RAM, Node runtime | **~5MB RAM, zero deps** |
| WhatsApp | Bridge esterno | **Nativo Rust (zero deps)** |
| Customizzazione | Config UI | **TUI + TOML + codice Rust** |
