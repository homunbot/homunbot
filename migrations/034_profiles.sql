-- Profile system: profiles table + seed default profile.
-- A profile is the fundamental identity unit — memory, knowledge, contacts,
-- and sessions are all scoped to a profile.

CREATE TABLE IF NOT EXISTS profiles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    slug TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    avatar_emoji TEXT NOT NULL DEFAULT '👤',
    profile_json TEXT NOT NULL DEFAULT '{}',
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_profiles_slug ON profiles(slug);

-- Seed the default profile (always present, cannot be deleted).
INSERT OR IGNORE INTO profiles (slug, display_name, is_default)
VALUES ('default', 'Default', 1);
