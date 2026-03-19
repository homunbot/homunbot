# Homun Onboarding Experience — ONB-4

> **Status**: SPEC — da approvare prima dell'esecuzione
> **Assorbe**: ONB-1 (setup wizard v2), ONB-2 (flusso Ollama), AUD-2 (feature gating)
> **Effort stimato**: 2 settimane
> **Design system**: Editorial Canvas (REDESIGN-SPEC.md)

---

## 1. Obiettivi

### Problema attuale
Il wizard esistente (4 step inline nella pagina Settings) funziona ma:
- Non raccoglie informazioni sull'utente (lingua, nome, timezone)
- Non guida la configurazione dei canali (Telegram, Discord, ecc.)
- Non spiega le funzionalita disponibili (feature gating / AUD-2)
- UX frammentata: setup wizard + settings nella stessa pagina
- Non supporta multi-lingua

### Obiettivo
Un'esperienza first-run completa, dedicata, che porta l'utente da zero a operativo in 5 fasi. Deve essere:
- **Autosufficiente**: nessuna conoscenza pregressa richiesta
- **Progressiva**: ogni fase sblocca la successiva, ma si puo saltare
- **Bilingue**: EN + IT, con detection automatica
- **Rientrabile**: l'utente puo tornare in qualsiasi momento da Settings

---

## 2. Personas & Flussi

### Persona A — Cloud User (80% stimato)
> Vuole usare Claude/GPT via API. Ha una API key pronta.

Flusso tipico: Welcome → Inserisce API key Anthropic → Sceglie claude-sonnet → Skip canali → Test → Chat

**Tempo stimato**: 2-3 minuti

### Persona B — Local-First User (15%)
> Privacy-first, vuole Ollama locale. Nessun servizio cloud.

Flusso tipico: Welcome → Ollama auto-detect → Pull llama3.2 → Skip canali → Test → Chat

**Tempo stimato**: 3-5 minuti (include download modello)

### Persona C — Power User (5%)
> Configura tutto: provider multipli, Telegram + Discord, timezone.

Flusso tipico: Welcome → Provider primario + secondario → Canale Telegram + Discord → Test → Chat

**Tempo stimato**: 8-10 minuti

---

## 3. Layout

### Desktop (>768px)

```
┌──────┬──────────────────────────────────────────────────┐
│      │                                                  │
│ Nav  │  ┌─────────────┬──────────────────────────────┐  │
│ 80px │  │ Side Panel  │  Content Area                │  │
│      │  │ 280px       │                              │  │
│      │  │             │  ┌────────────────────────┐  │  │
│      │  │ ○ Welcome   │  │                        │  │  │
│      │  │ ● Provider  │  │  Fase corrente         │  │  │
│      │  │ ○ Model     │  │  (form / cards / test) │  │  │
│      │  │ ○ Channels  │  │                        │  │  │
│      │  │ ○ Ready!    │  │                        │  │  │
│      │  │             │  └────────────────────────┘  │  │
│      │  │             │                              │  │
│      │  │ ─ ─ ─ ─ ─  │  [Back]           [Continue] │  │
│      │  │ Skip setup →│                              │  │
│      │  └─────────────┴──────────────────────────────┘  │
│      │                                                  │
└──────┴──────────────────────────────────────────────────┘
```

### Side Panel
- Sfondo `--surface`, border-right dashed `--border`
- Lista fasi con indicatori di stato:
  - `○` Pending (grigio `--t3`)
  - `●` Corrente (accent `--accent`, dot pieno)
  - `✓` Completata (verde `--ok`)
  - `—` Skippata (grigio `--t4`, testo barrato)
- Ogni fase mostra: titolo + sottotitolo breve (1 riga)
- In fondo: link "Skip setup — I'll configure later →"

### Mobile (<768px)
- Side panel collassa in **progress bar orizzontale** in alto (5 dots)
- Content area full-width con padding 16px
- Navigazione: swipe o bottoni Back/Continue
- Step indicator: `2 of 5 — Provider` sotto la progress bar

---

## 4. Le 5 Fasi

### Fase 1: Welcome

