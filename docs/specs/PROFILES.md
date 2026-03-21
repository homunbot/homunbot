# SPEC: Profile System (Profilo-First Architecture)

> Status: DESIGN — non implementare prima dell'approvazione
> Data: 2026-03-21

## Problema

Oggi Homun ha un'identità singleton: un unico SOUL.md, USER.md, INSTRUCTIONS.md globali. Le persona sono 4 stringhe hardcodate (bot/owner/company/custom) con un match in `persona.rs`. Non c'è modo di:

- Accumulare conoscenze diverse per contesti diversi (personale vs aziendale)
- Avere skill specifiche per contesto
- Filtrare documenti RAG per contesto
- Gestire identità strutturate (stile AIEOS)

## Modello: Profilo-First

Il **profilo** diventa l'unità fondamentale. Tutto è legato a un profilo: identità, memoria, istruzioni apprese, skill, documenti, contatti.

### Struttura directory

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
    ├── PROFILE.json
    ├── SOUL.md
    ├── USER.md
    ├── INSTRUCTIONS.md
    └── skills/
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

La sezione `visibility.readable_from` controlla l'isolamento:
- `["default"]` → questo profilo può leggere anche dal profilo default
- `["*"]` → può leggere da tutti i profili
- `[]` → completamente isolato

### Tabella DB: profiles

```sql
CREATE TABLE profiles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    slug TEXT UNIQUE NOT NULL,          -- "default", "fabio-personal", "acme-corp"
    display_name TEXT NOT NULL,         -- "Fabio Personale", "AcmeCorp"
    avatar_emoji TEXT DEFAULT '👤',
    profile_json TEXT NOT NULL DEFAULT '{}',  -- PROFILE.json content cached
    is_default INTEGER NOT NULL DEFAULT 0,    -- exactly one row = 1
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Seed default profile
INSERT INTO profiles (slug, display_name, is_default) VALUES ('default', 'Default', 1);
```

### Colonne profile_id aggiunte

```sql
-- Memory: ogni chunk appartiene a un profilo
ALTER TABLE memory_chunks ADD COLUMN profile_id INTEGER REFERENCES profiles(id);

-- Knowledge/RAG: ogni documento taggato per profilo
ALTER TABLE knowledge_chunks ADD COLUMN profile_id INTEGER REFERENCES profiles(id);

-- Contacts: ogni contatto ha un profilo di default per le risposte
ALTER TABLE contacts ADD COLUMN profile_id INTEGER REFERENCES profiles(id);
-- (sostituisce persona_override + persona_instructions)

-- Sessions: ogni sessione opera in un profilo
ALTER TABLE sessions ADD COLUMN profile_id INTEGER REFERENCES profiles(id);

-- Canali: profilo di default per canale
-- (gestito in config.toml, non in DB)
```

## Isolamento: lettura cross-profile, scrittura isolata

- **Scrittura**: sempre nel profilo attivo. Remember tool, consolidation, RAG ingest scrivono con `profile_id` del profilo corrente.
- **Lettura**: il profilo attivo vede i propri dati + quelli dei profili elencati in `visibility.readable_from`.
- **Query**: `memory_search` e RAG filtrano per `profile_id IN (attivo, ...readable_from)`.
- **Brain files**: caricati dalla directory del profilo attivo. USER.md/INSTRUCTIONS.md del profilo attivo, non del default (a meno che il default sia in readable_from, ma i brain files sono per-profilo, non uniti).

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
  → identifica contatto (channel:chat_id → Contact)
  → risolvi profilo: Contact.profile_id > Channel.default_profile > "default"
  → carica brain files dal profilo attivo:
      SOUL.md    = profiles/{slug}/SOUL.md
      USER.md    = profiles/{slug}/USER.md
      INSTRUCTIONS.md = profiles/{slug}/INSTRUCTIONS.md
  → carica PROFILE.json per linguistics/personality/capabilities
  → inietta nel prompt
  → esegui con tool scoped al profilo
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

- `consolidate()` riceve `profile_id` + `contact_id`
- Appende a `brain/profiles/{slug}/INSTRUCTIONS.md`
- Nuovi memory_chunks salvati con `profile_id`

### Memory Search (`memory_search.rs`)

- `search_scoped_full(query, topk, contact_id, profile_id)` → aggiunge filtro
- Cerca in: `profile_id = attivo OR profile_id IN readable_from OR profile_id IS NULL`

### RAG (`rag/engine.rs`)

- Ingest: tag `profile_id` su ogni chunk
- Search: filtra per `profile_id IN (attivo, ...readable_from, NULL)`
- UI: quando fai upload, scegli il profilo destinazione

### Skills (`skills/loader.rs`)

