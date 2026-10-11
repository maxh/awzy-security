-- Append-only history of the outage fallback setting (ADR 0089): each MCP tool's tier and which
-- tiers fail open while the PDP is unreachable. The current setting is the highest version; with
-- none, every tool is unclassified and fails closed. Rows are never updated or deleted, so the
-- history is the audit trail of who changed what, when.
CREATE TABLE outage_fallback_changes (
    version         bigint PRIMARY KEY CHECK (version > 0),
    tools           jsonb NOT NULL,
    servers         jsonb NOT NULL,
    fail_open_tiers text[] NOT NULL,
    changed_by      text NOT NULL,
    changed_at      timestamptz NOT NULL,
    signature       bytea,
    signing_key_id  text
);