**Scopo**: Raccogliere le informazioni base sull'utente e stabilire il tono.

```
┌─────────────────────────────────────────┐
│                                         │
│  ☀  Welcome to Homun                    │
│                                         │
│  Your personal AI assistant.            │
│  Let's set things up in a few steps.    │
│                                         │
│  ┌───────────────────────────────────┐  │
│  │ Your name                         │  │
│  │ ┌─────────────────────────────┐   │  │
│  │ │ Fabio                       │   │  │
│  │ └─────────────────────────────┘   │  │
│  │                                   │  │
│  │ Language           Timezone       │  │
│  │ ┌──────────┐  ┌────────────────┐ │  │
│  │ │ English ▼│  │ Europe/Rome  ▼ │ │  │
│  │ └──────────┘  └────────────────┘ │  │
│  └───────────────────────────────────┘  │
│                                         │
│                         [Get started →] │
└─────────────────────────────────────────┘
```

**Campi:**
| Campo | Tipo | Default | Persistenza |
|-------|------|---------|-------------|
| Name | text input | vuoto (opzionale) | `config.toml` → `[agent] user_name` |
| Language | select | detect da `Accept-Language` | `config.toml` → `[agent] language` |
| Timezone | select | detect da `Intl.DateTimeFormat().resolvedOptions().timeZone` | `config.toml` → `[agent] timezone` |

**Lingue disponibili**: English, Italiano (estendibile).

**Logica Language**:
- Detect browser `navigator.language`
- Se `it*` → default Italiano, altrimenti English
- Cambiare lingua qui aggiorna immediatamente tutta la UI dell'onboarding

**Logica Timezone**:
- Auto-detect via `Intl.DateTimeFormat`
- Dropdown con le timezone IANA principali, raggruppate per area geografica
- Importante per: cron jobs, automazioni, timestamps nei log

**API**: `PATCH /api/v1/config` con chiavi `agent.user_name`, `agent.language`, `agent.timezone`

---

### Fase 2: Provider

**Scopo**: Configurare almeno un provider LLM (cloud o locale).

**Layout a due colonne** (desktop):

```
┌─────────────────────────────────────────┐
│                                         │
│  Connect your AI                        │
│  Choose how Homun talks to language     │
│  models.                                │
│                                         │
│  ┌─ Cloud Providers ─────────────────┐  │
│  │                                   │  │
│  │  [Anthropic]  [OpenAI]  [Gemini]  │  │
│  │  [OpenRouter] [DeepSeek] [+more]  │  │
│  │                                   │  │
│  └───────────────────────────────────┘  │
│                                         │
│  ┌─ Local AI ────────────────────────┐  │
│  │                                   │  │
│  │  🟢 Ollama detected (localhost)   │  │
│  │  3 models available               │  │
│  │                        [Use this] │  │
│  │                                   │  │
│  └───────────────────────────────────┘  │
│                                         │
│  [← Back]                 [Continue →]  │
└─────────────────────────────────────────┘
```

**Cloud Provider Card** (on click/expand):
```
┌─ Anthropic ──────────────────────────┐
│                                      │
│  API Key                             │
│  ┌──────────────────────────┐        │
│  │ sk-ant-api03-...         │ [👁]   │
│  └──────────────────────────┘        │
│                                      │
│  [Test connection]   ✓ Connected     │
│                                      │
└──────────────────────────────────────┘
```

**Ollama Auto-Detect** (riutilizza logica ONB-2):
- Probe `GET http://localhost:11434/api/tags` al mount della fase
- Se risponde: mostra box verde con lista modelli gia installati
- Se nessun modello installato: suggerisci pull di `llama3.2:3b` o `gemma3:4b`
- Se Ollama non risponde: mostra box grigio "Ollama not detected" con link alla guida di installazione
- Pull progress: barra di avanzamento con percentuale

**Validazione**:
- Almeno 1 provider configurato per procedere (o skip)
- Test connection obbligatorio prima di segnare come "configured"
- API key non vuota, formato validato (prefix check per Anthropic/OpenAI)

**Skip**: Permesso ma con warning "You won't be able to chat without a provider."

