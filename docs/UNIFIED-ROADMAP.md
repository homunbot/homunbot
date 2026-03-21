# Homun — Unified Roadmap

> Last updated: 2026-03-21
> Consolidamento di: ROADMAP.md, IMPLEMENTATION-GAPS.md, openclaw-connections-vs-homun-detailed.md
> Obiettivo: piano unico orientato a **prodotto industriale**, sicurezza-first, senza legacy o feature completate.

---

## Stato Attuale — Snapshot

| Metrica | Valore |
|---------|--------|
| LOC Rust | ~87,400 |
| LOC Frontend | ~19,000 (JS) + ~10,000 (CSS) |
| Test | 762 passing |
| Canali | 7 (CLI, Telegram✅, Discord⚠️, WhatsApp⚠️, Slack⚠️, Email✅, Web✅) |
| Tool built-in | ~20 |
| Web UI Pages | 21 |
| MCP Recipes bundled | 17 (github, google-workspace, google-maps, notion, slack, gitlab, linear, jira, reddit, brave-search, spotify, stripe, twitter, sentry, todoist, home-assistant, wordpress) |
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
- MCP recipes con OAuth curato (17 servizi verificati, setup guides, runtime token refresh)
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
| CHH-4 | **WhatsApp re-pairing da gateway** | 3 giorni | ✅ DONE — WebSocket pairing endpoint `/api/v1/channels/whatsapp/pair`, JS client con pair code display, config auto-save. Usa pair code (8 char) non QR |
| CHH-5 | **Email robustness** | 3 giorni | ✅ DONE (2026-03-18) — NOOP keepalive ogni 5 cicli IDLE, seen messages pruning (cap 5000), reconnect con exponential backoff + password re-resolve da vault |
| PRO-1 | **Proactive messaging Discord** | 2 giorni | ✅ DONE (2026-03-18) — thread_id routing in outbound, config warning. Proactive già funzionante via default_channel_id |
| PRO-2 | **Proactive messaging Slack** | 2 giorni | ✅ DONE (2026-03-18) — default_channel_id config, fallback a channel_id, startup warning. Proactive via chat.postMessage + routing in active_channels_with_chat_ids |
| PRO-3 | **Proactive messaging WhatsApp** | 3 giorni | ✅ DONE (2026-03-18) — già funzionante via phone_number→JID in active_channels_with_chat_ids. Aggiunto typing indicator (composing/paused), presence online, startup warning |
| CAP-1 | **Channel capability matrix** | 3 giorni | ✅ DONE (2026-03-18) — `ChannelCapabilities` struct, `capabilities_for()`, system prompt injection, send_message soft warnings. 7 test. ~190 LOC |
| CHR-2 | **Unified auth (fail-closed)** | — | ✅ DONE (2026-03-18) — Auth spostata dai canali al gateway. `agent/auth.rs` con pipeline: allow_from → contact DB live lookup → pairing/reject. Tutti i canali fail-closed. Email domain matching preservato. 5 test |
| CHR-3 | **Per-channel persona** | — | ✅ DONE (2026-03-18) — Persona bot/owner/company/custom per canale e per contatto. Priority chain: contact > channel > "bot". Tone of voice chain analoga. `agent/persona.rs` resolver + `PersonaSection` nel prompt builder. Migration 023. 6 test |
| CHR-4 | **MCP channel support** | — | ✅ DONE (2026-03-18) — `channels/mcp_channel.rs` + `McpChannelConfig`. Gateway startup + auth pipeline per canali MCP. Scaffold per messaging protocol (attende spec MCP messaging) |
| CHR-5 | **Memory scoping by contact** | — | ✅ DONE (2026-03-18) — `contact_id` su memory_chunks (migration 024). `search_scoped()` in memory_search.rs filtra per contact + globale. Consolidation passa None (TODO: resolve da session_key) |
| CHR-6 | **ChannelBehavior trait** | — | ✅ DONE (2026-03-19) — Trait unificato con 7 metodi (persona, tone, response_mode, notify, allow_from, pairing). Implementato su 6 config canale. `behavior_for()` unico punto di lookup. Eliminati 4+ match ripetuti |
| LLM-1 | **LLM request queue con priorità** | 1 settimana | ✅ DONE (2026-03-19) — `QueuedProvider` in `provider/queued.rs`: tokio::Semaphore per-provider (auto: 1 local, 5 cloud, configurabile via `llm_max_concurrent`). `RequestPriority` 3 livelli (High/Normal/Low) su `ChatRequest`. Yield-loop scheduling: Low cede a High+Normal. Wrappa ReliableProvider in factory.rs, trasparente a tutti i call site |
| MAG-1 | **Multi-agent: agent definitions** | 1 settimana | ✅ DONE (2026-03-19) — `AgentDefinition` in `agent/definition.rs`: struct con model/instructions/tools/skills/concurrency. `AgentDefinitionConfig` in schema.rs per parsing `[agents.*]` TOML. `AgentInstructionsSection` nel prompt builder. `with_agent_definition()` su AgentLoop (tool+skill filter). Gateway risolve definizioni in `run()`. 11 test. Backward-compat: nessun `[agents.*]` → "default" sintetizzato da `[agent]` |
| MAG-2 | **Multi-agent: registry + config router** | 1 settimana | ✅ DONE (2026-03-19) — `AgentRegistry` in `agent/registry.rs`: pool di N AgentLoop (uno per definizione), `route()` config-based (contact.agent_override > channel.default_agent > "default"). `default_agent` su ChannelBehavior trait (7 config canale). Migration 025 per `contact.agent_override`. Gateway usa registry con routing nel debounce dispatch. `for_each_mut()` per setter condivisi. 3 test |
| MAG-3 | **Multi-agent: LLM router** | 3 giorni | ✅ DONE (2026-03-19) — `classify_message()` in registry.rs: fast LLM one-shot classifica il messaggio e sceglie l'agente. `RoutingConfig` con `classifier_model` (vuoto = disabilitato). Cache in-memory session_key→agent_id. Prompt auto-generato dalle definizioni agente. Timeout 5s, validazione risposta, fallback graceful. 5 test |
| MAG-4 | **Multi-agent: pipeline orchestration** | 1 settimana | ✅ DONE (2026-03-19) — `agent_id` per workflow step (migration 026). `WorkflowEngine` usa `Arc<AgentRegistry>` invece di `Arc<AgentLoop>`. `execute_step()` fa lookup agente via `registry.get(&step.agent_id)`. Tool `workflow` espone parametro `agent_id` per step. Backward-compat: default a "default". Parallel steps rimandati |

