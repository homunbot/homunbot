-- Initial schema for HomunBot

-- Conversation sessions
CREATE TABLE IF NOT EXISTS sessions (
    key TEXT PRIMARY KEY,              -- "channel:chat_id" (e.g., "cli:default", "telegram:123456")
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_consolidated INTEGER NOT NULL DEFAULT 0,
    metadata TEXT NOT NULL DEFAULT '{}'  -- JSON blob for channel-specific data
);

-- Conversation messages (append-only for LLM cache efficiency)
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_key TEXT NOT NULL REFERENCES sessions(key) ON DELETE CASCADE,
    role TEXT NOT NULL,                 -- "user", "assistant", "system", "tool"
    content TEXT NOT NULL,
    tools_used TEXT NOT NULL DEFAULT '[]',  -- JSON array of tool names used
    timestamp TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_key);
CREATE INDEX IF NOT EXISTS idx_messages_session_id ON messages(session_key, id);

-- Long-term memories (consolidated by LLM)
CREATE TABLE IF NOT EXISTS memories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_key TEXT,                   -- NULL for global memories
    content TEXT NOT NULL,
    memory_type TEXT NOT NULL DEFAULT 'summary',  -- "summary", "fact", "preference"
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(session_key);

-- Cron jobs (Phase 4)
CREATE TABLE IF NOT EXISTS cron_jobs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    message TEXT NOT NULL,
    schedule TEXT NOT NULL,             -- cron expression or interval seconds
    enabled INTEGER NOT NULL DEFAULT 1,
    deliver_to TEXT,                    -- channel:chat_id for delivery
    last_run TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