---

### Fase 3: Model

**Scopo**: Selezionare il modello attivo e (opzionalmente) il modello di fallback.

```
┌─────────────────────────────────────────┐
│                                         │
│  Choose your model                      │
│  This is the AI model Homun will use    │
│  for conversations.                     │
│                                         │
│  ┌─ Recommended ─────────────────────┐  │
│  │                                   │  │
│  │  ● claude-sonnet-4-5              │  │
│  │    Fast, capable, great default   │  │
│  │                                   │  │
│  │  ○ claude-opus-4                  │  │
│  │    Most capable, slower           │  │
│  │                                   │  │
│  │  ○ gpt-4o                         │  │
│  │    OpenAI flagship                │  │
│  │                                   │  │
│  └───────────────────────────────────┘  │
│                                         │
│  ▸ All available models (12)            │
│                                         │
│  ┌─ Fallback (optional) ─────────────┐  │
│  │ If the primary model fails:       │  │
│  │ ┌─────────────────────────────┐   │  │
│  │ │ None ▼                      │   │  │
│  │ └─────────────────────────────┘   │  │
│  └───────────────────────────────────┘  │
│                                         │
│  [← Back]                 [Continue →]  │
└─────────────────────────────────────────┘
```

**Logica**:
- Mostra solo modelli dei provider configurati nella fase precedente
- Sezione "Recommended": top 3 modelli ordinati per capability
- "All available models": lista espandibile con tutti i modelli
- Radio button selection per modello primario
- Dropdown opzionale per fallback
- Se un solo provider con un solo modello: auto-seleziona e mostra conferma

**Modelli suggeriti per provider**:
| Provider | Suggerito | Nota |
|----------|-----------|------|
| Anthropic | claude-sonnet-4-5 | Miglior rapporto velocita/qualita |
| OpenAI | gpt-4o | Flagship |
| Ollama | llama3.2:3b | Piu leggero per locale |
| OpenRouter | (varia) | Mostra i piu popolari |

**API**: `PATCH /api/v1/config` con `agent.model` e opzionalmente `agent.fallback_model`

---

### Fase 4: Channels

**Scopo**: Connettere canali di messaggistica (opzionale).

```
┌─────────────────────────────────────────┐
│                                         │
│  Connect your channels                  │
│  Homun can reach you on multiple        │
│  platforms. Set up now or add later.     │
│                                         │
│  ┌──────────┐ ┌──────────┐             │
│  │ Telegram  │ │ Discord  │             │
│  │           │ │          │             │
│  │  [Setup]  │ │  [Setup] │             │
│  └──────────┘ └──────────┘             │
│  ┌──────────┐ ┌──────────┐             │
│  │ WhatsApp  │ │ Slack    │             │
│  │           │ │          │             │
│  │  [Setup]  │ │  [Setup] │             │
│  └──────────┘ └──────────┘             │
│  ┌──────────┐ ┌──────────┐             │
│  │ Email     │ │ Web UI   │             │
│  │           │ │ ✓ Always │             │
│  │  [Setup]  │ │   on     │             │
│  └──────────┘ └──────────┘             │
│                                         │
│  [← Back]             [Continue →]      │
│                                         │
│  ℹ  You can always add channels later   │
│     from Settings → Channels            │
└─────────────────────────────────────────┘
```

**Channel Card** (on click → espande inline):

```
┌─ Telegram ───────────────────────────┐
│                                      │
│  1. Talk to @BotFather on Telegram   │
│  2. Create a new bot                 │
│  3. Paste the token below            │
│                                      │
│  Bot Token                           │
│  ┌──────────────────────────┐        │
│  │ 123456:ABC-...           │        │
│  └──────────────────────────┘        │
│                                      │
│  Allowed User IDs (optional)         │
│  ┌──────────────────────────┐        │
│  │ 12345678                 │        │
│  └──────────────────────────┘        │
│  ↳ Your Telegram user ID.           │
│    Leave empty to allow anyone.      │
│                                      │
│  [Test & Save]        ✓ Connected    │
│                                      │
└──────────────────────────────────────┘
```