#### 1A-bis. Memory Hardening (P1)

> Ispirato a MemCompressor (arxiv 2512.24601). Migliora qualità retrieval e previene crescita infinita.

| # | Task | Effort | Note |
|---|------|--------|------|
| MEM-1 | **Contact-aware consolidation** | ✅ DONE 2026-03-19 | `consolidate()` riceve `contact_id` dal caller. `resolve_contact_from_session()` parsa session_key → channel:chat_id → contact. Memory search usa `search_scoped()` con contact_id |
| MEM-2 | **Agent-scoped memory chunks** | ✅ DONE 2026-03-19 | Migration 027: `agent_id TEXT` su `memory_chunks`. `search_scoped_full()` filtra per agent_id + contact_id. AgentLoop passa `agent_definition.id` a consolidation |
| MEM-3 | **Importance scoring** | ✅ DONE 2026-03-19 | Migration 028: `importance INTEGER DEFAULT 3`. LLM assegna 1-5 via `ScoredInstruction` (con backward compat per plain string). Score finale = RRF × (importance/3) × temporal_decay |
| MEM-4 | **Memory budget + pruning** | ✅ DONE 2026-03-19 | Config `agent.max_memory_chunks` (default 1000). Post-consolidation: `prune_memory_chunks_to_budget()` elimina chunks con importance basso + vecchi. HNSW index aggiornato |
| MEM-5 | **Hierarchical summarization** | ✅ DONE 2026-03-19 | Migration 029: tabella `memory_summaries`. `maybe_summarize_period()` crea digest settimanali (lunedì) e mensili (primi 3 giorni mese). LLM summarization idempotente |

