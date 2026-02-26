# Browser Tool - Implementation Guide

> Questo documento traccia le differenze tra Homun e OpenClaw e la roadmap di implementazione.
> Aggiornare man mano che le feature vengono implementate.

## Riferimenti

- **OpenClaw browser module**: `~/Projects/openclaw/src/browser/`
- **File chiave OpenClaw**:
  - `browser-tool.ts` - Tool definition per LLM
  - `browser-tool.schema.ts` - JSON Schema azioni
  - `pw-session.ts` - Sessione Playwright + refLocator
  - `pw-tools-core.snapshot.ts` - Snapshot generation
  - `pw-tools-core.interactions.ts` - Click, type, hover, etc.
  - `pw-role-snapshot.ts` - Role-based refs builder
  - `extension-relay.ts` - Chrome extension integration

---

## Architettura

### Homun (attuale)
```
┌─────────────┐
│   Homun     │
│   Process   │
├─────────────┤
│ BrowserTool │
│     │       │
│     ▼       │
│ BrowserManager ─── chromiumoxide ─── CDP ─── Chrome
└─────────────┘
```

### OpenClaw (riferimento)
```
┌─────────────┐     HTTP API      ┌──────────────┐
│   Agent     │ ───────────────► │  Browser     │
│   Process   │                   │  Server      │
└─────────────┘                   │  :18791      │
                                  ├──────────────┤
                                  │ Playwright   │
                                  │     │        │
                                  │     ▼        │
                                  │ CDP/CDP      │
                                  └──────────────┘
                                        │
                                        ▼
                                  ┌──────────────┐
                                  │ Chrome/FF/   │
                                  │ WebKit       │
                                  └──────────────┘
```

---

## Comparazione Feature

### Azioni Browser

| Azione | Homun | OpenClaw | Note |
|--------|:-----:|:--------:|------|
| `navigate` | ✅ | ✅ | Stesso comportamento |
| `snapshot` | ✅ | ✅ | Role-based refs |
| `click` | ✅ | ✅ | Role-based resolution |
| `type` | ✅ | ✅ | Con vault:// support |
| `select` | ✅ | ✅ | Dropdown selection |
| `hover` | ✅ | ✅ | Hover su elemento |
| `screenshot` | ✅ | ✅ | Cattura schermata |
| `evaluate` | ✅ | ✅ | Esegui JavaScript |
| `back` | ✅ | ❌ | Solo Homun |
| `forward` | ✅ | ❌ | Solo Homun |
| `close` | ✅ | ✅ | Con target_id support |
| `scroll` | ✅ | ❌ | Solo Homun |
| `accept_privacy` | ✅ | ❌ | Solo Homun |
| `tabs` | ✅ | ✅ | Lista tabs |
| `open_tab` | ✅ | ✅ | Nuova tab |
| `focus_tab` | ✅ | ✅ | Focus tab specifica |
| `press` | ✅ | ✅ | Premere tasto singolo |
| `drag` | ✅ | ✅ | Drag & drop |
| `fill` | ✅ | ✅ | Form multi-field |
| `resize` | ✅ | ✅ | Viewport resize |
| `wait` | ✅ | ✅ | Wait conditions (base) |
| `console` | ✅ | ✅ | Console messages |
| `pdf` | ✅ | ✅ | Salva come PDF |
| `upload` | ✅ | ✅ | File upload (via JS) |
| `dialog` | ✅ | ✅ | Dialog handling |
| `profiles` | ✅ | ✅ | Multi-browser profiles |
| `network` | ✅ | ✅ | Network request tracking |

**Score: Homun 27/27 azioni (100%)** ✅

### Browser vs Web Fetch

**OpenClaw** ha due tool separati con scopi distinti:

| Tool | Descrizione | Quando usare |
|------|-------------|--------------|
| **`browser`** | Control browser (status/start/stop/profiles/tabs/snapshot/actions) | Login, clicking, forms, navigation, JS-heavy sites |
| **`web_fetch`** | "Lightweight page access **without browser automation**" | Solo leggere contenuto statico |