**Istruzioni per canale**:

| Canale | Campi | Guida inline |
|--------|-------|--------------|
| **Telegram** | `token`, `allow_from[]` | Link a @BotFather, step 1-2-3 |
| **Discord** | `token`, `default_channel_id` | Link a Discord Developer Portal |
| **WhatsApp** | (web pairing) | Mostra QR code via WebSocket, istruzioni scannerizza |
| **Slack** | `token`, `channel_id` | Link a Slack App creation, Socket Mode |
| **Email** | `imap_host/port`, `smtp_host/port`, `username`, `password`, `from_address` | Preset per Gmail/Outlook, form manuale per altri |

**WhatsApp speciale**: usa il flusso di web pairing gia implementato (`/api/v1/channels/whatsapp/pair`). Mostra il QR code direttamente nell'onboarding.

**Email presets**:
```
┌─ Quick setup ────────────────────────┐
│                                      │
│  [Gmail]  [Outlook]  [Custom IMAP]   │
│                                      │
└──────────────────────────────────────┘
```
Gmail/Outlook: pre-compila host/port, chiede solo username + app password.

**Fase completamente opzionale**: nessun canale richiesto. Web UI e sempre attiva.

---

### Fase 5: Ready!

**Scopo**: Test del provider, primo messaggio, celebrazione.

```
┌─────────────────────────────────────────┐
│                                         │
│  🎉  You're all set!                    │
│                                         │
│  ┌─ System check ────────────────────┐  │
│  │                                   │  │
│  │  ✓ Provider connected             │  │
│  │  ✓ Model: claude-sonnet-4-5       │  │
│  │  ✓ Channels: Web UI, Telegram     │  │
│  │  ✓ Language: English              │  │
│  │  ✓ Timezone: Europe/Rome          │  │
│  │                                   │  │
│  └───────────────────────────────────┘  │
│                                         │
│  ┌─ What you can do ─────────────────┐  │
│  │                                   │  │
│  │  💬 Chat — ask anything           │  │
│  │  🔧 Skills — extend with plugins  │  │
│  │  📋 Automations — schedule tasks  │  │
│  │  🧠 Knowledge — teach documents   │  │
│  │  🔐 Vault — store secrets safely  │  │
│  │                                   │  │
│  └───────────────────────────────────┘  │
│                                         │
│       [Open Chat →]                     │
│                                         │
└─────────────────────────────────────────┘
```

**System check**: esegue automaticamente:
1. `GET /api/v1/providers/test` — verifica connessione provider attivo
2. Verifica model impostato
3. Lista canali configurati
4. Mostra riassunto configurazione

Se il test fallisce: mostra errore inline con bottone "Retry" e link per tornare alla fase 2.

**Feature gating (AUD-2)**: la sezione "What you can do" serve come introduzione alle funzionalita. Non e un tutorial completo, ma orienta l'utente su cosa puo esplorare dopo. Ogni voce e cliccabile e porta alla pagina relativa.

**"Open Chat →"**: redirect a `/chat`. Segna l'onboarding come completato nel DB.

---

## 5. Persistenza & Checkpoint

### localStorage (client-side)
```js
{
  "homun-onboarding": {
    "version": 1,
    "currentPhase": 2,          // 1-5
    "completed": [1],            // fasi completate
    "skipped": [],               // fasi saltate
    "startedAt": "2026-03-18T10:00:00Z",
    "expiresAt": "2026-03-19T10:00:00Z"  // 24h
  }
}
```

### Server-side
- **Config**: scritto via `PATCH /api/v1/config` a ogni fase (persistente)
- **Onboarding status**: nuovo campo `config.toml` → `[web] onboarding_completed = true`
- **Secrets**: API keys salvate nel vault (AES-256-GCM), mai in config

### Rientro
- Se `onboarding_completed = false` e nessun provider configurato → redirect automatico a `/onboarding` dopo login
- Bottone "Run setup again" in Settings per rientrare
- Il checkpoint localStorage preserva il progresso per 24h (sopravvive a refresh/chiusura tab)

---

## 6. Multi-lingua (i18n)

