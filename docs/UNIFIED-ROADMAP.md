# Homun — Unified Roadmap

> Last updated: 2026-03-18
> Consolidamento di: ROADMAP.md, IMPLEMENTATION-GAPS.md, openclaw-connections-vs-homun-detailed.md
> Obiettivo: piano unico orientato a **prodotto industriale**, sicurezza-first, senza legacy o feature completate.

---

## Stato Attuale — Snapshot

| Metrica | Valore |
|---------|--------|
| LOC Rust | ~87,400 |
| LOC Frontend | ~19,000 (JS) + ~10,000 (CSS) |
| Test | 690 passing |
| Canali | 7 (CLI, Telegram✅, Discord⚠️, WhatsApp⚠️, Slack⚠️, Email✅, Web✅) |
| Tool built-in | ~20 |
| Web UI Pages | 20 |
| MCP Recipes bundled | 6 (github, google-workspace, gmail, google-calendar, notion, slack) |
| Provider LLM | 14 |
| Release | Alpha v0.2 (REL-1..12 tutti ✅ DONE) |

*✅ = production-ready, ⚠️ = funzionante ma da hardening*

---

## Posizionamento Strategico

Homun è un **nucleo locale controllato** — single binary Rust, privacy-first, skill-powered.

Il gap principale vs competitor (OpenClaw) non è il numero di canali o provider.
È la **rete di connessioni operative**: canali maturi, accesso remoto, trust model, API esterne, multi-agent.

La strategia NON è copiare la superficie di OpenClaw (22+ canali, companion app, Tailscale).
La strategia è **consolidare il nucleo e aprirlo al mondo esterno** in ordine di ROI.

### Vantaggi competitivi da mantenere

