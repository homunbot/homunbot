CREATE TABLE IF NOT EXISTS web_chat_runs (
    run_id TEXT PRIMARY KEY,
    session_key TEXT NOT NULL,
    status TEXT NOT NULL,
    user_message TEXT NOT NULL,
    assistant_response TEXT NOT NULL DEFAULT '',
    events_json TEXT NOT NULL DEFAULT '[]',
    error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (session_key) REFERENCES sessions(key) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_web_chat_runs_session_updated
    ON web_chat_runs(session_key, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_web_chat_runs_session_status
    ON web_chat_runs(session_key, status, updated_at DESC);
