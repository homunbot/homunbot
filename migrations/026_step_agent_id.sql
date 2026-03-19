-- MAG-4: per-step agent routing for multi-agent pipelines
ALTER TABLE workflow_steps ADD COLUMN agent_id TEXT DEFAULT 'default';