### Approccio
File JSON per lingua, caricato client-side:

```
static/i18n/
├── en.json
└── it.json
```

Struttura:
```json
{
  "onboarding.welcome.title": "Welcome to Homun",
  "onboarding.welcome.subtitle": "Your personal AI assistant.",
  "onboarding.welcome.cta": "Get started",
  "onboarding.provider.title": "Connect your AI",
  "onboarding.provider.subtitle": "Choose how Homun talks to language models.",
  "onboarding.provider.ollama_detected": "Ollama detected (localhost)",
  ...
}
```

### Funzionamento
1. Al mount di `/onboarding`, controlla: localStorage `homun-lang` → browser `Accept-Language` → default `en`
2. Carica il file JSON corrispondente
3. Funzione `t(key)` per i template string
4. Se l'utente cambia lingua nella Fase 1, ricarica le stringhe e aggiorna la UI

### Scope
Solo l'onboarding e bilingue in questa fase. Il resto dell'app resta in inglese.
L'infrastruttura i18n (file JSON + funzione `t()`) sara riusabile per future localizzazioni.

---

## 7. Responsive Design

### Breakpoints

| Viewport | Layout | Side Panel | Navigation |
|----------|--------|------------|------------|
| **≥1024px** | 2 colonne (panel + content) | 280px fisso | Step list verticale |
| **768-1023px** | 2 colonne (panel + content) | 240px comprimibile | Step list verticale |
| **<768px** | 1 colonna (full width) | Collassa in progress bar | 5 dots + label orizzontale |

### Mobile (<768px)
- Progress bar: 5 dots orizzontali, dot corrente accent, completati verdi
- Sotto i dots: `Step 2 of 5 — Provider`
- Content: padding 16px laterale
- Cards: full width, stack verticale
- Bottoni Back/Continue: fixed in basso, full width, 48px altezza (touch target)
- Channel cards: 1 per riga

### Tablet (768-1023px)
- Side panel 240px, collapsible con hamburger
- Channel cards: 2 per riga

### Desktop (≥1024px)
- Side panel 280px fisso
- Channel cards: 3 per riga
- Content area max-width 720px, centrato

---

## 8. Stati dei Componenti

Ogni componente dell'onboarding deve gestire tutti gli stati:

### Provider Card
| Stato | Visualizzazione |
|-------|----------------|
| Default | Logo + nome, bordo `--border`, hover lift |
| Selected | Bordo `--accent`, sfondo `--accent-light` |
| Configuring | Espansa con form campi |
| Testing | Spinner + "Testing connection..." |
| Connected | Badge verde "✓ Connected" |
| Error | Bordo `--err`, messaggio errore inline |

### Phase Step (side panel)
| Stato | Dot | Testo | Sottotitolo |
|-------|-----|-------|-------------|
| Pending | `○` grigio | `--t2` | Nascosto |
| Current | `●` accent | `--t1` bold | Visibile |
| Completed | `✓` verde | `--t1` | Riassunto (es. "Anthropic") |
| Skipped | `—` grigio | `--t3` barrato | "Skipped" |

### Form Input
| Stato | Stile |
|-------|-------|
| Default | Border `--border`, bg `--surface` |
| Focus | Ring `--accent`, border `--accent-border` |
| Valid | Icona ✓ verde a destra |
| Invalid | Border `--err`, hint rosso sotto |
| Disabled | Opacity 0.5, cursor not-allowed |

---

## 9. Animazioni & Transizioni

- **Cambio fase**: content area fade-out 150ms → fade-in 200ms (CSS `opacity` + `transform: translateX`)
- **Espansione card**: `max-height` transition 250ms ease-out
- **Progress dot**: scale 1 → 1.2 bounce su fase corrente
- **System check (Fase 5)**: item appaiono uno per uno con delay 200ms stagger
- **Nessuna animazione pesante**: rispettare `prefers-reduced-motion`

---

## 10. API Endpoints Necessari

