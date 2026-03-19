# Refactor Report — 2026-03-19

> Analisi strutturale completa del codebase Homun.
> Metodologia: scansione automatica con rg/grep su tutte le categorie definite nel CLAUDE.md.
> Stato: **R1-R3, R5-R7 completati** (2026-03-19). R4, R8+ in backlog.

---

## Sintesi

| Livello | Conteggio | Descrizione |
|---------|-----------|-------------|
| **CRITICO** | 4 | Duplicazioni strutturali e trait mancanti che bloccano estensibilità |
| **MEDIO** | 5 | Violazioni convenzioni (file size, naming, sync FS, SQL fuori db) |
| **MINORE** | 2 | Doc comments mancanti, dead code annotations |

**Metriche codebase al momento della scansione:**
- 97,872 LOC Rust (72 file > 400 righe)
- 22,234 LOC JS (17 file > 400 righe)
- 0 warning da `cargo check`
- 690+ test passing

---

## 1. Trait Mancanti [CRITICO]

### 1.1 SkillSource — Installer/Search unificato

**Struct coinvolte:**
- `SkillInstaller` (`src/skills/installer.rs`)
- `ClawHubInstaller` (`src/skills/clawhub.rs`)
- `OpenSkillsSource` (`src/skills/openskills.rs`)
- `SkillSearcher` (`src/skills/search.rs`)

**Metodi condivisi:**
| Metodo | SkillInstaller | ClawHubInstaller | OpenSkillsSource |
|--------|---------------|-----------------|-----------------|
| `new(client, skills_dir)` | yes | yes | yes |
| `search(query, limit) -> Vec<Result>` | via SkillSearcher | yes | yes |
| `install(id) -> InstallResult` | yes | yes | yes |
| `install_with_options(id, opts) -> InstallResult` | yes | yes | yes |

Campi identici: `client: Client` + `skills_dir: PathBuf` in tutti e tre.

Inoltre 3 tipi di risultato separati (`SkillSearchResult`, `ClawHubSearchResult`, `OpenSkillsResult`) con campi sovrapposti (name, description, source/url).

**Trait proposto:**
```rust
/// Unified interface for skill sources (GitHub, ClawHub, OpenSkills).
pub trait SkillSource: Send + Sync {
    fn name(&self) -> &str;
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SkillSearchResult>>;
    async fn install(&self, identifier: &str, options: InstallOptions) -> Result<InstallResult>;
}
```

**Impatto:** 5 file (`installer.rs`, `clawhub.rs`, `openskills.rs`, `search.rs`, `skills/mod.rs`). ~200 LOC ridotte.

---

### 1.2 WatcherHandle — Struct identica copiata 3 volte

**Struct coinvolte:**
- `SkillWatcher` + `WatcherHandle` (`src/skills/watcher.rs`)
- `RagWatcher` + `WatcherHandle` (`src/rag/watcher.rs`)
- `BootstrapWatcher` + `WatcherHandle` (`src/agent/bootstrap_watcher.rs`)

**Codice identico (copia-incolla):**
```rust
pub struct WatcherHandle {
    stop_tx: Option<oneshot::Sender<()>>,
    join_handle: Option<JoinHandle<()>>,
}
impl Drop for WatcherHandle { /* identico in tutti e 3 */ }
```

Anche `start() -> WatcherHandle` e lo scheletro di `watch_loop()` (create RecommendedWatcher, debounce, reload) sono strutturalmente identici.

**Astrazione proposta:**
```rust
// src/utils/watcher.rs
pub struct WatcherHandle { ... }  // una sola copia
impl Drop for WatcherHandle { ... }

pub fn spawn_watcher<F>(paths: Vec<PathBuf>, debounce_ms: u64, on_change: F) -> WatcherHandle
where F: Fn() + Send + 'static;
```

**Impatto:** 3 file + 1 nuovo (`utils/watcher.rs`). ~120 LOC di boilerplate eliminate (40 LOC x 3).

---

## 2. Duplicazioni [CRITICO]

### 2.1 Funzioni `truncate` — 12 implementazioni + 11 inline

**Il problema più grave di DRY nel codebase.** Esistono 12 funzioni separate che troncano stringhe e 11 pattern inline `.chars().take(N).collect()`.

