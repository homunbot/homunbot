-- Add profile_id FK to core tables for profile-scoped data.
-- NULL means "unscoped" (legacy data before profiles existed).
-- Note: backfill UPDATEs are done in Rust after this migration
-- because ALTER TABLE + FTS5 content-sync triggers cause SQLite
-- to invalidate the table name for subsequent DML in the same batch.

ALTER TABLE memory_chunks ADD COLUMN profile_id INTEGER REFERENCES profiles(id);
CREATE INDEX IF NOT EXISTS idx_memory_chunks_profile ON memory_chunks(profile_id);

ALTER TABLE rag_chunks ADD COLUMN profile_id INTEGER REFERENCES profiles(id);
CREATE INDEX IF NOT EXISTS idx_rag_chunks_profile ON rag_chunks(profile_id);

ALTER TABLE contacts ADD COLUMN profile_id INTEGER REFERENCES profiles(id);
CREATE INDEX IF NOT EXISTS idx_contacts_profile ON contacts(profile_id);

ALTER TABLE sessions ADD COLUMN profile_id INTEGER REFERENCES profiles(id);
CREATE INDEX IF NOT EXISTS idx_sessions_profile ON sessions(profile_id);
