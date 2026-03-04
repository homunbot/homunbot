-- Automation plan metadata and dependency tracking.
-- Used for validation and impact analysis when skills/MCP servers change.

ALTER TABLE automations ADD COLUMN plan_json TEXT;
ALTER TABLE automations ADD COLUMN dependencies_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE automations ADD COLUMN plan_version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE automations ADD COLUMN validation_errors TEXT;