Sequenza completata: MEM-1 → MEM-2 → MEM-3 → MEM-4 → MEM-5.

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
| TST-1 | **E2E Playwright CI** | ✅ DONE 2026-03-19 | `e2e-ci.yml`: automated on push/PR (web UI paths). Builds binary, starts server in setup mode (no LLM), runs `e2e_ci_suite.sh` (UI-structural smoke). Server auto-start/stop, artifact upload. Manual `e2e-smoke.yml` retained for full LLM-powered suite |
| TST-3 | **Channel integration tests** | ✅ DONE (2026-03-18) | 8 integration test in channels/mod.rs: health lifecycle, capabilities coverage, degradation/recovery, proactive routing, Slack Socket Mode toggle, config defaults |
| TST-4 | **CI sandbox validation** | ✅ DONE (2026-03-18) | `sandbox-validation.yml`: automated on push/PR (sandbox paths). Linux native (bwrap), runtime image build, cross-platform E2E (Linux/Windows/macOS) |

#### 1D. UX Beta (P1)

| # | Task | Effort | Note |
|---|------|--------|------|
| ONB-1 | **Setup wizard v2** | 1 settimana | ✅ DONE (2026-03-18) — 4 step (provider → model → test → first message), localStorage checkpoint con expiry 24h, resume su reload, redirect a /chat per step 4 |
| ONB-2 | **Flusso Ollama locale** | 3 giorni | ✅ DONE (2026-03-18) — Auto-detect Ollama nel wizard, banner "No API key?", select modello + pull + activate one-click. Suggerisce llama3.2:3b/gemma3:4b se nessun modello installato |
| ONB-4 | **Onboarding Experience** | 2 settimane | ✅ DONE (2026-03-18) — Redesign completo v2: usa `page_html()` con vera nav bar, 4 fasi (Welcome+theme/accent, Provider+Model, Channels, Meet Homun con chat LLM streaming), accent-utils.js condiviso, i18n EN+IT, mobile responsive |
| AUD-2 | **Feature gating doc** | — | ✅ Assorbito in ONB-4 |
| CHR-1 | **Chat rename UX** | — | ✅ DONE (2026-03-18) — Optimistic update, no more timing gap, blur guard |
| CTC-1 | **Contacts UX overhaul** | — | ✅ DONE (2026-03-18) — Pagina ristrutturata come Workflows (page-header + badge + inline form), identità nel form creazione, form inline per add identity/relationship/event, nomi contatto nelle relazioni |
| HRL-1 | **Channel hot-reload** | — | ✅ DONE (2026-03-18) — `ChannelCommand::Start` + `SharedOutboundSenders`, canali avviabili senza restart gateway, auto-trigger dopo WhatsApp pairing, `POST /v1/channels/{name}/start` |
| PRO-4 | **Proactive messaging contacts** | — | ✅ DONE (2026-03-18) — `known_chat_ids` pre-seeded da identità contatti, tool messages pre-registrano chat_id, `contacts send` restituisce chat_id corretto (JID WhatsApp) |
| BHV-1 | **Unified channel behavior** | — | ✅ DONE (2026-03-18) — Assisted/on_demand/silent per tutti i canali (non solo email). Config: `notify_channel` + `notify_chat_id` su Telegram/WhatsApp/Discord/Slack. `pending_responses` con notify routing (migration 021). Approval interception generico. UI Behavior section nel modale Channels |
| WA-1 | **WhatsApp self-message filter** | — | ✅ DONE (2026-03-18) — Messaggi `is_from_me` ignorati (non solo bot echo, anche messaggi scritti dal telefono) |

#### 1E. Cognition-First Architecture (P1)

> Obiettivo: sostituire il routing keyword-based con una fase di cognizione LLM-driven che analizza l'intent dell'utente prima dell'execution loop. Feature-gated via `cognition_enabled`.

