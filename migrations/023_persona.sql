-- Per-contact persona override (bot/owner/company/custom)
ALTER TABLE contacts ADD COLUMN persona_override TEXT DEFAULT NULL;
ALTER TABLE contacts ADD COLUMN persona_instructions TEXT DEFAULT '';
