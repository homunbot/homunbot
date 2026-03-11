-- Web authentication: password hash for login, scope for API keys.
-- SEC-1 + SEC-4: Web Security Hardening (P0)

ALTER TABLE users ADD COLUMN password_hash TEXT;

ALTER TABLE webhook_tokens ADD COLUMN scope TEXT NOT NULL DEFAULT 'admin';
