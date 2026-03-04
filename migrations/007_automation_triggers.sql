-- Automation triggers and conditional notifications (Sprint 4 completion)
-- Adds optional trigger fields to automation definitions.

ALTER TABLE automations ADD COLUMN trigger_kind TEXT NOT NULL DEFAULT 'always';
ALTER TABLE automations ADD COLUMN trigger_value TEXT;

CREATE INDEX IF NOT EXISTS idx_automations_trigger_kind ON automations(trigger_kind);
