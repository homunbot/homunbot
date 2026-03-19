-- Add notify routing to pending_responses for unified channel approval
ALTER TABLE pending_responses ADD COLUMN notify_channel TEXT;
ALTER TABLE pending_responses ADD COLUMN notify_chat_id TEXT;
