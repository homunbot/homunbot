-- MAG-2: per-contact agent routing override
ALTER TABLE contacts ADD COLUMN agent_override TEXT;
