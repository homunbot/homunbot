-- Memory chunks for vector + FTS5 hybrid search.
-- Each chunk is a piece of consolidated memory (fact, history entry, etc.)
-- stored with its source and type for retrieval.

CREATE TABLE IF NOT EXISTS memory_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL DEFAULT (date('now')),
    source TEXT NOT NULL DEFAULT 'consolidation',
    heading TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL,
    memory_type TEXT NOT NULL DEFAULT 'fact',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_chunks_date ON memory_chunks(date);
CREATE INDEX IF NOT EXISTS idx_chunks_type ON memory_chunks(memory_type);

-- FTS5 virtual table for BM25 full-text search on memory chunks.
-- Synced via triggers so it stays up-to-date automatically.
CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
    content,
    heading,
    content='memory_chunks',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

-- Triggers to keep FTS5 in sync with memory_chunks
CREATE TRIGGER IF NOT EXISTS memory_chunks_ai AFTER INSERT ON memory_chunks BEGIN
    INSERT INTO memory_fts(rowid, content, heading)
    VALUES (new.id, new.content, new.heading);
END;

CREATE TRIGGER IF NOT EXISTS memory_chunks_ad AFTER DELETE ON memory_chunks BEGIN
    INSERT INTO memory_fts(memory_fts, rowid, content, heading)
    VALUES ('delete', old.id, old.content, old.heading);
END;

CREATE TRIGGER IF NOT EXISTS memory_chunks_au AFTER UPDATE ON memory_chunks BEGIN
    INSERT INTO memory_fts(memory_fts, rowid, content, heading)
    VALUES ('delete', old.id, old.content, old.heading);
    INSERT INTO memory_fts(rowid, content, heading)
    VALUES (new.id, new.content, new.heading);
END;
