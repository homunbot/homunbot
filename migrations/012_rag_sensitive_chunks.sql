-- Add sensitivity flag to RAG chunks for automatic redaction of secrets.
ALTER TABLE rag_chunks ADD COLUMN sensitive INTEGER NOT NULL DEFAULT 0;
