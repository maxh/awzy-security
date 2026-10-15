-- The resource change log is read by transaction ID, up to the oldest transaction still running
-- (ADR 0126's resource-cache model). A row's sequence number is taken before its transaction
-- commits, so a write that committed after a later one could land behind a cursor that had
-- already passed it, and its change was never seen. Every row below the oldest running
-- transaction is final, so a cursor at that horizon misses nothing.
ALTER TABLE resource_changes ADD COLUMN txid xid8 NOT NULL DEFAULT pg_current_xact_id();
CREATE INDEX resource_changes_by_txid ON resource_changes (txid);

-- The newest transaction ID pruning has deleted, so a cursor at or before it resets.
CREATE TABLE resource_changes_pruned (
    one   boolean PRIMARY KEY DEFAULT true CHECK (one),
    below xid8 NOT NULL DEFAULT '0'
);
INSERT INTO resource_changes_pruned DEFAULT VALUES;