- Single binary Rust (zero deps runtime)
- Web UI locale più ricca di qualsiasi competitor
- Agent Skills standard (unico personal assistant a supportarlo)
- Sandbox always-on multi-platform (macOS Seatbelt, Linux bwrap, Windows Job, Docker)
- MCP recipes con OAuth curato (Google, GitHub, Notion)
- Workflow engine persistente (più potente di OpenClaw Lobster)
- RAG personale con vault-gated access (né OpenClaw né ZeroClaw ce l'hanno)
- Security: exfiltration guard, content source labeling, email framing, vault 2FA

### Gap strutturali da colmare

- Canali esistenti non ancora production-ready (Discord, Slack, WhatsApp)
- Nessun accesso remoto ufficiale (localhost-only)
- Nessuna API esterna compatibile (non usabile come backend)
- Trust model non formalizzato per uso remoto
- Proactive messaging assente su 3/5 canali chat

---

## Piano Esecutivo — 4 Fasi

### Fase 1: Hardening Industriale (6-8 settimane)

> Obiettivo: ogni componente esistente è production-ready e sicuro.
> Criterio: un sysadmin può deployare su VPS e fidarsi che funzioni 24/7.

#### 1A. Channel Hardening (P0)

| # | Task | Effort | Note |
|---|------|--------|------|
| CHH-2 | **Discord reconnect robusto** | 2 giorni | ✅ DONE (2026-03-18) — resume/cache_ready handlers, health tracking in message loop, ChannelHealthTracker passato a Handler. Serenity auto-reconnect + spawn_monitored_channel restart |
| CHH-3 | **Slack → Socket Mode** | 1 settimana | ✅ DONE (2026-03-18) — Socket Mode via tokio-tungstenite quando app_token presente, polling fallback altrimenti. Latenza da 3s a <100ms |
| CHH-4 | **WhatsApp re-pairing da gateway** | 3 giorni | QR code via web UI, non solo TUI |
| CHH-5 | **Email robustness** | 3 giorni | ✅ DONE (2026-03-18) — NOOP keepalive ogni 5 cicli IDLE, seen messages pruning (cap 5000), reconnect con exponential backoff + password re-resolve da vault |
| PRO-1 | **Proactive messaging Discord** | 2 giorni | ✅ DONE (2026-03-18) — thread_id routing in outbound, config warning. Proactive già funzionante via default_channel_id |
| PRO-2 | **Proactive messaging Slack** | 2 giorni | ✅ DONE (2026-03-18) — default_channel_id config, fallback a channel_id, startup warning. Proactive via chat.postMessage + routing in active_channels_with_chat_ids |
| PRO-3 | **Proactive messaging WhatsApp** | 3 giorni | Valutare fattibilità in wa-rs fork |
| CAP-1 | **Channel capability matrix** | 3 giorni | ✅ DONE (2026-03-18) — `ChannelCapabilities` struct, `capabilities_for()`, system prompt injection, send_message soft warnings. 7 test. ~190 LOC |

#### 1B. Sicurezza Prompt Injection (P0)

Stato: SEC-6/7/8/11/12/13/14/15 tutti ✅ DONE. Scudo anti-injection completo.

| # | Task | Effort | Note |
|---|------|--------|------|
| SEC-13 | **Tool result injection detection** | 2 giorni | ✅ DONE (2026-03-18) — `scan_tool_for_injection()` riusa `detect_injection()` da RAG, 7 pattern, warning inline. 6 test |
| SEC-14 | **Webhook payload framing** | 1 giorno | ✅ DONE (2026-03-18) — `[INCOMING WEBHOOK — UNTRUSTED CONTENT]` wrapper in `webhook_ingress()` |
| SEC-15 | **Browser content labeling** | 2 giorni | ✅ DONE (2026-03-18) — browser rimosso da skip list, ora etichettato come `browser page content (untrusted)` |

#### 1C. Testing (P1)

| # | Task | Effort | Note |
|---|------|--------|------|
| TST-1 | **E2E Playwright CI** | 1 settimana | CHAT-7: smoke suite per Web UI in CI (chat, automations, settings) |
| TST-3 | **Channel integration tests** | 1 settimana | ✅ DONE (2026-03-18) — 8 integration test in channels/mod.rs: health lifecycle, capabilities coverage, degradation/recovery, proactive routing, Slack Socket Mode toggle, config defaults |
| TST-4 | **CI sandbox validation** | 2 giorni | Push e trigger workflow GitHub Actions su runner reali |

#### 1D. UX Beta (P1)

| # | Task | Effort | Note |
|---|------|--------|------|
| ONB-1 | **Setup wizard v2** | 1 settimana | Flusso: provider → API key → test → first message. Resume se interrotto |
| ONB-2 | **Flusso Ollama locale** | 3 giorni | "Vuoi AI locale senza API key?" → Ollama → pull → pronto |
| ONB-4 | **First-run tutorial** | 3 giorni | Tour interattivo dismissable |
| AUD-2 | **Feature gating doc** | 1 giorno | Documentare chiaramente embeddings/RAG gating in README e setup wizard |

---

### Fase 2: Apertura al Mondo Esterno (8-12 settimane)

> Obiettivo: Homun non è più solo localhost. È raggiungibile, usabile come backend, con trust model chiaro.
> Criterio: un utente può usare Homun da remoto in modo sicuro e documentato.

#### 2A. Remote Access (P0)

| # | Task | Effort | Note |
|---|------|--------|------|
| REM-1 | **Remote access story ufficiale** | 3 giorni | `docs/REMOTE-ACCESS.md` con 3 pattern testati: SSH tunnel, Tailscale Serve, Caddy/nginx reverse proxy |
| REM-2 | **X-Forwarded-For awareness** | 1 giorno | Rate limiting corretto dietro reverse proxy |
| REM-3 | **Trusted device model** | 1 settimana | Device enrollment esplicito (browser fingerprint + approval). Oltre il semplice login session |
| REM-4 | **Session hardening per remoto** | 3 giorni | CSRF token, session binding a IP/device, configurable session lifetime |

#### 2B. API Esterne Compatibili (P1)

| # | Task | Effort | Note |
|---|------|--------|------|
| API-1 | **OpenAI Chat Completions compat** | 1 settimana | `POST /v1/chat/completions` con bearer auth, streaming SSE, model routing |
| API-2 | **Session routing API** | 3 giorni | `session_id` parameter, create/resume sessions via API |
| API-3 | **API docs (OpenAPI spec)** | 3 giorni | Swagger/OpenAPI per tutti i 50+ endpoint |

#### 2C. Trust Model Esplicito (P1)

| # | Task | Effort | Note |
|---|------|--------|------|
| TRS-1 | **Trust model doc** | 2 giorni | Documento che definisce: admin locale, admin remoto, browser trusted, sender canale, webhook caller, MCP service, agente. Per ciascuno: come si autentica, che poteri ha, dove vive il suo stato |
| TRS-2 | **Vault access audit log** | ✅ DONE | Migration 019, fire-and-forget audit, `GET /v1/vault/audit` |

#### 2D. MCP Recipes Expansion (P1)

| # | Task | Effort | Note |
|---|------|--------|------|
| RCP-1 | **Google Drive recipe** | 1 giorno | TOML + OAuth wiring (scopes già in google_mcp_scopes) |
| RCP-2 | **Google Calendar recipe** | ✅ DONE | Parte di google-workspace.toml |
| RCP-3 | **Linear recipe** | 1 giorno | API key auth, project management |
| RCP-4 | **Jira recipe** | 1 giorno | API key auth, issue tracking |
| RCP-5 | **Reddit recipe** | 1 giorno | OAuth, content monitoring |
| RCP-6 | **GitLab recipe** | 1 giorno | API key auth |

Target: **12 recipes bundled** (da 6 attuali).

#### 2E. Installer Nativi (P1)

| # | Task | Effort | Note |
|---|------|--------|------|
| INST-1 | **macOS .dmg** | 1 settimana | App bundle, launchd, code signing |
| INST-2 | **Windows .msi** | 1 settimana | MSI installer, Windows Service |
| INST-3 | **Linux packages** | 3 giorni | .deb + .rpm + systemd unit |
| INST-4 | **Homebrew formula** | 2 giorni | `brew install homun` |

---

### Fase 3: Prodotto Consumer-Ready (12-16 settimane)

> Obiettivo: utente non-tecnico può installare e usare senza assistenza.
> Criterio: one-click install, docs complete, app mobile base, sito web.

#### 3A. Sito Web Prodotto

| # | Task | Effort |
|---|------|--------|
| WEB-1 | Landing page (hero, features, CTA download) | 1 settimana |
| WEB-2 | Pagina download (detect OS, link per piattaforma) | 3 giorni |
| WEB-3 | Pagina features (screenshot/GIF per macro-feature) | 1 settimana |
| WEB-7 | Dominio + hosting (homun.dev, Cloudflare Pages) | 1 giorno |

#### 3B. Docs Site

| # | Task | Effort |
|---|------|--------|
| DOC-1 | Infrastruttura (MkDocs Material, docs.homun.dev) | 2 giorni |
| DOC-2 | Guida installazione per piattaforma | 3 giorni |
| DOC-3 | Guida configurazione (ogni sezione config.toml) | 3 giorni |
| DOC-4 | Guida canali (setup per ogni canale con screenshot) | 1 settimana |
| DOC-5 | Guida automazioni (builder + NLP + template gallery) | 3 giorni |
| DOC-6 | Guida skills/MCP | 3 giorni |
| DOC-8 | Troubleshooting/FAQ (top 20 problemi) | 3 giorni |
| DOC-9 | Contributing guide | 2 giorni |

#### 3C. App Mobile (Flutter)

| # | Task | Effort |
|---|------|--------|
| APP-1 | Fondazioni: pairing sicuro + channel "app" + chat base + push | 3-4 settimane |
| APP-2 | Esperienza ricca: vault mobile + dashboard + approval inline + allegati | 2-3 settimane |
| APP-3 | Polish: offline cache + biometric lock + widget | 2 settimane |
| APP-4 | Store publishing (App Store + Play Store) | 1 settimana |

#### 3D. Osservabilità

| # | Task | Effort |
|---|------|--------|
| OBS-1 | Metrics base (Prometheus `/metrics`) | 3 giorni |
| OBS-2 | Correlation IDs (request tracing gateway → agent → tool) | 2 giorni |
| OBS-3 | Crash reporting (panic handler + optional Sentry) | 2 giorni |

#### 3E. Localizzazione

| # | Task | Effort |
|---|------|--------|
| I18N-1 | Framework i18n (JS + prompt agent, EN + IT) | 1 settimana |
| I18N-2 | Uniformare UI a EN come base | 3 giorni |

#### 3F. Auto-update

| # | Task | Effort |
|---|------|--------|
| UPD-1 | Update checker (GitHub Releases, notifica in UI) | 2 giorni |
| UPD-2 | Auto-update binary + Docker watchtower | 3 giorni |

---

### Fase 4: Espansione Strategica (timeline aperta)

> Solo dopo le Fasi 1-3. Questi item si attivano quando c'è domanda reale.

#### 4A. Multi-Agent Routing

| # | Task | Note |
|---|------|------|
| MAR-1 | `agent_id` first-class nel runtime | Directory dati separata per agente |
| MAR-2 | Bindings inbound channel/sender → agent_id | Routing per agente |
| MAR-3 | Per-agent bootstrap files e session store | Isolamento stato |
| MAR-4 | API selezione agente (`/v1/chat/completions?agent=work`) | Integrazione con API-1 |

#### 4B. PWA / Desktop Wrapper

| # | Task | Note |
|---|------|------|
| PWA-1 | Service worker + manifest (Web UI come PWA installabile) | Offline cache chat |
| PWA-2 | Web Push notifications | Push browser |
| PWA-3 | Desktop wrapper opzionale (Tauri) | Auto-update |

#### 4C. Ingress Specializzati

| # | Task | Note |
|---|------|------|
| ING-1 | Webhook templates (GitHub, Stripe, monitoring alerts) | Mapping strutturato |
| ING-2 | Event source framework | `source → event type → normalized payload → routing` |
| ING-3 | Poll/button callbacks cross-channel | Interazione strutturata |

#### 4D. UI Redesign

| # | Task | Note |
|---|------|------|
| RED-1 | **Editorial Canvas** redesign | Spec in `docs/design/REDESIGN-SPEC.md`. Da Olive Moss a editorial moderno |

---

## Cose Rimosse (e perché)

Questi item erano nella roadmap precedente e sono stati **eliminati** dal piano attivo:

| Item | Motivo rimozione |
|---|---|
| **Nuovi canali** (Signal, IRC, Matrix, Teams, LINE, Feishu, Nostr, etc.) | ROI troppo basso per single-dev. Nessuna domanda reale. 7 canali coprono i casi d'uso principali |
| **BIZ-2..5** (payments, accounting, marketing, crypto) | Troppo ambizioso, non core. BIZ-1 engine esiste per chi vuole sperimentare. Il resto via MCP/skill |
| **Voice/telephony pipeline** | Richiede infrastruttura dedicata, non prioritario per v1 |
| **Plugin system compilato** (dynamic Rust loading) | Eccessivamente complesso. MCP + Skills coprono l'estensibilità |
| **Data marketplace** | Premature optimization |
| **AIEOS identity** (da ZeroClaw) | Non rilevante per il posizionamento |
| **Gmail Pub/Sub ingress** | Il canale Email con IMAP IDLE copre già il caso d'uso. Overhead di setup sproporzionato |
| **macOS companion app** | Prematura — prima serve remote access (Fase 2) |
| **File Split FS-1..44, FS-JS-1..12** | Refactoring incrementale, non sprint dedicato. Si fa quando si tocca il file |
| **AUTO-4 wizard step-by-step** | Nice-to-have, non blocca. Il builder visuale è sufficiente per il target attuale |
| **WEB-4 pricing page** | Prematura fino a v1 |
| **WEB-5 blog** | Prematura fino a v1 |
| **WEB-6 SEO** | Prematura fino a v1 |

---

## Backlog Tecnico (ongoing, non bloccante)

Questi non sono feature — sono manutenzione continua. Si fanno quando si tocca il codice vicino.

| Area | Stato | Azione |
|---|---|---|
| File split (FS-*) | 44 file Rust + 12 JS over limit | Split quando si tocca il file, non come sprint |
| RAG format coverage | 8 formati con parsing reale, 25+ plain text fallback | Aggiungere parser quando serve (TypeScript AST, Python AST) |
| Browser E2E in CI | Smoke manuale, non in CI | Promuovere a CI con TST-1 |
| Stealth browser avanzato | `addInitScript` baseline | CDP endpoint mode + wrapper script (se necessario) |
| Windows sandbox v2 | Job Objects base | AppContainer + NTFS ACL (non bloccante per MVP) |
| Runtime image browser | Core baseline | Estendere per skill/MCP browser-heavy |
| Config hot-reload | MCP hot-reload ✅, canali richiedono restart | Documentare restart-required changes |
| Dashboard real-time | Data caricata al page load | WebSocket push (P2) |

---

## Timeline

| Fase | Focus | Effort | Target |
|------|-------|--------|--------|
| **1: Hardening** | Canali, sicurezza, testing, onboarding | 6-8 settimane | ~Maggio 2026 |
| **2: Apertura** | Remote access, API, trust, recipes, installer | 8-12 settimane | ~Agosto 2026 |
| **3: Consumer** | Sito, docs, mobile, osservabilità, i18n | 12-16 settimane | ~Dicembre 2026 |
| **4: Espansione** | Multi-agent, PWA, ingress, redesign | Timeline aperta | 2027+ |

---

## Principi Operativi

1. **Non inseguire OpenClaw sulla breadth.** Loro hanno un team. Noi abbiamo coesione.
2. **Ogni feature nuova deve essere production-ready.** Meglio 7 canali solidi che 22 fragili.
3. **Sicurezza non è un'opzione.** Content labeling, sandbox, exfiltration guard, trust model — non negoziabili.
4. **Remote-first è il prossimo unlock.** Senza remote access, Homun resta un tool locale. Con remote access diventa un prodotto.
5. **API esterne trasformano il posizionamento.** Da "app che riceve messaggi" a "backend accessibile da software terzo".
6. **Documenta prima di costruire.** Trust model, remote access, deployment stories — servono doc prima di codice.
