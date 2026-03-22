# SPEC: Profile System (Profilo-First Architecture)

> Status: DESIGN — non implementare prima dell'approvazione
> Data: 2026-03-22
> Revisione: 2 — aggiunto modello user_id + profile_id + DB esterni

## Problema

Oggi Homun ha un'identità singleton: un unico SOUL.md, USER.md, INSTRUCTIONS.md globali. Le persona sono 4 stringhe hardcodate (bot/owner/company/custom) con un match in `persona.rs`. Non c'è modo di:

- Accumulare conoscenze diverse per contesti diversi (personale vs aziendale)
- Avere skill specifiche per contesto
- Filtrare documenti RAG per contesto
- Gestire identità strutturate (stile AIEOS)
- Supportare scenari multi-utente (enterprise)

## Evoluzione a 3 livelli

```
v1 (oggi):     1 utente, 1 identità
v2 (profili):  1 utente, N profili — ★ IMPLEMENTAZIONE ATTUALE
v3 (futuro):   N utenti, N profili, admin, permessi RBAC
```

v2 introduce `user_id` + `profile_id` ovunque. In v2 esiste un solo utente (`user_id = 1`, admin, non visibile nella UI). In v3 si aggiunge la tabella `users` e la gestione multi-utente con cambiamenti minimi perché le FK sono già in posizione.

## Modello: User + Profile

### Tabella users (v2: un solo record, v3: multi-utente)

```sql
CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'admin',  -- v2: sempre 'admin'; v3: 'admin'|'user'|'viewer'
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- v2: seed unico utente admin (invisibile nella UI)
INSERT INTO users (username, display_name, role) VALUES ('admin', 'Admin', 'admin');
```

### Tabella profiles

```sql
CREATE TABLE profiles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id),  -- proprietario del profilo
    slug TEXT UNIQUE NOT NULL,              -- "default", "fabio-personal", "acme-corp"
    display_name TEXT NOT NULL,             -- "Fabio Personale", "AcmeCorp"
    avatar_emoji TEXT DEFAULT '👤',
    profile_json TEXT NOT NULL DEFAULT '{}',  -- PROFILE.json content (AIEOS-inspired)
    is_default INTEGER NOT NULL DEFAULT 0,    -- esattamente un record = 1 per user
    storage_config TEXT NOT NULL DEFAULT '{}', -- config DB esterni (vedi sezione)
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- v2: seed profilo default per l'admin
INSERT INTO profiles (user_id, slug, display_name, is_default)
    VALUES (1, 'default', 'Default', 1);
```

### Struttura directory brain

```
~/.homun/brain/profiles/
├── default/                    # sempre presente, non eliminabile
│   ├── PROFILE.json            # identità strutturata (AIEOS-inspired)
│   ├── SOUL.md                 # personalità in prosa
│   ├── USER.md                 # chi sono in questo contesto
│   ├── INSTRUCTIONS.md         # istruzioni apprese operando come questo profilo
│   └── skills/                 # skill specifiche di questo profilo
├── fabio-personal/
│   ├── PROFILE.json
│   ├── SOUL.md
│   ├── USER.md
│   ├── INSTRUCTIONS.md
│   └── skills/
└── acme-corp/
    ├── ...
```

### PROFILE.json (AIEOS-inspired)

Struttura generata/assistita da LLM, editabile dall'utente:

```json
{
  "version": "1.0",
  "identity": {
    "name": "Fabio Cannavò",
    "display_name": "Fabio",
    "bio": "Software engineer, padre di Felicia",
    "role": "personal",
    "avatar_emoji": "👤"
  },
  "linguistics": {
    "language": "it",
    "formality": "informal",
    "style": "direct, warm, concise",
    "forbidden_words": [],
    "catchphrases": []
  },
  "personality": {
    "traits": ["curious", "pragmatic", "protective"],
    "tone": "friendly",
    "humor": true
  },
  "capabilities": {
    "tools_emphasis": ["remember", "web_search"],
    "domains": ["tech", "family", "finance"]
  },
  "visibility": {
    "readable_from": ["default"]
  }
}
```