| # | Task | Effort | Note |
|---|------|--------|------|
| COG-1 | **Cognition engine** | 3 giorni | ✅ DONE (2026-03-21) — `agent/cognition/engine.rs`: mini ReAct loop con discovery tools, analizza intent e produce `CognitionResult` (understanding, plan, constraints, tools/skills/MCP/memory/RAG). Config: `cognition_enabled`, `cognition_model`, `cognition_max_iterations`, `cognition_timeout_secs` |
| COG-2 | **Discovery tools** | 2 giorni | ✅ DONE (2026-03-21) — `agent/cognition/discovery.rs`: 5 read-only tools (memory_search, rag_search, list_tools, list_skills, list_mcp). Non modificano stato, solo raccolgono contesto |
| COG-3 | **Selective tool loading** | 1 giorno | ✅ DONE (2026-03-21) — `build_selective_tool_defs()` in cognition/mod.rs: solo i tool identificati dalla cognizione passati al LLM (+ always-available: send_message, remember, approval) |
| COG-4 | **System prompt integration** | 1 giorno | ✅ DONE (2026-03-21) — Understanding/plan/constraints iniettati in `ToolsSection` al posto delle routing rules keyword. Browser essentials sempre presenti |
| COG-5 | **Browser task plan from cognition** | 1 giorno | ✅ DONE (2026-03-21) — `BrowserTaskPlanState::from_cognition()`: inizializza da CognitionResult invece di keyword matching |
| COG-6 | **Tool veto safety-net mode** | 1 giorno | ✅ DONE (2026-03-21) — Quando cognition attiva, solo veto minimali (search-first, shell-not-for-web). Full keyword vetoes solo con cognition off |
| COG-7 | **Dual-path fallback** | — | ✅ DONE (2026-03-21) — Quando `cognition_enabled=false` (default), tutto il vecchio path keyword funziona invariato: blind memory/RAG injection, full tool set, keyword browser routing, full keyword vetoes |
| COG-8 | **answer_directly fast-path** | — | ✅ DONE (2026-03-21) — Richieste semplici (saluti, domande fattuali) risposte direttamente dalla cognizione senza entrare nell'execution loop |
| COG-9 | **E2E tests** | 1 giorno | ✅ DONE (2026-03-21) — Test cognition engine, discovery tools, selective tool defs, dual-path |

---

### Fase 2: Apertura al Mondo Esterno (8-12 settimane)

> Obiettivo: Homun non è più solo localhost. È raggiungibile, usabile come backend, con trust model chiaro.
> Criterio: un utente può usare Homun da remoto in modo sicuro e documentato.

#### 2A. Remote Access (P0)

| # | Task | Effort | Note |
|---|------|--------|------|
| REM-1 | **Remote access story ufficiale** | ✅ DONE 2026-03-19 | `docs/REMOTE-ACCESS.md`: 3 pattern (SSH tunnel, Tailscale Serve, Caddy/nginx reverse proxy), security checklist, device approval flow docs, config examples |
| REM-2 | **X-Forwarded-For awareness** | ✅ DONE 2026-03-19 | `extract_client_ip()` in auth.rs: parsa X-Forwarded-For (leftmost IP) quando `trust_x_forwarded_for = true`. Entrambi i rate limiter (auth + API) aggiornati. Default false per sicurezza |
| REM-3 | **Trusted device model** | ✅ DONE 2026-03-19 | Migration 030: `trusted_devices` table. Fingerprint = SHA-256(user_id + User-Agent). Login handler: blocca device sconosciuti con codice 6 cifre. `device_approve_handler` per OTP approval. API: `/v1/devices` (list/approve/revoke). Login page: UI code input per device approval. Config: `require_device_approval` (default false) |
| REM-4 | **Session hardening per remoto** | ✅ DONE 2026-03-19 | CSRF: token in `WebSession` + `homun_csrf` cookie (JS-readable) + `csrf_guard_middleware` valida `X-CSRF-Token`. Session binding: IP + User-Agent catturati al login, warning su drift. TTL configurabile via `session_ttl_secs`. SameSite=Strict. 5 nuovi test |

#### 2B. API Esterne Compatibili (P1)

| # | Task | Effort | Note |
|---|------|--------|------|
| API-1 | **OpenAI Chat Completions compat** | ✅ DONE 2026-03-19 | `POST /v1/chat/completions` in `openai.rs`. Full agent loop (tools, memory, skills). Non-streaming (JSON) + streaming (SSE). Bearer auth. Session routing via `session_id`. Filtra stream a content-only (skip tool events). Compatible con Python `openai` SDK |
| API-2 | **Session routing API** | ✅ DONE 2026-03-19 | `sessions.rs`: GET/POST/DELETE `/v1/sessions`, GET `/v1/sessions/{id}/messages`. Prefisso `api:` per distinguere da `web:`. CRUD completo con message history |
| API-3 | **API docs (OpenAPI spec)** | ✅ DONE 2026-03-19 | `docs/openapi.yaml`: OpenAPI 3.1 spec per endpoint principali (chat completions, sessions, devices, health). Copre auth, request/response schemas, SSE streaming |

