-- User system for multi-user support and channel authentication.
-- Phase 7.1: Foundation for webhook ingress and permission enforcement.

-- Users table: internal user accounts
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,              -- UUID v4
    username TEXT NOT NULL UNIQUE,    -- Display name / login
    roles TEXT NOT NULL DEFAULT '[]', -- JSON array: ["admin", "user"]
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    metadata TEXT NOT NULL DEFAULT '{}'  -- JSON blob for extensibility
);

-- User identities: map channel platform IDs to users
-- A user can have multiple identities (Telegram, Discord, webhook, etc.)
CREATE TABLE IF NOT EXISTS user_identities (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel TEXT NOT NULL,            -- "telegram", "discord", "webhook", "cli"
    platform_id TEXT NOT NULL,        -- Platform-specific ID (Telegram user ID, etc.)
    display_name TEXT,                -- Optional friendly name for the identity
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(channel, platform_id)      -- One platform ID = one user
);

CREATE INDEX IF NOT EXISTS idx_identities_user ON user_identities(user_id);
CREATE INDEX IF NOT EXISTS idx_identities_channel ON user_identities(channel, platform_id);

-- Webhook tokens for external integrations
-- Each token maps to a specific user
CREATE TABLE IF NOT EXISTS webhook_tokens (
    token TEXT PRIMARY KEY,           -- Random secure token
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,               -- Human-readable label
    enabled INTEGER NOT NULL DEFAULT 1,
    last_used TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_webhook_tokens_user ON webhook_tokens(user_id);
