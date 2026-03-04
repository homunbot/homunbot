-- Email pending queue for assisted/escalated approval flow.
-- Tracks incoming emails that require human approval before the agent can respond.

CREATE TABLE IF NOT EXISTS email_pending (
    id TEXT PRIMARY KEY,                -- UUID
    account_name TEXT NOT NULL,         -- email account name (e.g. "lavoro")
    from_address TEXT NOT NULL,         -- sender email address
    subject TEXT,                       -- email subject line
    body_preview TEXT,                  -- first 500 chars of body
    message_id TEXT,                    -- IMAP Message-ID header (for reply threading)
    draft_response TEXT,                -- agent-generated draft (NULL until generated)
    status TEXT NOT NULL DEFAULT 'pending',  -- pending | approved | sent | rejected | snoozed
    notify_session_key TEXT,            -- session key on the notify channel
    snooze_until TEXT,                  -- ISO 8601 timestamp for snoozed items
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_email_pending_account ON email_pending(account_name);
CREATE INDEX IF NOT EXISTS idx_email_pending_status ON email_pending(status);