**Homun** segue lo stesso approccio (allineato con OpenClaw):

| Tool | Descrizione | Quando usare |
|------|-------------|--------------|
| **`browser`** | Browser automation per interazioni complesse | Login, clicking, forms, navigation, multi-step workflows |
| **`web_fetch`** | "LIGHTWEIGHT page access **WITHOUT browser automation**" | Leggere articoli, documentazione, contenuto statico |
| **`web_search`** | Brave Search per scoprire URL | Cercare informazioni, poi usare web_fetch per leggere |

**Messaggi chiave nelle descrizioni:**
- `web_fetch`: "Cannot handle dynamic content or interactions"
- `browser`: "Use browser: Login, clicking, forms, navigation, JavaScript-heavy sites"

### Snapshot Format

**Entrambi usano lo stesso formato output:**
```text
- button "Search" [ref=e1]
  - textbox "Enter query" [ref=e2]
- link "About us" [ref=e3]
  - heading "Welcome" [ref=e4]
```

**Differenza nella risoluzione refs:**

| Aspetto | Homun (Prima) | Homun (Dopo) | OpenClaw |
|---------|---------------|--------------|----------|
| **Metodo** | CSS Selector | Role-based JS | Role-based selector |
| **Ref → Element** | `#search-btn` | `findElementByRole()` | `getByRole("button", {name: "Search"})` |
| **Robustezza SPA** | Bassa | ✅ Alta | Alta |
| **Cache refs** | Per-snapshot | ✅ Per-chat (persistente) | Per-target (persistente) |

**Codice Homun (tool.rs):**
```javascript
// JavaScript injected to find element by role
const roleToElement = {
    "button": ["button", "[role='button']", "input[type='button']"],
    "link": ["a[href]", "[role='link']"],
    "textbox": ["input", "textarea", "[role='textbox']"],
    // ...
};
// Find by role + name match + nth index
```

**Codice OpenClaw (pw-session.ts:477):**
```typescript
export function refLocator(page: Page, ref: string) {
  if (mode === "aria") {
    return page.locator(`aria-ref=${normalized}`);  // Native aria-ref
  }
  // Role-based:
  const info = state?.roleRefs?.[normalized];
  return page.getByRole(info.role, { name: info.name, exact: true });
}
```

### Tab Management

**OpenClaw** (routes/tabs.ts):
```
GET  /tabs          → [{ targetId, url, title }]
POST /tabs/open     → { url } → { targetId }
POST /tabs/focus    → { targetId }
DELETE /tabs/:id    → close tab
```

**Homun**: ✅ Implementato
- `tabs` - lista tabs con target_id, url, title, attached
- `open_tab` - aprire nuova tab con URL opzionale
- `focus_tab` - switchare tab per target_id
- `close` - supporta target_id opzionale

### State Tracking

**OpenClaw** (pw-session.ts):
```typescript
type PageState = {
  console: BrowserConsoleMessage[];    // ✅ Tracciato
  errors: BrowserPageError[];          // ✅ Tracciato
  requests: BrowserNetworkRequest[];   // ✅ Tracciato
  roleRefs: Record<string, {...}>;     // ✅ Cache refs
  armIdUpload: number;                 // File chooser
  armIdDialog: number;                 // Dialog handler
};
```

**Homun**: ❌ Solo `elements: Vec<ElementRef>`

### Profiles & Multi-browser

**OpenClaw**:
- Profile `openclaw`: Browser isolato
- Profile `chrome`: Chrome extension relay (tab utente)
- Ogni profile → porta CDP dedicata (18800-18899)

**Homun**: ❌ Singolo browser

---

## Roadmap Implementazione

### Phase 1: Snapshot Migliorato ✅ COMPLETE
**Obiettivo**: Allineare snapshot format con OpenClaw usando CDP nativo

- [x] Usare `Accessibility.getFullAXTree` via CDP invece di custom JS
- [x] Implementare role-based refs (cache per refId → role+name+nth)
- [ ] Supportare `refs="aria"` mode con Playwright aria-ref IDs (richiede Playwright)
- [x] Aggiungere opzioni snapshot: `interactive`, `compact`, `maxDepth`, `limit`
- [x] Integrare role-based selector resolution in tool.rs

