-- Append-only history of the model routing setting (ADR 0114): the catalog of models Awzy Bot and
-- investigations may use, and each feature's route. The current setting is the highest version;
-- with none, each service keeps the routing its deploy configuration started it with. Rows are
-- never updated or deleted, so the history is the audit trail of who changed what, when.
CREATE TABLE model_routing_changes (
    version         bigint PRIMARY KEY CHECK (version > 0),
    routing_json    text NOT NULL,
    changed_by      text NOT NULL,
    changed_at      timestamptz NOT NULL,
    signature       bytea,
    signing_key_id  text
);