### Esistenti (riusati)
| Endpoint | Uso |
|----------|-----|
| `PATCH /api/v1/config` | Salvataggio ogni campo |
| `GET /api/v1/config` | Lettura configurazione corrente |
| `GET /api/v1/providers` | Lista provider disponibili |
| `POST /api/v1/providers/{name}/test` | Test connessione |
| `GET /api/v1/models` | Lista modelli per provider configurati |
| `GET /api/v1/channels` | Lista canali e stato |
| `WS /api/v1/channels/whatsapp/pair` | Web pairing WhatsApp |

### Nuovi
| Endpoint | Metodo | Scopo |
|----------|--------|-------|
| `GET /api/v1/onboarding/status` | GET | Stato corrente onboarding (completed, currentPhase) |
| `POST /api/v1/onboarding/complete` | POST | Segna onboarding completato |
| `GET /api/v1/ollama/detect` | GET | Probe Ollama + lista modelli (proxy per evitare CORS) |
| `POST /api/v1/ollama/pull` | POST | Pull modello con SSE progress |

**Nota**: `ollama/detect` e `ollama/pull` probabilmente esistono gia nel flusso ONB-2. Verificare e riusare.

---

## 11. Struttura File

### Nuovi file
```
static/js/onboarding.js        # Orchestrazione 5 fasi (~400 righe)
static/i18n/en.json             # Stringhe inglese
static/i18n/it.json             # Stringhe italiano
src/web/api/onboarding.rs       # Endpoint status + complete (~80 righe)
```

### File modificati
```
src/web/pages.rs                # Aggiunta fn onboarding_page()
src/web/server.rs               # Route /onboarding + redirect logic
src/web/api/mod.rs              # Registrazione rotte onboarding
src/config/schema.rs            # Campi: agent.user_name, agent.language, agent.timezone, web.onboarding_completed
static/css/style.css            # Classi onboarding (~150 righe)
src/web/auth.rs                 # Redirect a /onboarding invece di /setup-wizard per first-run
```

### File NON toccati
- `setup.js` — resta per la pagina Settings (configurazione avanzata post-onboarding)
- Il wizard inline nella pagina Setup resta come "advanced settings"

---

## 12. Migration Plan

### Da setup-wizard a onboarding
1. **First-run redirect**: cambiare `auth.rs` riga 337 da `/setup-wizard` a `/onboarding`
2. **Setup page**: rimuovere la sezione wizard, lasciare solo configurazione avanzata
3. **Re-entry**: aggiungere bottone "Run setup again" in Settings che porta a `/onboarding`
4. **Utenti esistenti**: se `onboarding_completed` non esiste ma hanno provider configurati → segnare come completato (migrazione automatica)

### Migrazione DB
```sql
-- Nessuna migrazione SQL necessaria.
-- Lo stato onboarding e in config.toml (web.onboarding_completed).
-- I campi utente sono in config.toml (agent.user_name, agent.language, agent.timezone).
```

---

## 13. Accessibilita

- **Contrasto**: WCAG AA minimo (4.5:1 testo, 3:1 UI)
- **Focus**: visibile su tutti gli elementi interattivi (ring accent)
- **Keyboard**: Tab attraverso i campi, Enter per confermare, Escape per chiudere espansioni
- **Screen reader**: `aria-current="step"` su fase corrente, `aria-label` su tutti i bottoni icona
- **Touch target**: minimo 44x44px su mobile
- **Reduced motion**: `@media (prefers-reduced-motion: reduce)` disabilita tutte le animazioni

---

## 14. Metriche di Successo

| Metrica | Target |
|---------|--------|
| Completion rate (fase 5 raggiunta) | >80% |
| Tempo medio completamento | <5 min (Persona A) |
| Drop-off per fase | <10% per fase |
| Re-entry rate | <5% (onboarding chiaro al primo tentativo) |

**Tracking**: log lato server al completamento di ogni fase (tracing event, non analytics esterno).

---

## 15. Fuori Scope (v1)

- Tutorial interattivo post-onboarding (futuro)
- Onboarding MCP servers (vedi `mcp-onboarding-semplificato.md` — fase separata)
- Import configurazione da file/backup
- Onboarding multi-utente (Homun e single-user)
- Lingue oltre EN/IT