**Funzioni duplicate:**

| File | Funzione | Suffisso |
|------|----------|----------|
| `src/workflows/engine.rs:597` | `truncate_for_context()` | `...` |
| `src/workflows/mod.rs:359` | `truncate()` | `...` (unicode) |
| `src/tools/approval.rs:269` | `truncate()` | `...` |
| `src/tools/email_inbox.rs:221` | `truncate_chars()` | `...` |
| `src/agent/gateway.rs:2083` | `truncate_for_status()` | `...` |
| `src/scheduler/automations.rs:707` | `truncate_label()` | `...` (unicode) |
| `src/tools/shell.rs:354` | `truncate_output()` | — |
| `src/security/exfiltration.rs:401` | `truncate_for_log()` | — |
| `src/web/api/chat.rs:345` | `truncate_conversation_label()` | — |
| `src/tools/browser.rs:1602` | `truncate_utf8()` | byte-level |
| `src/agent/agent_loop.rs:3277` | `truncate_utf8()` | byte-level |
| `src/agent/attachment_router.rs:506` | `truncate_analysis()` | — |

**Identiche character-for-character:** `truncate_utf8()` in `browser.rs:1602` e `agent_loop.rs:3277`.

**Inline (ad-hoc `.chars().take(N).collect()`):**
- `src/main.rs:2614` (take 200)
- `src/agent/memory.rs:275` (take 500)
- `src/agent/memory.rs:1050` (take 200)
- `src/agent/gateway.rs:1434` (take 500)
- `src/agent/browser_task_plan.rs:38` (take 220)
- `src/web/auth.rs:797` (take 80)
- altri 5 sparsi

**Fix proposto:**
```rust
// src/utils/text.rs (nuovo file, ~20 righe)
/// Truncate a string to `max_chars` characters, appending `suffix` if truncated.
pub fn truncate_str(s: &str, max_chars: usize, suffix: &str) -> String { ... }

/// Truncate to `max_bytes` preserving UTF-8 boundary.
pub fn truncate_utf8(s: &str, max_bytes: usize) -> &str { ... }
```

**Impatto:** 12 file modificati + 1 nuovo. ~150 LOC rimosse nette.

---

### 2.2 `dirs::home_dir().join(".homun")` — ~35 occorrenze

`Config::data_dir()` esiste già in `src/config/schema.rs:103` ma **~35 call site** costruiscono il path manualmente.

**Peggiori offenders:**

| File | Occorrenze | Pattern |
|------|-----------|---------|
| `src/tools/file.rs` | 5 | `dirs::home_dir()` per path expansion |
| `src/web/api/skills.rs` | 4 | `.join(".homun").join("skills")` |
| `src/skills/installer.rs` | 3 | `.join(".homun").join("skills")` |
| `src/web/server.rs` | 3 | TLS dir |
| `src/skills/openskills.rs` | 2 | `.join(".homun").join("skills")` |
| `src/skills/loader.rs` | 1 | `.join(".homun").join("skills")` |
| `src/skills/clawhub.rs` | 1 | `.join(".homun").join("skills")` |
| `src/mcp_setup.rs` | 1 | MCP config path |
| altri 15 file | 1 ciascuno | vari subdirectory |

**Fix proposto:** Aggiungere convenience methods a `Config`:
```rust
impl Config {
    pub fn skills_dir() -> PathBuf { Self::data_dir().join("skills") }
    pub fn brain_dir() -> PathBuf { Self::data_dir().join("brain") }
    pub fn tls_dir() -> PathBuf { Self::data_dir().join("tls") }
    pub fn cache_dir() -> PathBuf { Self::data_dir().join("cache") }
}
```
E sostituire tutti i call site.

**Impatto:** ~15 file, ~35 sostituzioni. Rischio basso (refactor meccanico).

---

## 3. File da Splittare [MEDIO]

### File Rust > 500 righe (non grandfathered)

Questi file superano il limite di 500 righe e **non sono nella lista grandfathered** del CLAUDE.md:

