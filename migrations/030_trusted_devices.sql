-- REM-3: Trusted device model — device enrollment + approval.
-- When require_device_approval is enabled, new browsers must be approved before login.
CREATE TABLE IF NOT EXISTS trusted_devices (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    fingerprint TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    user_agent TEXT NOT NULL,
    ip_at_login TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    approved_at TEXT,
    approval_code TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_device_fp ON trusted_devices(user_id, fingerprint);