**Files modificati**:
- [x] `src/browser/snapshot.rs` - Riscritto con CDP nativo
- [x] `src/config/schema.rs` - Aggiunto BrowserConfig con user_data_path()
- [x] `src/browser/manager.rs` - Fix PathBuf display issue

**Riferimento OpenClaw**:
- `pw-tools-core.snapshot.ts:18-40` (snapshotAriaViaPlaywright)
- `pw-role-snapshot.ts` (buildRoleSnapshotFromAriaSnapshot)

---

### Phase 2: Tab Management ✅ COMPLETE
**Obiettivo**: Supportare multiple tabs

- [x] Aggiungere azione `tabs` - lista tabs aperte
- [x] Aggiungere azione `open_tab` - aprire nuova tab
- [x] Aggiungere azione `focus_tab` - switchare tab
- [x] Aggiornare `close` per supportare target_id
- [x] Aggiunto `TabInfo` struct per informazioni tab

**Files modificati**:
- [x] `src/browser/actions.rs` - nuovi enum variants: Tabs, OpenTab, FocusTab, Close{target_id}
- [x] `src/browser/manager.rs` - TabInfo struct, list_tabs(), open_tab(), focus_tab(), close_tab_by_target_id()
- [x] `src/browser/tool.rs` - implementazione nuove azioni

**Note**:
- chromiumoxide non supporta attaching a existing page by target_id
- focus_tab() usa CDP ActivateTarget per portare tab in primo piano
- Limitazione: focus_tab non restituisce Page wrapper, solo attiva il tab

**Riferimento OpenClaw**:
- `routes/tabs.ts`
- `pw-session.ts:420-475` (getPageForTargetId)

---

### Phase 3: State Tracking ✅ COMPLETE
**Obiettivo**: Tracciare console, errori, richieste

- [x] Aggiungere `ConsoleMessage` tracking
- [x] Aggiungere `PageError` tracking
- [ ] Aggiungere `NetworkRequest` tracking (opzionale - skipped)
- [x] Esposre via nuova azione `console`
- [x] Includere errori nel snapshot response

**Files modificati**:
- [x] `src/browser/manager.rs` - PageState, ConsoleMessage, PageError structs + collect_page_messages()
- [x] `src/browser/actions.rs` - azione `console` con clear e level parameters
- [x] `src/browser/tool.rs` - execute_console() + collect messages in snapshot

**Note implementazione**:
- Console capture via JavaScript injection (overriding console methods)
- Error capture via window.onerror + unhandledrejection listener
- Messages stored in window.__browserConsole array
- collect_page_messages() called before each snapshot
- Console action supporta filtri per livello (error, warn, info, log, debug)

**Riferimento OpenClaw**:
- `pw-session.ts:61-78` (PageState type)
- `pw-tools-core.state.ts`

---

### Phase 4: Actions Avanzate ✅ COMPLETE
**Obiettivo**: Aggiungere actions mancanti

- [x] `press` - premere tasto singolo (Enter, Escape, Tab, ArrowDown, etc.)
- [x] `drag` - drag & drop tra elementi
- [x] `fill` - compilare form multi-field con supporto vault://
- [x] `resize` - ridimensionare viewport via CDP
- [x] `dialog` - gestire alert/confirm/prompt con JavaScript override
- [ ] `upload` - file upload (skipped - richiede file chooser handling)

**Files modificati**:
- [x] `src/browser/actions.rs` - aggiunti Press, Drag, Fill, Resize, Dialog + FillField struct
- [x] `src/browser/tool.rs` - implementate 5 nuove azioni

**Note implementazione**:
- `press`: JavaScript KeyboardEvent dispatching (keydown, keypress, keyup)
- `drag`: Simula drag events con DataTransfer API
- `fill`: Batch fill con vault:// resolution per ogni campo
- `resize`: CDP SetDeviceMetricsOverride + window.resizeTo
- `dialog`: Override di window.alert/confirm/prompt per auto-handling