`visibility.readable_from` controlla l'isolamento:
- `["default"]` → questo profilo può leggere anche dal profilo default
- `["*"]` → può leggere da tutti i profili
- `[]` → completamente isolato

## DB Esterni e RAG esterno

Ogni profilo può avere il proprio storage backend per dati che non stanno nel SQLite locale.

### storage_config nel profilo

```json
{
  "database": {
    "type": "sqlite",
    "url": null
  },
  "vector_store": {
    "type": "local",
    "url": null
  },
  "rag_sources": []
}
```

Valori possibili:

**database.type:**
- `"sqlite"` (default) — usa il DB locale `homun.db`
- `"postgres"` — connessione esterna: `"url": "postgres://user:pass@host/db"`
- `"mysql"` — connessione esterna

**vector_store.type:**
- `"local"` (default) — HNSW locale in SQLite
- `"qdrant"` — `"url": "http://localhost:6333"`
- `"pinecone"` — `"url": "...", "api_key": "vault:pinecone_key"`

**rag_sources:** — fonti aggiuntive per questo profilo
- `{"type": "mcp", "server": "notion"}` — documenti via MCP
- `{"type": "s3", "bucket": "company-docs", "prefix": "legal/"}` — S3
- `{"type": "directory", "path": "/mnt/shared/company-docs"}` — directory locale/montata

### Implementazione v2

In v2 solo `sqlite` + `local` sono implementati. I tipi esterni sono definiti nel config ma restituiscono errore "not yet implemented" se usati. Questo permette di:
- Validare il modello dati
- Mostrare i campi nella UI (disabilitati con tooltip "coming soon")
- Implementare i connettori uno alla volta senza breaking change

## Colonne user_id + profile_id aggiunte

```sql
-- Tutte le tabelle con dati scoped ricevono entrambe le FK.
-- In v2 user_id è sempre 1. In v3 filtra per utente.

ALTER TABLE memory_chunks ADD COLUMN user_id INTEGER REFERENCES users(id) DEFAULT 1;
ALTER TABLE memory_chunks ADD COLUMN profile_id INTEGER REFERENCES profiles(id);

ALTER TABLE knowledge_chunks ADD COLUMN user_id INTEGER REFERENCES users(id) DEFAULT 1;
ALTER TABLE knowledge_chunks ADD COLUMN profile_id INTEGER REFERENCES profiles(id);

ALTER TABLE contacts ADD COLUMN user_id INTEGER REFERENCES users(id) DEFAULT 1;
ALTER TABLE contacts ADD COLUMN profile_id INTEGER REFERENCES profiles(id);

ALTER TABLE sessions ADD COLUMN user_id INTEGER REFERENCES users(id) DEFAULT 1;
ALTER TABLE sessions ADD COLUMN profile_id INTEGER REFERENCES profiles(id);

ALTER TABLE vault_entries ADD COLUMN user_id INTEGER REFERENCES users(id) DEFAULT 1;
ALTER TABLE vault_entries ADD COLUMN profile_id INTEGER REFERENCES profiles(id);

ALTER TABLE workflows ADD COLUMN user_id INTEGER REFERENCES users(id) DEFAULT 1;
ALTER TABLE workflows ADD COLUMN profile_id INTEGER REFERENCES profiles(id);

ALTER TABLE automations ADD COLUMN user_id INTEGER REFERENCES users(id) DEFAULT 1;
ALTER TABLE automations ADD COLUMN profile_id INTEGER REFERENCES profiles(id);

ALTER TABLE logs ADD COLUMN user_id INTEGER REFERENCES users(id) DEFAULT 1;
ALTER TABLE logs ADD COLUMN profile_id INTEGER REFERENCES profiles(id);

-- Indici per performance
CREATE INDEX idx_memory_chunks_profile ON memory_chunks(profile_id);
CREATE INDEX idx_knowledge_chunks_profile ON knowledge_chunks(profile_id);
CREATE INDEX idx_sessions_profile ON sessions(profile_id);
CREATE INDEX idx_workflows_profile ON workflows(profile_id);
CREATE INDEX idx_automations_profile ON automations(profile_id);
```

## Isolamento: lettura cross-profile, scrittura isolata