| File | Righe | Proposta di split |
|------|-------|-------------------|
| `src/web/api/providers.rs` | 1520 | Split per dominio: `providers_crud.rs` + `providers_models.rs` + `providers_test.rs` |
| `src/web/auth.rs` | 1386 | `auth.rs` (login/session) + `auth/csrf.rs` + `auth/devices.rs` + `auth/rate_limit.rs` |
| `src/web/api/channels.rs` | 1137 | `channels_crud.rs` + `channels_pairing.rs` |
| `src/web/api/mcp/install.rs` | 1024 | `install.rs` (core) + `install_recipe.rs` (recipe logic) |
| `src/web/api/mcp/helpers.rs` | 1003 | `helpers.rs` (core) + `helpers_oauth.rs` |
| `src/tools/mcp.rs` | 984 | `mcp.rs` (tool) + `mcp/client.rs` (MCP client logic) |
| `src/tools/file.rs` | 983 | `file.rs` (read/write) + `file_list.rs` (list/search) |
| `src/web/api/automations.rs` | 955 | `automations_crud.rs` + `automations_generation.rs` |
| `src/provider/ollama.rs` | 934 | `ollama.rs` (provider) + `ollama_models.rs` (model management) |
| `src/agent/browser_task_plan.rs` | 917 | `browser_task_plan.rs` (planning) + `browser_task_state.rs` (state tracking) |
| `src/tui/ui.rs` | 910 | `ui.rs` (layout) + `ui_widgets.rs` (custom widgets) |
| `src/web/api/skills.rs` | 894 | `skills_crud.rs` + `skills_marketplace.rs` |
| `src/scheduler/automations.rs` | 879 | `automations.rs` (engine) + `automations_plan.rs` (plan/validation) |
| `src/agent/prompt/sections.rs` | 877 | `sections.rs` (core) + `sections_tools.rs` + `sections_context.rs` |
| `src/provider/openai_compat.rs` | 860 | `openai_compat.rs` (provider) + `openai_compat_stream.rs` (SSE) |
| `src/tools/shell.rs` | 857 | `shell.rs` (execution) + `shell_parse.rs` (output parsing) |
| `src/contacts/db.rs` | 847 | `db.rs` (CRUD) + `db_queries.rs` (complex queries) |
| `src/web/api/chat.rs` | 845 | `chat.rs` (handlers) + `chat_streaming.rs` (SSE) |
| `src/web/api/mcp/oauth.rs` | 841 | `oauth.rs` (flow) + `oauth_providers.rs` (provider-specific) |
| `src/web/api/vault.rs` | 818 | `vault.rs` (CRUD) + `vault_2fa.rs` (TOTP) |
| `src/tools/sandbox/mod.rs` | 776 | Già submodule, ma mod.rs troppo grande → estrarre `sandbox/manager.rs` |
| `src/agent/execution_plan.rs` | 774 | `execution_plan.rs` (types) + `execution_plan_runner.rs` (orchestration) |
| `src/scheduler/cron.rs` | 756 | `cron.rs` (scheduler) + `cron_jobs.rs` (job types) |
| `src/channels/slack.rs` | 721 | `slack.rs` (channel) + `slack_socket.rs` (Socket Mode) |
| `src/skills/creator.rs` | 710 | `creator.rs` (LLM gen) + `creator_templates.rs` (prompts) |
| `src/tools/sandbox/runtime_image.rs` | 697 | `runtime_image.rs` (status) + `runtime_image_build.rs` (build/pull) |
| `src/rag/engine.rs` | 682 | `engine.rs` (search) + `engine_ingest.rs` (ingestion) |
| `src/tools/business.rs` | 670 | `business.rs` (tool) + `business_actions.rs` (OODA actions) |
| `src/business/db.rs` | 667 | `db.rs` (CRUD) + `db_analytics.rs` (aggregazioni) |
| `src/channels/whatsapp.rs` | 665 | `whatsapp.rs` (channel) + `whatsapp_pairing.rs` |
| `src/security/exfiltration.rs` | 657 | `exfiltration.rs` (guard) + `exfiltration_patterns.rs` (detection) |
| `src/provider/anthropic.rs` | 654 | `anthropic.rs` (provider) + `anthropic_stream.rs` (SSE) |
| `src/agent/embeddings.rs` | 647 | `embeddings.rs` (provider) + `embeddings_local.rs` (fastembed) |
| `src/agent/attachment_router.rs` | 644 | Accettabile (singola responsabilità) |
| `src/skills/installer.rs` | 621 | Accettabile dopo trait SkillSource |
| `src/workflows/engine.rs` | 605 | `engine.rs` (orchestration) + `engine_steps.rs` (step execution) |
| `src/channels/telegram.rs` | 582 | Accettabile (singola responsabilità) |
| `src/storage/secrets.rs` | 579 | `secrets.rs` (vault) + `secrets_keychain.rs` (OS keyring) |
| `src/tools/sandbox/resolve.rs` | 552 | Accettabile (singola responsabilità) |
| `src/agent/debounce.rs` | 541 | Accettabile (singola responsabilità) |
| `src/agent/context.rs` | 528 | Accettabile |
| `src/skills/openskills.rs` | 521 | Accettabile dopo trait SkillSource |
| `src/utils/retry.rs` | 520 | Accettabile |
| `src/tools/web.rs` | 520 | Accettabile |
| `src/rag/chunker.rs` | 516 | Accettabile |
| `src/web/api/sandbox.rs` | 510 | Accettabile |
| `src/tools/contacts.rs` | 503 | Accettabile |
| `src/agent/memory_search.rs` | 501 | Accettabile (borderline) |