#### 2C. Trust Model Esplicito (P1)

| # | Task | Effort | Note |
|---|------|--------|------|
| TRS-1 | **Trust model doc** | ✅ DONE 2026-03-19 | `docs/TRUST-MODEL.md`: 8 principal types (local admin, remote admin, API client, channel sender, webhook, MCP, agent, cron). Trust boundaries, content trust levels, security controls summary |
| TRS-2 | **Vault access audit log** | ✅ DONE | Migration 019, fire-and-forget audit, `GET /v1/vault/audit` |

#### 2C-bis. API Access Management (P1)

| # | Task | Effort | Note |
|---|------|--------|------|
| AAM-1 | **Scope enforcement globale** | ✅ DONE 2026-03-20 | `require_write()`/`require_admin()`/`check_write()`/`check_admin()` in auth.rs. Applicato a tutti i mutating handler (~40 endpoint): workflows, contacts, memory, knowledge, email_accounts, mcp/crud, mcp/oauth, mcp/install, skills, automations, chat, vault, sandbox, maintenance, devices, business, account. Bearer token scope (admin/write/read) enforced, session = sempre admin |
| AAM-2 | **Token expiry** | ✅ DONE 2026-03-20 | Migration 031: `expires_at TEXT` su `webhook_tokens`. Dual check: Rust middleware (chrono parse) + SQL WHERE clause. API `expires_in` (7d/30d/90d/never) → RFC-3339 `expires_at` |
| AAM-3 | **Token masking + token_id** | ✅ DONE 2026-03-20 | Lista token: `token_id` (primi 16 char) + `display_token` (mascherato `wh_****…abcd`). Token completo visibile solo alla creazione. Delete/toggle via `find_token_by_prefix()` |
| AAM-4 | **Per-token rate limiting** | ✅ DONE 2026-03-20 | `RateLimiter<K>` generico (default IpAddr). `token_rate_limiter: RateLimiter<String>` in AppState, 60 req/min per token. Check nel Bearer auth flow |
| AAM-5 | **Pagina Web UI API Keys** | ✅ DONE 2026-03-20 | `/api-keys` page con create form (name, scope, expiry), one-time token reveal + copy, lista chiavi con badges (scope, expiry, enabled), toggle/delete. `static/js/api-keys.js` (~220 righe). Sezione token rimossa da account page |

#### 2D. MCP Recipes Expansion (P1)

