-- Each notification is recorded once for each channel it goes out on (ADR 0122), so a channel
-- that's failing retries on its own without resending the others. Rows from before were Slack's.
ALTER TABLE notifications ADD COLUMN channel text NOT NULL DEFAULT 'slack';
ALTER TABLE notifications ALTER COLUMN channel DROP DEFAULT;