- **Scrittura**: sempre nel profilo attivo. Remember tool, consolidation, RAG ingest, vault, workflow, automations scrivono con `user_id` + `profile_id` correnti.
- **Lettura**: il profilo attivo vede i propri dati + quelli dei profili elencati in `visibility.readable_from`.
- **Query**: tutte le query aggiungono `WHERE user_id = ? AND profile_id IN (attivo, ...readable_from)`.
- **v3 multi-utente**: il filtro `user_id` diventa significativo. L'admin vede tutto, l'utente vede solo i propri profili.

## Skill: globali + per-profilo

- Le skill in `~/.homun/skills/` e `./skills/` sono **globali** (disponibili a tutti i profili).
- Le skill in `~/.homun/brain/profiles/{slug}/skills/` sono **specifiche** di quel profilo.
- Il loader scansiona entrambi: globali + profilo attivo.
- Nella cognition phase, `list_skills` restituisce l'unione.
- Nella UI: ogni skill ha un flag `profile: "all" | "profile-slug"`.

## Impatto sui componenti

### Agent Loop (`agent_loop.rs`)

```
Messaggio in arrivo
  → identifica utente (v2: sempre admin; v3: da auth)
  → identifica contatto (channel:chat_id → Contact)
  → risolvi profilo: Contact.profile_id > Channel.default_profile > "default"
  → carica brain files dal profilo attivo:
      SOUL.md    = profiles/{slug}/SOUL.md
      USER.md    = profiles/{slug}/USER.md
      INSTRUCTIONS.md = profiles/{slug}/INSTRUCTIONS.md
  → carica PROFILE.json per linguistics/personality/capabilities
  → inietta nel prompt
  → esegui con tool scoped a user_id + profile_id
```

### Prompt Builder (`prompt/sections.rs`)

- `IdentitySection`: carica SOUL.md dal profilo attivo (non più globale)
- Nuova sezione `ProfileSection`: inietta info strutturate da PROFILE.json (linguistics, personality)
- `PersonaSection` → rinominata/rimossa (il profilo sostituisce la persona)

### Bootstrap Watcher (`bootstrap_watcher.rs`)

- Watch su `brain/profiles/{active_slug}/` invece di `brain/`
- Quando cambia profilo attivo, ricarica i file
- Fallback: se un file non esiste nel profilo, usa quello del default

### Remember Tool (`tools/remember.rs`)

- Riceve `profile_id` dal contesto della sessione
- Scrive su `brain/profiles/{slug}/USER.md` invece di `brain/USER.md`

### Memory Consolidation (`memory.rs`)

- `consolidate()` riceve `user_id` + `profile_id` + `contact_id`
- Appende a `brain/profiles/{slug}/INSTRUCTIONS.md`
- Nuovi memory_chunks salvati con `user_id` + `profile_id`

### Memory Search (`memory_search.rs`)

- `search_scoped_full(query, topk, contact_id, user_id, profile_id)` → aggiunge filtri
- Cerca in: `user_id = attivo AND (profile_id = attivo OR profile_id IN readable_from OR profile_id IS NULL)`

### RAG (`rag/engine.rs`)

- Ingest: tag `user_id` + `profile_id` su ogni chunk
- Search: filtra per `user_id` + `profile_id IN (attivo, ...readable_from, NULL)`
- UI: quando fai upload, scegli il profilo destinazione

### Skills (`skills/loader.rs`)

- Scansiona: `~/.homun/skills/` (globali) + `brain/profiles/{slug}/skills/` (per-profilo)
- `SkillDefinition` aggiunge campo `profile: Option<String>`
- Cognition discovery filtra per profilo attivo

### Vault (`storage/secrets.rs`)

- Entries taggati con `user_id` + `profile_id`
- v2: un profilo aziendale ha le proprie API key separate dal profilo personale
- Query: `WHERE user_id = ? AND (profile_id = ? OR profile_id IS NULL)`

### Workflows + Automations (`workflows/`, `scheduler/automations.rs`)

- Ogni workflow/automation appartiene a un `user_id` + `profile_id`
- Un'automation del profilo aziendale non triggera nel profilo personale
- Query: filtro per profilo attivo