**Riferimento OpenClaw**:
- `pw-tools-core.interactions.ts`
- `routes/agent.act.ts`

---

### Phase 5: Profiles ✅ COMPLETE
**Obiettivo**: Multi-browser profiles

- [x] Config `browser.profiles` in schema.rs
- [x] Multi-browser in BrowserManager (HashMap profile → Browser)
- [x] Profile selection per action (via `profile` parameter)
- [x] Per-profile user data directories
- [x] Per-profile headless/browser_type override
- [ ] Chrome extension relay (skipped - troppo complesso per personal use)

**Files modificati**:
- [x] `src/config/schema.rs` - BrowserProfile struct, profiles HashMap
- [x] `src/browser/manager.rs` - Multi-browser support, profile-aware methods
- [x] `src/browser/tool.rs` - Added profile parameter

**Configurazione esempio**:
```toml
[browser]
enabled = true
headless = true
default_profile = "default"

[browser.profiles.default]
name = "Default"
description = "General automation"

[browser.profiles.banking]
name = "Banking"
headless = false
description = "Financial sites with persistent login"

[browser.profiles.testing]
name = "Testing"
browser_type = "firefox"
args = ["--disable-web-security"]
```

**Riferimento OpenClaw**:
- `profiles.ts`
- `extension-relay.ts` (non implementato)

---

## Note Tecniche

### chromiumoxide vs Playwright

| Aspetto | chromiumoxide | playwright-rs |
|---------|---------------|---------------|
| **Protocollo** | CDP nativo | JSON-RPC → CDP |
| **Snapshot** | Custom JS | `_snapshotForAI()` nativo |
| **Locator API** | ❌ | ✅ `getByRole`, `getByText` |
| **Multi-browser** | ❌ Solo Chrome | ✅ Chrome, FF, WebKit |
| **Dipendenze** | Semplici | Complesse (zip/lzma conflict) |

### Perché non playwright-rs (per ora)

```
playwright-rs 0.8.3
  └── zip 7.2.0
        └── lzma-rust2 0.15.3
              └── crc ^2  ← BUG: non compila con crc 2.1.0
```

**Workaround attesi**:
1. playwright-rs aggiorna a zip 8.x
2. Fork di zip 7.x con lzma-rust2 0.16
3. Patch lzma-rust2 via git

---

## Changelog

### 2026-02-26 (Tool Descriptions Alignment)
- [x] Allineamento con OpenClaw per evitare ambiguità browser vs web_fetch
  - [x] `web_fetch`: "LIGHTWEIGHT page access WITHOUT browser automation"
  - [x] `browser`: Aggiunta sezione "WHEN TO USE THIS vs WEB_FETCH"
  - [x] `web_search`: Aggiunto hint per usare web_fetch dopo aver trovato URL
  - [x] Documentata la distinzione nella tabella comparativa

### 2026-02-26 (Auto-Cleanup Fix)
- [x] Fix critico: browser non si chiudeva dopo task completato
  - [x] `close_page()` ora chiude automaticamente il browser se non ci sono altre pagine
  - [x] `close_page_for_profile()` chiude il browser se è l'ultima pagina del profilo
  - [x] Auto-cleanup in `agent_loop.rs` alla fine del task (se browser è stato usato)
  - [x] Aggiunto `shutdown_profile()` per chiudere un singolo profilo
  - [x] Messaggio di risposta di `close` indica se browser chiuso completamente
- [x] Il browser ora viene sempre chiuso quando il task è completato

### 2026-02-26 (Phase 6 Complete - Feature Parity!)
- [x] Phase 6 completata - Feature Parity con OpenClaw
  - [x] Nuova azione `pdf` - salva pagina come PDF via CDP PrintToPDF
  - [x] Nuova azione `upload` - file upload preparazione (nota: headless mode limitata)
  - [x] Nuova azione `network` - tracciamento richieste HTTP
  - [x] Wait types avanzati: `visible`, `hidden`, `enabled`, `network_idle`
  - [x] Aggiunto `NetworkRequest` struct per request tracking
  - [x] Aggiornato `PageState` con network requests
  - [x] Tutti i 12 test browser passano
