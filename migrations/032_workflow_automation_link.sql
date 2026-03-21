-- Link workflows to the automation that spawned them.
-- Enables traceability and proper completion feedback.
ALTER TABLE workflows ADD COLUMN automation_id TEXT REFERENCES automations(id) ON DELETE SET NULL;
ALTER TABLE workflows ADD COLUMN automation_run_id TEXT;
CREATE INDEX IF NOT EXISTS idx_workflows_automation ON workflows(automation_id);
