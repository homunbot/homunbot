-- MEM-5: Hierarchical summarization — weekly/monthly digests of memory chunks.
-- Reduces search space for temporal queries over old content.
CREATE TABLE IF NOT EXISTS memory_summaries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    period TEXT NOT NULL,              -- 'week' or 'month'
    start_date TEXT NOT NULL,          -- YYYY-MM-DD (inclusive)
    end_date TEXT NOT NULL,            -- YYYY-MM-DD (inclusive)
    content TEXT NOT NULL,
    contact_id INTEGER DEFAULT NULL REFERENCES contacts(id) ON DELETE SET NULL,
    agent_id TEXT DEFAULT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_summaries_period ON memory_summaries(period, start_date);
