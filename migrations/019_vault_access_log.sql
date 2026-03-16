-- VLT-4: Audit log for vault secret access
CREATE TABLE IF NOT EXISTS vault_access_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    key_name TEXT NOT NULL,
    action TEXT NOT NULL,       -- 'retrieve', 'store', 'delete', 'list', 'reveal'
    source TEXT NOT NULL,       -- 'tool', 'web_api'
    success INTEGER NOT NULL DEFAULT 1,
    user_agent TEXT
);

CREATE INDEX IF NOT EXISTS idx_vault_access_key ON vault_access_log(key_name);
CREATE INDEX IF NOT EXISTS idx_vault_access_ts ON vault_access_log(timestamp);