### Logs

- Ogni log entry taggato con `user_id` + `profile_id`
- Filtrabile nella UI logs per profilo

### Config (`config/schema.rs`)

```toml
[profiles]
default = "default"

[channels.telegram]
default_profile = "fabio-personal"

[channels.whatsapp]
default_profile = "fabio-personal"
```

`ChannelBehavior` trait: `fn persona(&self)` → `fn default_profile(&self)`

### Contacts (`contacts/mod.rs`)

```rust
pub struct Contact {
    // Aggiunto:
    pub user_id: i64,                 // FK a users (v2: sempre 1)
    pub profile_id: Option<i64>,      // FK a profiles, NULL = usa default canale
    // Mantenuti per backward compat durante transizione:
    pub persona_override: Option<String>,
    pub persona_instructions: String,
}
```

### Session (`session/manager.rs`)

- `SessionRow.user_id: i64` — utente della sessione (v2: sempre 1)
- `SessionRow.profile_id: Option<i64>` — profilo attivo per la sessione
- Settato all'inizio della sessione, modificabile dall'utente
- Usato da agent loop per risolvere il profilo

### API (`web/api/`)

Nuovi endpoint:

```
GET    /api/v1/profiles                    # lista profili (dell'utente corrente)
POST   /api/v1/profiles                    # crea profilo
GET    /api/v1/profiles/{id}               # dettaglio
PUT    /api/v1/profiles/{id}               # aggiorna
DELETE /api/v1/profiles/{id}               # elimina (non default)
POST   /api/v1/profiles/{id}/generate      # genera PROFILE.json via LLM
GET    /api/v1/profiles/{id}/soul          # leggi SOUL.md
PUT    /api/v1/profiles/{id}/soul          # scrivi SOUL.md
GET    /api/v1/profiles/{id}/user          # leggi USER.md
PUT    /api/v1/profiles/{id}/user          # scrivi USER.md
GET    /api/v1/profiles/{id}/instructions  # leggi INSTRUCTIONS.md
```

Query param globale: `?profile={slug}` su endpoint che supportano scoping (memory, knowledge, skills, vault, workflows, automations, logs). Se omesso, usa profilo della sessione o default.

### Chat UI

- **Selettore profilo** nella toolbar della chat (dropdown con emoji + nome)
- Cambiare profilo cambia il `profile_id` della sessione
- Indicatore visivo del profilo attivo (emoji badge accanto al nome della chat)
- Il messaggio di benvenuto riflette il profilo ("Ciao, sto rispondendo come AcmeCorp")

### Chat da canali (Telegram, WhatsApp, etc.)

- Comando `/profile <slug>` per cambiare profilo mid-conversation
- Se non specificato, usa: Contact.profile_id > Channel.default_profile > "default"
- Il profilo viene memorizzato nella sessione corrente

### Profiles Page (Settings → Profiles)

Layout master-detail come la rubrica contatti:

- **Sidebar**: lista profili con emoji + nome, profilo default evidenziato
- **Detail pane**:
  - Header: emoji grande + nome + slug + "Default" badge
  - **Identity section**: campi da PROFILE.json (name, bio, role)
  - **Linguistics section**: formality, style, forbidden_words
  - **Personality section**: traits, tone, humor
  - **Visibility section**: checkbox profili da cui leggere
  - **Storage section**: config DB/vector store (v2: solo local, campi disabilitati per esterni)
  - **Brain files section**: editor SOUL.md, link a USER.md e INSTRUCTIONS.md
  - **Skills section**: lista skill associate a questo profilo
- **Genera con AI**: bottone che chiama `/api/v1/profiles/{id}/generate` con un prompt per generare PROFILE.json a partire da una descrizione testuale

## Migrazione dati esistenti

