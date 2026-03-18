# Homun Contact Book — SPEC

> **Codename**: CTB (Contact Book)
> **Status**: SPEC — da implementare
> **Effort stimato**: 2 settimane (5 sprint da 2 giorni)

## Visione

Trasformare Homun da "bot che risponde a chi gli scrive" a "assistente che conosce le persone, le loro relazioni, e sa come comportarsi con ciascuna". Un CRM personale con social graph, response policy per contatto, e automazioni relazionali (auguri, promemoria, eventi).

## Requisiti chiave

### R1: Rubrica contatti con identita' multi-canale
Ogni contatto ha un nome, bio, note, e N identita' su canali diversi (telegram_id, email, whatsapp, discord_id, slack_id). L'agent risolve automaticamente "Marco" → contatto con tutti i suoi recapiti.

### R2: Social graph con relazioni bidirezionali
Relazioni tra contatti: "madre di", "collega di", "partner di", etc. L'agent puo' navigare il grafo: "manda un messaggio alla mamma di Felicia" → risolve Felicia → trova relazione "madre" → trova contatto madre → usa il suo canale preferito.

### R3: Response mode per canale + per contatto
Tre modalita' (come email):
- **automatic** — risponde subito senza approvazione
- **assisted** — prepara bozza, aspetta approvazione via web/telegram
- **on_demand** — non risponde mai da solo, solo quando richiesto

Default globale per canale + override per contatto. Es: Telegram default=automatic, ma per il capo=assisted.

### R4: Eventi e automazioni relazionali
Date importanti per contatto: compleanno, onomastico, anniversario, custom. Cron giornaliero controlla eventi imminenti. Puo':
- Notificare il proprietario ("Domani e' il compleanno di Marco")
- Inviare auguri automatici (se configurato per quel contatto)
- Generare messaggi personalizzati basati sulla bio e storia conversazionale

### R5: Context injection nell'agent
Quando arriva un messaggio, il system prompt include la scheda contatto:
```
[Contact: Marco Rossi]
Bio: CTO di AcmeCorp, lavoriamo insieme dal 2023
Relationship to you: colleague, close friend
Relationships: married to Laura Bianchi, father of Giulia Rossi
Preferred channel: telegram
Response mode: automatic
Upcoming: birthday in 3 days (March 21)
Recent topics: progetto Alpha, quarterly review
```

### R6: Tool `contacts` per l'agent
L'agent ha un tool per gestire la rubrica:
- `contacts_search(query)` — ricerca fuzzy per nome, nickname, relazione
- `contacts_resolve(description)` — "la mamma di Felicia" → contatto
- `contacts_get(id)` — scheda completa
- `contacts_create(name, ...)` — crea nuovo contatto
- `contacts_update(id, fields)` — aggiorna campi
- `contacts_add_event(id, type, date, label)` — aggiunge evento
- `contacts_list_upcoming(days)` — eventi nei prossimi N giorni
- `contacts_send(id, message)` — invia messaggio via canale preferito

### R7: Web UI — pagina /contacts
- Lista contatti con ricerca fuzzy
- Scheda dettaglio: info, identita', relazioni, eventi, storia
- Grafo relazioni visualizzabile (mini SVG o lista)
- Form per aggiungere/modificare contatti
- Timeline eventi prossimi

---

## Schema Database

### Migration: `NNN_contacts.sql`

