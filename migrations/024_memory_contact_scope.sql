-- Scope memory chunks by contact for per-contact memory retrieval.
-- NULL = global memory (no contact association).
ALTER TABLE memory_chunks ADD COLUMN contact_id INTEGER DEFAULT NULL REFERENCES contacts(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_chunks_contact ON memory_chunks(contact_id);