**Totale file Rust > 500 righe non grandfathered: 45**
(Quelli < 550 sono borderline e accettabili, i > 700 sono prioritari per lo split.)

### File JS > 500 righe (non grandfathered)

| File | Righe | Note |
|------|-------|------|
| `connections.js` | 798 | Non grandfathered, da valutare split |
| `flow-renderer.js` | 764 | Non grandfathered, rendering engine |
| `account.js` | 596 | Non grandfathered |
| `memory.js` | 572 | Non grandfathered |
| `onboarding.js` | 565 | Non grandfathered |
| `workflows.js` | 557 | Non grandfathered |
| `sandbox.js` | 556 | Non grandfathered |
| `file-access.js` | 543 | Non grandfathered |
| `vault.js` | 539 | Non grandfathered |
| `contacts.js` | 494 | Borderline |
| `business.js` | 432 | Borderline |
| `dashboard.js` | 423 | Borderline |

---

## 4. Naming Violations [MEDIO]

### 4.1 Booleani senza prefisso — ~130+ campi

Il CLAUDE.md richiede `is_`, `has_`, `can_`, `should_` per booleani. Solo ~10 campi su ~140 rispettano la convenzione.

**Cluster principali:**

| Area | Campi violanti | Esempio |
|------|---------------|---------|
| `config/schema.rs` | ~40 | `enabled`, `headless`, `stealth`, `multimodal`, `pairing_required`, `auto_tls`, `strict` |
| `channels/capabilities.rs` | 12 | `inbound_text`, `outbound_attachments`, `proactive_send`, `markdown_support` |
| `tools/sandbox/types.rs` | 18 | `sanitize_env`, `docker_available`, `user_namespace`, `cgroup_v2_available` |
| `security/estop.rs` | 4 | `stop_requested`, `network_offline`, `browser_closed`, `mcp_shutdown` |
| `contacts/mod.rs` | 2 | `done`, `approved` in `PendingResponse` |
| `bus/queue.rs` | 3 | `requires_approval`, `done`, `success` |
| `skills/*.rs` | 8 | `eligible`, `cached`, `stale`, `force`, `overwrite` |
| `provider/traits.rs` | 1 | `done` in `StreamDelta` |
| `storage/db.rs` | 1 | `sensitive` |

**Nota:** Rinominare `enabled` → `is_enabled` su ~15 config struct è un refactor ampio che tocca serializzazione TOML, config.toml degli utenti, e API JSON. Richiede migration strategy (serde alias).

### 4.2 Abbreviazione `mgr`

~30 usi di `mgr` come variabile locale, concentrati in:
- `src/tools/approval.rs` (test code)
- `src/web/api/approvals.rs` (production code — `approval_mgr`)

Severità bassa (variabili locali, non API pubblica), ma viola la regola "Mai `mgr`".

---

## 5. Doc Comments Mancanti [MINORE]

### 5.1 Moduli senza `//!` module-level docs

17 su 28 `mod.rs` mancano di `//!` docs:

`agent`, `bus`, `channels`, `config`, `provider`, `rag`, `scheduler`, `session`, `skills`, `storage`, `tools`, `tools/sandbox`, `tools/sandbox/backends`, `tui`, `web`, `web/api`, `web/api/mcp`

11 moduli sono conformi: `agent/prompt`, `browser`, `business`, `connections`, `contacts`, `queue`, `security`, `service`, `user`, `utils`, `workflows`.

### 5.2 Public items senza `///`

| Categoria | Stima violazioni | Cluster peggiori |
|-----------|-----------------|------------------|
| `pub struct` | ~150+ | `config/schema.rs` (~20), `storage/db.rs` (~18), `business/mod.rs` (~8), `web/auth.rs` (~9), `bus/queue.rs` (~5) |
| `pub fn` | ~250+ | `storage/db.rs` (~25), `contacts/db.rs` (~20), `tui/app.rs` (~30), `tools/*.rs` constructors, `web/api/*.rs` handlers |
| `pub enum` | ~15 | `business/mod.rs` (6), `workflows/mod.rs` (3), `provider/traits.rs` (1) |
| `pub trait` | 0 | Tutti i trait sono documentati |

### 5.3 Dead code annotations

Solo 4 `#[allow(dead_code)]`:

| File | Item | Motivo |
|------|------|--------|
| `src/channels/slack.rs:650` | `ConversationsHistoryResponse.error` | Campo API deserializzato ma non letto |
| `src/skills/clawhub.rs:124` | `ClawHubApiSkillDetail.skill` | Campo API deserializzato ma non usato |
| `src/agent/verifier.rs:16` | `VerificationResult::NeedsVerification` | Variante mantenuta per compatibilità API |
| `src/agent/gateway.rs:183` | `Gateway.session_manager` | Stored ma mai acceduto |

Nessun `#[allow(unused_*)]` problematico (1 è cfg-conditional, 1 indica codice incompleto).

---

## 6. Altre Osservazioni [MEDIO]

### 6.1 `reqwest::Client::new()` ad-hoc (3 occorrenze)

Ogni chiamata crea un nuovo client HTTP senza connection pooling:

| File | Contesto |
|------|----------|
| `src/skills/security.rs:697` | VirusTotal API (HTTPS esterno) |
| `src/main.rs:2324` | POST a localhost |
| `src/agent/embeddings.rs:72` | OpenAI embeddings (memorizzato come field) |

### 6.2 `sqlx::query` fuori dai file DB dedicati (4 file)

| File | Query |
|------|-------|
| `src/web/api/memory.rs:337` | `query_as` per memory chunks |
| `src/web/api/maintenance.rs` | 4 query COUNT + DELETE |
| `src/web/api/health.rs:148` | `SELECT 1` health check |
| `src/main.rs:2444-2453` | COUNT per `status` command |

### 6.3 `std::fs::` sync in contesto async (39 file)

39 file non-test usano `std::fs::` sync. I più problematici (in path async hot):
- `src/agent/context.rs` — `read_to_string` per brain files
- `src/agent/memory.rs` — `write`, `OpenOptions` in consolidation
- `src/web/api/knowledge.rs` — `create_dir_all` + `write` + `remove_file` in endpoint
- `src/rag/engine.rs` — `read` + `read_dir` in ingest pipeline
- `src/tools/browser.rs` — `write` + `remove_file` per temp screenshots

`tokio::fs` è già usato in 23 file, mostrando consapevolezza — l'uso di sync è inconsistente.

---

## Piano di Esecuzione

Ordine consigliato per il refactor, dal più impattante al meno rischioso. Ogni item è indipendente.

### Priorità 1 — Quick wins alto impatto

| # | Task | File coinvolti | Stima | Rischio |
|---|------|---------------|-------|---------|
| R1 | **Estrarre `truncate_str` + `truncate_utf8` in `utils/text.rs`** | 12 file + 1 nuovo | ~200 righe cambiate | Basso — refactor meccanico, nessun cambio di comportamento |
| R2 | **Estrarre `WatcherHandle` in `utils/watcher.rs`** | 3 file + 1 nuovo | ~150 righe cambiate | Basso — struct identiche, test esistenti |
| R3 | **Aggiungere `Config::skills_dir()` etc. e sostituire `dirs::home_dir()`** | ~15 file | ~60 righe cambiate | Basso — refactor meccanico |