```sql
-- Core contact
CREATE TABLE IF NOT EXISTS contacts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    nickname TEXT,
    bio TEXT DEFAULT '',
    notes TEXT DEFAULT '',
    birthday TEXT,              -- ISO date YYYY-MM-DD
    nameday TEXT,               -- ISO date YYYY-MM-DD (onomastico)
    preferred_channel TEXT,     -- "telegram", "whatsapp", "email", etc.
    response_mode TEXT DEFAULT 'automatic',  -- automatic|assisted|on_demand|silent
    tags TEXT DEFAULT '[]',     -- JSON array of tags
    avatar_url TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Multi-channel identities
CREATE TABLE IF NOT EXISTS contact_identities (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    channel TEXT NOT NULL,       -- "telegram", "discord", "slack", "whatsapp", "email", "phone"
    identifier TEXT NOT NULL,    -- chat_id, email address, phone number, etc.
    label TEXT,                  -- "personale", "lavoro", etc.
    UNIQUE(channel, identifier)
);
CREATE INDEX IF NOT EXISTS idx_contact_identities_lookup ON contact_identities(channel, identifier);

-- Relationships between contacts (social graph)
CREATE TABLE IF NOT EXISTS contact_relationships (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    to_contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    relationship_type TEXT NOT NULL,  -- "madre", "padre", "figlio/a", "partner", "collega", "amico/a", "capo", custom
    bidirectional INTEGER DEFAULT 0,  -- if 1, implies reverse relationship
    reverse_type TEXT,                -- "figlio/a" if relationship_type is "madre"
    notes TEXT,
    UNIQUE(from_contact_id, to_contact_id, relationship_type)
);

-- Recurring events (birthdays, namedays, anniversaries, custom)
CREATE TABLE IF NOT EXISTS contact_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,    -- "birthday", "nameday", "anniversary", "custom"
    date TEXT NOT NULL,          -- ISO date YYYY-MM-DD (year optional for recurring)
    recurrence TEXT DEFAULT 'yearly',  -- "yearly", "once", "monthly"
    label TEXT,                  -- "Compleanno", "Anniversario matrimonio", custom
    auto_greet INTEGER DEFAULT 0,      -- if 1, send automated greeting
    greet_template TEXT,               -- custom greeting template (optional)
    notify_days_before INTEGER DEFAULT 1,  -- notify owner N days before
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_contact_events_date ON contact_events(date);

-- Per-channel global response mode (extends current config)
-- This is stored in config.toml, not DB. Schema here for reference:
-- [channels.telegram]
-- response_mode = "automatic"  # default for all telegram contacts
-- [channels.email]
-- response_mode = "assisted"   # default for all email contacts
```

## Response Mode Flow

### Nel gateway (gateway.rs)

Quando arriva un inbound message:

```
1. Identifica il contatto dal (channel, chat_id) via contact_identities
2. Determina response_mode:
   a. Se contatto ha override → usa quello
   b. Altrimenti → usa default del canale (config.toml)
   c. Fallback → "automatic"
3. In base al mode:
   - automatic → processa normalmente (come ora)
   - assisted → processa, ma NON invia la risposta.
     Salva come bozza in `pending_responses` table.
     Notifica owner via web/telegram: "Bozza pronta per [contatto]"
   - on_demand → non processa. Salva il messaggio come "pending".
     Notifica owner: "Messaggio da [contatto] in attesa"
   - silent → ignora completamente, nessun log
```

### Tabella bozze (riusa `email_pending` pattern)

```sql
CREATE TABLE IF NOT EXISTS pending_responses (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    contact_id INTEGER REFERENCES contacts(id),
    channel TEXT NOT NULL,
    chat_id TEXT NOT NULL,
    inbound_content TEXT NOT NULL,
    draft_response TEXT,
    status TEXT DEFAULT 'pending',  -- pending|approved|rejected|expired
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT
);
```

## Architettura Rust

### Nuovi file

```
src/contacts/
    mod.rs          -- re-exports, ContactManager struct
    db.rs           -- CRUD operations, relationship queries, event queries
    resolver.rs     -- "la mamma di Felicia" → contact resolution via LLM + graph traversal
    context.rs      -- build contact context section for system prompt
    events.rs       -- cron job: scan upcoming events, generate notifications/greetings

src/tools/contacts.rs  -- Tool implementation (contacts_search, contacts_resolve, etc.)

src/web/api/contacts.rs -- REST API endpoints

static/js/contacts.js   -- Web UI
```

### File da modificare

```
src/agent/context.rs     -- inject contact context into system prompt
src/agent/gateway.rs     -- response mode routing (assisted/on_demand flow)
src/config/schema.rs     -- per-channel response_mode default
src/web/pages.rs         -- /contacts page template
src/web/server.rs        -- route registration
src/web/api/mod.rs       -- mount contacts API routes
src/tools/registry.rs    -- register contacts tool
migrations/NNN_contacts.sql  -- new migration
```

## Contact Resolution (resolver.rs)

L'agent dice "manda un messaggio alla mamma di Felicia". Il resolver:

```
1. Parse NLP: estrai target="mamma di Felicia"
2. Cerca "Felicia" in contacts (fuzzy match su name, nickname)
3. Se trovata: cerca relazione "madre" da Felicia
4. Se trovata relazione → restituisci il contatto madre
5. Se non trovata relazione → chiedi all'utente
6. Determina canale: usa preferred_channel del contatto target
7. Se canale non disponibile → fallback al canale da cui e' arrivata la richiesta
```