1. Crea tabella `users` con seed admin (`id=1`)
2. Crea tabella `profiles` con seed default (`id=1, user_id=1`)
3. Sposta `~/.homun/brain/SOUL.md` → `~/.homun/brain/profiles/default/SOUL.md`
4. Sposta `~/.homun/brain/USER.md` → `~/.homun/brain/profiles/default/USER.md`
5. Sposta `~/.homun/brain/INSTRUCTIONS.md` → `~/.homun/brain/profiles/default/INSTRUCTIONS.md`
6. Tutti i record esistenti → `user_id = 1, profile_id = 1` (default)
7. `Contact.persona_override = "owner"` → crea profilo "owner", associa
8. `Contact.persona_override = "company"` → crea profilo "company", associa
9. `Contact.persona_override = "custom"` → crea profilo con `persona_instructions` come SOUL.md

## Piano di implementazione (ordine suggerito)

### Sprint 1: Foundation (DB + Config + Brain files)
1. Migration: `users` table + seed admin
2. Migration: `profiles` table + seed default
3. Migration: `user_id` + `profile_id` su tutte le tabelle (memory_chunks, knowledge_chunks, contacts, sessions, vault_entries, workflows, automations, logs) + indici
4. `src/profiles/mod.rs` — struct Profile, ProfileRegistry, load/save PROFILE.json
5. `src/profiles/db.rs` — CRUD
6. Migration script dati esistenti (sposta file, tagga record)
7. Config: `[profiles]` section, `default_profile` su canali

### Sprint 2: Agent Loop Integration
8. Refactor `persona.rs` → `profile_resolver.rs` (risolve profilo, non persona)
9. `agent_loop.rs` — risolvi profilo, carica brain files dal profilo attivo, passa user_id + profile_id ai tool
10. `bootstrap_watcher.rs` — watch su directory profilo attivo
11. `prompt/sections.rs` — IdentitySection carica dal profilo, nuova ProfileSection
12. `remember.rs` — scrive nel profilo attivo
13. `memory.rs` — consolidation nel profilo attivo con user_id + profile_id

### Sprint 3: Search + Skills + Storage Scoping
14. `memory_search.rs` — filtro user_id + profile_id + readable_from
15. `rag/engine.rs` — tag + filtro user_id + profile_id
16. `skills/loader.rs` — scan globali + per-profilo
17. `cognition/discovery.rs` — filtra skill per profilo
18. `vault` — scoping per user_id + profile_id
19. `workflows` + `automations` — scoping per user_id + profile_id
20. `logs` — tagging per user_id + profile_id

### Sprint 4: API + Web UI
21. `web/api/profiles.rs` — CRUD + generate + brain file endpoints
22. `static/js/profiles.js` — pagina gestione profili (master-detail)
23. `web/pages.rs` — template pagina profili + sidebar link
24. Chat UI — selettore profilo + indicatore
25. Contacts UI — dropdown profili (già predisposto in contacts.js)
26. `/profile` comando nei canali
27. Filtro per profilo nelle pagine: vault, workflows, automations, logs, knowledge

## Preparazione v3 (multi-utente) — NON implementare ora

Quando serve, il passaggio a v3 richiede:

1. **Auth per utente**: la tabella `users` ha già lo schema. Aggiungere `password_hash`, `email`, `last_login`
2. **RBAC**: `users.role` diventa significativo. Admin vede tutto, user vede solo i propri profili
3. **API scoping**: ogni endpoint filtra per `user_id` dalla sessione autenticata
4. **UI admin**: pagina gestione utenti (CRUD), assegnazione profili
5. **Inviti**: flow di invito utente con email + OTP
6. **DB per utente**: `storage_config` del profilo può puntare a DB diversi per tenant isolation

Il costo stimato per v3 è basso perché `user_id` è già FK ovunque — serve solo la logica di auth multi-utente e la UI admin.

## Rischi e mitigazioni

| Rischio | Mitigazione |
|---|---|
| Breaking change sui brain files | Migrazione automatica all'avvio + fallback su path legacy |
| Performance query con filtro user_id + profile_id | Indici compositi sulle tabelle |
| Complessità prompt (troppi file) | PROFILE.json è opzionale, SOUL.md resta il minimo |
| Confusione utente | Profilo "default" sempre presente, funziona senza configurare nulla |
| DB esterni non implementati in v2 | Campi visibili ma disabilitati con "coming soon" |
| user_id sempre 1 in v2 | Invisibile nella UI, DEFAULT 1 nelle migration |
