-- RAG Knowledge Base: sources and chunks.
-- Sources track ingested documents; chunks store searchable text segments.

CREATE TABLE IF NOT EXISTS rag_sources (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL,
    file_name TEXT NOT NULL,
    file_hash TEXT NOT NULL,
    doc_type TEXT NOT NULL,
    file_size INTEGER NOT NULL DEFAULT 0,
    chunk_count INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending',
    error_message TEXT,
    source_channel TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_rag_sources_hash ON rag_sources(file_hash);
CREATE INDEX IF NOT EXISTS idx_rag_sources_status ON rag_sources(status);

CREATE TABLE IF NOT EXISTS rag_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id INTEGER NOT NULL REFERENCES rag_sources(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    heading TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL,
    token_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_rag_chunks_source ON rag_chunks(source_id);

-- FTS5 virtual table for RAG chunk keyword search (separate from memory_fts)
CREATE VIRTUAL TABLE IF NOT EXISTS rag_fts USING fts5(
    content,
    heading,
    content='rag_chunks',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

-- Triggers to keep rag_fts in sync with rag_chunks
CREATE TRIGGER IF NOT EXISTS rag_chunks_ai AFTER INSERT ON rag_chunks BEGIN
    INSERT INTO rag_fts(rowid, content, heading)
    VALUES (new.id, new.content, new.heading);
END;

CREATE TRIGGER IF NOT EXISTS rag_chunks_ad AFTER DELETE ON rag_chunks BEGIN
    INSERT INTO rag_fts(rag_fts, rowid, content, heading)
    VALUES ('delete', old.id, old.content, old.heading);
END;

CREATE TRIGGER IF NOT EXISTS rag_chunks_au AFTER UPDATE ON rag_chunks BEGIN
    INSERT INTO rag_fts(rag_fts, rowid, content, heading)
    VALUES ('delete', old.id, old.content, old.heading);
    INSERT INTO rag_fts(rowid, content, heading)
    VALUES (new.id, new.content, new.heading);
END;
