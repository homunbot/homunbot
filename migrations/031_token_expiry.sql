-- Add optional expiry timestamp to webhook tokens.
-- NULL means the token never expires (backwards-compatible default).
ALTER TABLE webhook_tokens ADD COLUMN expires_at TEXT;