La risoluzione NLP usa `llm_one_shot()` con un prompt specifico:
```
Given the user request and the contact database, resolve the target contact.
User said: "manda un messaggio alla mamma di Felicia"
Contacts: [list]
Relationships: [list]
→ Return: { contact_id: X, channel: "whatsapp", confidence: 0.95 }
```

## Tool Schema

```json
{
  "name": "contacts",
  "description": "Manage the personal contact book. Search, resolve relationships, send messages.",
  "parameters": {
    "action": "search|resolve|get|create|update|add_event|upcoming|send",
    "query": "search query or natural language description",
    "contact_id": "integer ID",
    "fields": { "name": "", "bio": "", "nickname": "", ... },
    "message": "message content for send action",
    "days": "number of days for upcoming events"
  }
}
```

## API Endpoints

```
GET    /api/v1/contacts                 — lista contatti (con ricerca ?q=)
POST   /api/v1/contacts                 — crea contatto
GET    /api/v1/contacts/:id             — dettaglio contatto
PUT    /api/v1/contacts/:id             — aggiorna contatto
DELETE /api/v1/contacts/:id             — elimina contatto
GET    /api/v1/contacts/:id/relationships — relazioni del contatto
POST   /api/v1/contacts/:id/relationships — aggiungi relazione
DELETE /api/v1/contacts/relationships/:id — rimuovi relazione
GET    /api/v1/contacts/:id/events      — eventi del contatto
POST   /api/v1/contacts/:id/events      — aggiungi evento
DELETE /api/v1/contacts/events/:id       — rimuovi evento
GET    /api/v1/contacts/upcoming?days=7  — eventi prossimi
GET    /api/v1/contacts/resolve?q=...    — risolvi descrizione NLP
POST   /api/v1/contacts/pending          — lista bozze in attesa
POST   /api/v1/contacts/pending/:id/approve — approva bozza
POST   /api/v1/contacts/pending/:id/reject  — rifiuta bozza
```

## Sprint Plan

### Sprint CTB-1: Foundation (2 giorni)
- Migration SQL (4 tabelle)
- `src/contacts/mod.rs` + `db.rs` (CRUD base)
- Unit tests per DB operations
- API endpoints base (CRUD contacts + identities)

### Sprint CTB-2: Relationships + Resolution (2 giorni)
- `contact_relationships` CRUD
- `resolver.rs` — graph traversal + LLM resolution
- `contacts_search` e `contacts_resolve` nel tool
- API endpoints relazioni

### Sprint CTB-3: Response Mode + Gateway (2 giorni)
- Per-channel `response_mode` in config.toml
- Per-contact override in DB
- Gateway flow: automatic/assisted/on_demand/silent
- `pending_responses` table + approval flow
- Riuso pattern `email_pending` per bozze

### Sprint CTB-4: Events + Automations (2 giorni)
- `contact_events` CRUD
- `events.rs` — cron giornaliero per scan eventi
- Notifiche owner per eventi imminenti
- Auto-greeting con template personalizzabile
- `contacts_list_upcoming` nel tool

### Sprint CTB-5: Context + UI (2 giorni)
- `context.rs` — inject contatto nel system prompt
- Pagina `/contacts` nella Web UI
- Lista contatti con ricerca
- Scheda dettaglio con relazioni + eventi
- Form creazione/modifica

## Note implementative

- **DRY**: riusare `llm_one_shot()` per resolver, `db.rs` patterns per CRUD, `email_pending` per bozze
- **File size**: `contacts/db.rs` sara' il piu' grande (~300 righe). Splittare se >400.
- **Memoria**: la bio del contatto e' complementare a USER.md. Non duplicare — il contact context si aggiunge al prompt solo quando quel contatto scrive.
- **Privacy**: i dati contatto sono locali (SQLite). Mai sincronizzati con cloud. Vault per dati sensibili.
- **Performance**: `contact_identities` ha indice su (channel, identifier) per lookup O(1) su inbound.
- **Backward compat**: `allow_from` continua a funzionare. I contatti sono un layer sopra — se un sender non e' nei contatti ma e' in allow_from, viene processato normalmente (response_mode=automatic default).
