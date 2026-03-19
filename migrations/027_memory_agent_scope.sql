-- MEM-2: Add agent_id to memory_chunks for per-agent memory scoping.
-- NULL = global (visible to all agents), non-NULL = agent-specific.
ALTER TABLE memory_chunks ADD COLUMN agent_id TEXT DEFAULT NULL;
CREATE INDEX idx_chunks_agent ON memory_chunks(agent_id);