### Priorità 2 — Refactor strutturale

| # | Task | File coinvolti | Stima | Rischio |
|---|------|---------------|-------|---------|
| R4 | **Trait `SkillSource`** — unificare installer/search | 5 file skills/ | ~300 righe cambiate | Medio — richiede tipo unificato per risultati search |
| R5 | **Spostare `sqlx::query` nei file db dedicati** | 4 file → `storage/db.rs` | ~80 righe spostate | Basso |
| R6 | **Eliminare `#[allow(dead_code)]` — risolvere i 4 casi** | 4 file | ~20 righe | Basso |
| R7 | **Rinominare `mgr` → `manager`** | 2 file | ~30 righe | Basso |

### Priorità 3 — File split (top 10 per urgenza)

| # | File | Righe | Split proposto |
|---|------|-------|---------------|
| R8 | `web/api/providers.rs` | 1520 | 3 file |
| R9 | `web/auth.rs` | 1386 | 4 file (submodule `auth/`) |
| R10 | `web/api/channels.rs` | 1137 | 2 file |
| R11 | `web/api/mcp/install.rs` | 1024 | 2 file |
| R12 | `web/api/mcp/helpers.rs` | 1003 | 2 file |
| R13 | `tools/mcp.rs` | 984 | 2 file (submodule) |
| R14 | `tools/file.rs` | 983 | 2 file |
| R15 | `web/api/automations.rs` | 955 | 2 file |
| R16 | `provider/ollama.rs` | 934 | 2 file |
| R17 | `agent/browser_task_plan.rs` | 917 | 2 file |

### Priorità 4 — Convenzioni (alto effort, basso rischio)

| # | Task | File coinvolti | Stima | Note |
|---|------|---------------|-------|------|
| R18 | **Bool naming: `enabled` → `is_enabled`** | ~15 file config | ~200+ righe | Richiede `#[serde(alias)]` per backward compat con config.toml esistenti |
| R19 | **Bool naming: altri campi** | ~30 file | ~300+ righe | Può rompere API JSON — serve versioning |
| R20 | **Doc comments `///` su pub items** | ~80 file | ~500+ righe aggiunte | Puro lavoro additivo, zero rischio |
| R21 | **Module-level `//!` docs** | 17 `mod.rs` | ~50 righe aggiunte | Zero rischio |
| R22 | **Migrare `std::fs` → `tokio::fs` nei path async** | ~15 file hot | ~100 righe cambiate | Medio — potrebbe cambiare error handling |

---

## Refactor Completati — 2026-03-19

| # | Task | File | Risultato |
|---|------|------|-----------|
| **R1** | Truncate → `utils/text.rs` | 12 file | -150 LOC, 12 funzioni duplicate → 2 funzioni condivise, fix UTF-8 safety in `automations.rs` |
| **R2** | WatcherHandle → `utils/watcher.rs` | 5 file | -120 LOC, 3 struct identiche → 1 shared + helper `spawn_watched()` |
| **R3** | `Config::*_dir()` convenience methods | 9 file | -60 LOC, 22 call site sostituiti, 8 convenience methods aggiunti |
| **R5** | sqlx queries → Database methods | 4 file | 3 metodi aggiunti (`count_sessions`, `count_all_messages`, `list_memory_history`) |
| **R6** | Risolti 4 `#[allow(dead_code)]` | 5 file | 0 annotations rimaste. Slack/ClawHub: `_` prefix + serde rename. Verifier: variante rimossa + match semplificato. Gateway: campo rimosso |
| **R7** | `mgr` → `manager` | 2 file | 0 abbreviazioni non standard rimaste |

**Totale: 37 file toccati, -408/+237 righe (171 nette rimosse), 750 test passing, 0 warning.**

---

## Note Finali

- **I file grandfathered** (elencati nel CLAUDE.md) non sono inclusi nelle raccomandazioni di split.
- **Il bool naming** (R18-R19) è il refactor più ampio e rischioso — tocca serializzazione, API, e config utente. Va pianificato con migration strategy.
- **`cargo check` produce 0 warning** — il codebase è pulito dal punto di vista del compilatore.