- Scansiona: `~/.homun/skills/` (globali) + `brain/profiles/{slug}/skills/` (per-profilo)
- `SkillDefinition` aggiunge campo `profile: Option<String>`
- Cognition discovery filtra per profilo attivo

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
    // Sostituisce persona_override + persona_instructions:
    pub profile_id: Option<i64>,  // FK a profiles, NULL = usa default canale
    // Rimuovi:
    // pub persona_override: Option<String>,
    // pub persona_instructions: String,
}
```

### Session (`session/manager.rs`)

- `SessionRow.profile_id: Option<i64>` — profilo attivo per la sessione
- Settato all'inizio della sessione, modificabile dall'utente
- Usato da agent loop per risolvere il profilo

### API (`web/api/`)

Nuovi endpoint:

```
GET    /api/v1/profiles                    # lista profili
POST   /api/v1/profiles                    # crea profilo
GET    /api/v1/profiles/{id}               # dettaglio
PUT    /api/v1/profiles/{id}               # aggiorna
DELETE /api/v1/profiles/{id}               # elimina (non default)
POST   /api/v1/profiles/{id}/generate      # genera PROFILE.json via LLM
GET    /api/v1/profiles/{id}/soul          # leggi SOUL.md
PUT    /api/v1/profiles/{id}/soul          # scrivi SOUL.md
GET    /api/v1/profiles/{id}/instructions  # leggi INSTRUCTIONS.md
```

Query param globale: `?profile={slug}` su endpoint che supportano scoping (memory, knowledge, skills). Se omesso, usa profilo della sessione o default.

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
  - **Brain files section**: editor SOUL.md, link a USER.md e INSTRUCTIONS.md
  - **Skills section**: lista skill associate a questo profilo
- **Genera con AI**: bottone che chiama `/api/v1/profiles/{id}/generate` con un prompt per generare PROFILE.json a partire da una descrizione testuale

## Migrazione dati esistenti

1. Crea profilo "default" con `is_default = 1`
2. Sposta `~/.homun/brain/SOUL.md` → `~/.homun/brain/profiles/default/SOUL.md`
3. Sposta `~/.homun/brain/USER.md` → `~/.homun/brain/profiles/default/USER.md`
4. Sposta `~/.homun/brain/INSTRUCTIONS.md` → `~/.homun/brain/profiles/default/INSTRUCTIONS.md`
5. Tutti i `memory_chunks` esistenti → `profile_id = default.id`
6. Tutti i `knowledge_chunks` esistenti → `profile_id = default.id`
7. `Contact.persona_override = "owner"` → crea profilo "owner", associa
8. `Contact.persona_override = "company"` → crea profilo "company", associa
9. `Contact.persona_override = "custom"` → crea profilo con `persona_instructions` come SOUL.md

## Piano di implementazione (ordine suggerito)

### Sprint 1: Foundation (DB + Config + Brain files)
1. Migration: `profiles` table + seed default
2. Migration: `profile_id` su memory_chunks, knowledge_chunks, contacts, sessions
3. `src/profiles/mod.rs` — ProfileRegistry, load/save PROFILE.json
4. `src/profiles/db.rs` — CRUD
5. Migration script dati esistenti
6. Config: `[profiles]` section, `default_profile` su canali

### Sprint 2: Agent Loop Integration
7. Refactor `persona.rs` → `profile_resolver.rs` (risolve profilo, non persona)
8. `agent_loop.rs` — risolvi profilo, carica brain files dal profilo attivo
9. `bootstrap_watcher.rs` — watch su directory profilo attivo
10. `prompt/sections.rs` — IdentitySection carica dal profilo, nuova ProfileSection
11. `remember.rs` — scrive nel profilo attivo
12. `memory.rs` — consolidation nel profilo attivo

### Sprint 3: Search + Skills Scoping
13. `memory_search.rs` — filtro profile_id + readable_from
14. `rag/engine.rs` — tag + filtro profile_id
15. `skills/loader.rs` — scan globali + per-profilo
16. `cognition/discovery.rs` — filtra skill per profilo

### Sprint 4: API + Web UI
17. `web/api/profiles.rs` — CRUD + generate endpoint
18. `static/js/profiles.js` — pagina gestione profili (master-detail)
19. `web/pages.rs` — template pagina profili
20. Chat UI — selettore profilo + indicatore
21. Contacts UI — dropdown profili invece di persona enum
22. `/profile` comando nei canali

## Rischi e mitigazioni

| Rischio | Mitigazione |
|---|---|
| Breaking change sui brain files | Migrazione automatica all'avvio + fallback su path legacy |
| Performance query con filtro profile_id | Indice su profile_id nelle tabelle |
| Complessità prompt (troppi file) | PROFILE.json è opzionale, SOUL.md resta il minimo |
| Confusione utente | Profilo "default" sempre presente, funziona senza configurare nulla |
