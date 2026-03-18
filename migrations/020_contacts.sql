-- Contact Book foundation (CTB-1)

CREATE TABLE IF NOT EXISTS contacts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    nickname TEXT,
    bio TEXT DEFAULT '',
    notes TEXT DEFAULT '',
    birthday TEXT,
    nameday TEXT,
    preferred_channel TEXT,
    response_mode TEXT DEFAULT 'automatic',
    tags TEXT DEFAULT '[]',
    avatar_url TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS contact_identities (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    channel TEXT NOT NULL,
    identifier TEXT NOT NULL,
    label TEXT,
    UNIQUE(channel, identifier)
);
CREATE INDEX IF NOT EXISTS idx_contact_identities_lookup ON contact_identities(channel, identifier);

CREATE TABLE IF NOT EXISTS contact_relationships (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    to_contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    relationship_type TEXT NOT NULL,
    bidirectional INTEGER DEFAULT 0,
    reverse_type TEXT,
    notes TEXT,
    UNIQUE(from_contact_id, to_contact_id, relationship_type)
);

CREATE TABLE IF NOT EXISTS contact_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    contact_id INTEGER NOT NULL REFERENCES contacts(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    date TEXT NOT NULL,
    recurrence TEXT DEFAULT 'yearly',
    label TEXT,
    auto_greet INTEGER DEFAULT 0,
    greet_template TEXT,
    notify_days_before INTEGER DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_contact_events_date ON contact_events(date);

CREATE TABLE IF NOT EXISTS pending_responses (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    contact_id INTEGER REFERENCES contacts(id),
    channel TEXT NOT NULL,
    chat_id TEXT NOT NULL,
    inbound_content TEXT NOT NULL,
    draft_response TEXT,
    status TEXT DEFAULT 'pending',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT
);
