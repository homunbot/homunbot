-- MEM-3: Add importance scoring to memory chunks.
-- Scale 1-5: 1=trivial, 3=normal (default), 5=critical.
-- Used as multiplier in search scoring: final = rrf * (importance/3) * decay.
ALTER TABLE memory_chunks ADD COLUMN importance INTEGER NOT NULL DEFAULT 3;