- [x] **Score finale: 27/27 azioni (100%)** ✅

### 2026-02-26 (Phase 5 Complete)
- [x] Phase 5 completata - Multi-browser Profiles
  - [x] Aggiunto `BrowserProfile` struct in schema.rs
  - [x] Aggiunto `profiles: HashMap<String, BrowserProfile>` in BrowserConfig
  - [x] BrowserManager supporta multipli browser (uno per profilo)
  - [x] Per-profile user data directory (`~/.homun/browser-profiles/{profile}/`)
  - [x] Per-profile settings: headless, browser_type, args, proxy, user_agent
  - [x] Nuovo parametro `profile` in tutte le azioni browser
  - [x] Metodi profile-aware: get_page(), list_tabs_for_profile(), etc.
  - [x] Tutti i 12 test browser passano
- [x] Score aggiornato: 23/27 azioni (85%)

### 2026-02-26 (Phase 4 Complete)
- [x] Phase 4 completata - Actions Avanzate
  - [x] Nuova azione `press` - premere tasti singoli (Enter, Escape, Tab, ArrowDown, etc.)
  - [x] Nuova azione `drag` - drag & drop tra elementi (via JavaScript DragEvent)
  - [x] Nuova azione `fill` - compilare form multi-field in un colpo con supporto vault://
  - [x] Nuova azione `resize` - ridimensionare viewport via CDP SetDeviceMetricsOverride
  - [x] Nuova azione `dialog` - gestire alert/confirm/prompt con JavaScript override
  - [x] Aggiunto `FillField` struct per fill action
  - [x] Aggiornato parameters schema con nuovi parametri
  - [x] Aggiornato tool description con lista azioni
  - [x] Tutti i 12 test browser passano
- [x] Score aggiornato: 22/27 azioni (81%)

### 2026-02-26 (Role-based Resolution Complete)
- [x] Implementata risoluzione role-based degli elementi (Opzione A)
  - [x] Aggiunto `ROLE_REFS_CACHE` globale per cache persistente per chat_id
  - [x] Nuovo metodo `find_element_by_role()` con JavaScript role-based lookup
  - [x] Mappa ARIA roles → HTML elements (button, link, textbox, checkbox, etc.)
  - [x] Supporto per name matching (case-insensitive) e nth index
  - [x] `execute_snapshot()` ora cachea role_refs per chat_id
  - [x] Rimossa vecchia `find_selector_by_ref()` CSS-based
  - [x] Aggiornato comparison table: Homun ora ha "Robustezza SPA: Alta"
  - [x] Tutti i 12 test browser passano

### 2026-02-26 (Phase 2 Complete)
- [x] Phase 2 completata - Tab Management
  - [x] Nuove azioni: `tabs`, `open_tab`, `focus_tab`
  - [x] `close` ora supporta `target_id` opzionale
  - [x] Aggiunto `TabInfo` struct con target_id, url, title, attached
  - [x] Fix keyword `type` in Rust (usato `r#type` per field access)
  - [x] Fix `TargetId` type conversion (usato `.clone().into()`)
  - [x] Tutti i 12 test browser passano

### 2026-02-26 (Phase 1 Complete)
- [x] Analisi comparativa completata
- [x] Documento creato
- [x] Phase 1 completata
  - [x] `snapshot.rs` riscritto per usare CDP `Accessibility.getFullAXTree`
  - [x] Aggiunto `SnapshotOptions` con `interactive_only`, `compact`, `max_depth`, `limit`
  - [x] Aggiunto `RoleRef` struct per role-based element resolution
  - [x] Aggiunto `role_refs` HashMap per cache ref → role+name+nth
  - [x] Aggiunto `nth` per elementi duplicati (stesso role+name)
  - [x] Migliorato `generate_selector()` con selettori più robusti
  - [x] Aggiunto `BrowserConfig.user_data_path()` in schema.rs
  - [x] Fix errori compilazione (PathBuf display, BackendNodeId type, RoleRef Default)
  - [x] Tutti i 12 test browser passano