| # | Task | Effort | Note |
|---|------|--------|------|
| RCP-1 | **Google services consolidation** | ✅ DONE 2026-03-20 | Consolidated gmail, google-calendar, google-drive into `google-workspace.toml` (covers Gmail, Calendar, Drive via single OAuth). 3 recipes removed, 1 unified |
| RCP-3 | **Linear recipe** | ✅ DONE 2026-03-20 | `recipes/linear.toml`: API key, `mcp-linear` (env: LINEAR_ACCESS_TOKEN) |
| RCP-4 | **Jira recipe** | ✅ DONE 2026-03-20 | `recipes/jira.toml`: API key + site name, `@aashari/mcp-server-atlassian-jira` (env: ATLASSIAN_SITE_NAME, ATLASSIAN_USER_EMAIL, ATLASSIAN_API_TOKEN) |
| RCP-5 | **Reddit recipe** | ✅ DONE 2026-03-20 | `recipes/reddit.toml`: OAuth (5 fields), `reddit-mcp-server` |
| RCP-6 | **GitLab recipe** | ✅ DONE 2026-03-20 | `recipes/gitlab.toml`: API key + optional URL, `@zereight/mcp-gitlab` (env: GITLAB_API_URL) |
| RCP-7 | **Brave Search recipe** | ✅ DONE 2026-03-20 | `recipes/brave-search.toml`: API key, `@anthropic/brave-search-mcp-server` |
| RCP-8 | **Spotify recipe** | ✅ DONE 2026-03-20 | `recipes/spotify.toml`: OAuth (client_id + secret + refresh_token), `mcp-spotify` |
| RCP-9 | **Stripe recipe** | ✅ DONE 2026-03-20 | `recipes/stripe.toml`: API key, `@stripe/agent-toolkit` |
| RCP-10 | **Twitter/X recipe** | ✅ DONE 2026-03-20 | `recipes/twitter.toml`: OAuth 1.0a (4 keys), `@kms_dev/x-mcp` (official X SDK). 7 tools: post, search, timeline, like, retweet, delete, user info |
| RCP-11 | **Sentry recipe** | ✅ DONE 2026-03-20 | `recipes/sentry.toml`: API key + org slug, `mcp-server-sentry` (env: SENTRY_ACCESS_TOKEN) |
| RCP-12 | **Todoist recipe** | ✅ DONE 2026-03-20 | `recipes/todoist.toml`: API key, `mcp-server-todoist` |
| RCP-13 | **Home Assistant recipe** | ✅ DONE 2026-03-20 | `recipes/home-assistant.toml`: URL + token, `mcp-server-home-assistant` |
| RCP-14 | **Google Maps recipe** | ✅ DONE 2026-03-20 | `recipes/google-maps.toml`: API key, `@modelcontextprotocol/server-google-maps`. ⚠️ Deprecated pkg |
| RCP-15 | **WordPress recipe** | ✅ DONE 2026-03-20 | `recipes/wordpress.toml`: URL + username + app password, `mcp-wordpress` |
| RCP-V1 | **NPM package verification** | ✅ DONE 2026-03-20 | All 17 recipes verified against npm registry. Fixed 7 wrong package names, 6 wrong env vars. Added `deprecated_notice` field + UI badge for deprecated packages (GitHub, Google Maps) |
| RCP-UX1 | **Setup guides for all recipes** | ✅ DONE 2026-03-20 | `setup_guide` field added to `ConnectionRecipe`. All 17 recipes have detailed markdown step-by-step guides |
| RCP-UX2 | **Split-pane connect modal** | ✅ DONE 2026-03-20 | Form fields left, setup guide right (marked.js for markdown). Independent scrolling. Mobile stacks vertically. `.skill-modal--split` CSS modifier |
| RCP-FN1 | **Runtime OAuth token refresh** | ✅ DONE 2026-03-20 | `is_auth_error()` detection + `try_refresh_and_retry()` in `tools/mcp.rs`. On auth error during tool call: auto-refresh OAuth token (Google/Notion) and retry once. Fixes Notion disconnection issue |

Target: **17 verified recipes bundled** — all done. Google services consolidated (3→1).

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

| # | Task | Effort | Note |
|---|------|--------|------|
| WEB-1 | Landing page (hero, features, CTA download) | 1 settimana | ⚠️ Esiste (Vite + Tailwind, repo `website/`) — da rivedere |
| WEB-2 | Pagina download (detect OS, link per piattaforma) | 3 giorni | ⚠️ Da rivedere |
| WEB-3 | Pagina features (screenshot/GIF per macro-feature) | 1 settimana | ⚠️ Da rivedere |
| WEB-7 | Dominio + hosting (homun.dev, Cloudflare Pages) | 1 giorno | ✅ DONE |

#### 3B. Docs Site

| # | Task | Effort | Note |
|---|------|--------|------|
| DOC-1 | Infrastruttura (Next.js, docs.homun.dev) | 2 giorni | ✅ DONE — repo `homun-docs/`, Next.js + MDX, Dockerized |
| DOC-2 | Guida installazione per piattaforma | 3 giorni | ✅ DONE — source.mdx, docker.mdx, service.mdx |
| DOC-3 | Guida configurazione (ogni sezione config.toml) | 3 giorni | ✅ DONE — providers.mdx, security.mdx, remote-access.mdx |
| DOC-4 | Guida canali (setup per ogni canale con screenshot) | 1 settimana | ✅ DONE — 7 canali documentati (telegram, discord, slack, whatsapp, email, web, cli) |
| DOC-5 | Guida automazioni (builder + NLP + template gallery) | 3 giorni | ✅ DONE — automations.mdx |
| DOC-6 | Guida skills/MCP | 3 giorni | ✅ DONE — skills.mdx |
| DOC-8 | Troubleshooting/FAQ (top 20 problemi) | 3 giorni | ✅ DONE — troubleshooting/index.mdx |
| DOC-9 | Contributing guide | 2 giorni | ⏳ TODO |

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
| DRY refactor (R1-R7) | ✅ DONE 2026-03-19 | `utils/text.rs` (truncate), `utils/watcher.rs` (WatcherHandle), `Config::*_dir()`, dead code, naming. 37 file, -171 LOC nette |
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
