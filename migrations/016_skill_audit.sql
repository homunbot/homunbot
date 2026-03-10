-- SKL-6: Skill activation audit log
-- Tracks every skill activation (tool-call or slash command) for usage analytics

CREATE TABLE IF NOT EXISTS skill_audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    skill_name TEXT NOT NULL,
    channel TEXT NOT NULL,
    query TEXT,
    activation_type TEXT NOT NULL DEFAULT 'tool_call',
    success INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_skill_audit_name ON skill_audit(skill_name);
CREATE INDEX IF NOT EXISTS idx_skill_audit_ts ON skill_audit(timestamp);
