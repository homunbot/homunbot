-- Add profile_id to automations and workflows for profile-scoped data.
-- NULL means "unscoped" (legacy data before profiles existed).

ALTER TABLE automations ADD COLUMN profile_id INTEGER REFERENCES profiles(id);
CREATE INDEX IF NOT EXISTS idx_automations_profile ON automations(profile_id);

ALTER TABLE workflows ADD COLUMN profile_id INTEGER REFERENCES profiles(id);
CREATE INDEX IF NOT EXISTS idx_workflows_profile ON workflows(profile_id);
