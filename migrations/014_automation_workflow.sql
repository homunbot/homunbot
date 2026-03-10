-- Add optional workflow steps to automations.
-- When set, the scheduler creates a workflow instead of sending a single prompt.
ALTER TABLE automations ADD COLUMN workflow_steps_json TEXT;
