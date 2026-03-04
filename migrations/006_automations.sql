-- Automations engine (Sprint 4):
-- - automations: definitions (prompt + schedule + state)
-- - automation_runs: execution history

CREATE TABLE IF NOT EXISTS automations (
    id TEXT PRIMARY KEY,                           -- UUID
    name TEXT NOT NULL,
    prompt TEXT NOT NULL,
    schedule TEXT NOT NULL,                       -- "cron:0 9 * * *" | "every:3600"
    enabled INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'active',        -- active | paused | error
    deliver_to TEXT,                              -- "channel:chat_id"
    last_run TEXT,
    last_result TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_automations_enabled ON automations(enabled);
CREATE INDEX IF NOT EXISTS idx_automations_status ON automations(status);

CREATE TABLE IF NOT EXISTS automation_runs (
    id TEXT PRIMARY KEY,                           -- UUID
    automation_id TEXT NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    finished_at TEXT,
    status TEXT NOT NULL DEFAULT 'queued',         -- queued | running | success | error
    result TEXT
);

CREATE INDEX IF NOT EXISTS idx_automation_runs_automation ON automation_runs(automation_id);
CREATE INDEX IF NOT EXISTS idx_automation_runs_started_at ON automation_runs(started_at);
