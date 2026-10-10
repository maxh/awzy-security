-- Append-only history of enforcement mode switches (ADR 0007). A scope's current mode is its
-- latest row; a scope with no rows is in monitor. Rows are never updated or deleted, so the
-- history doubles as the audit trail of who switched what, when.
CREATE TABLE enforcement_mode_changes (
    id         uuid PRIMARY KEY,
    scope      text NOT NULL,
    mode       text NOT NULL CHECK (mode IN ('monitor', 'enforce')),
    changed_by text NOT NULL,
    changed_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX enforcement_mode_changes_by_scope ON enforcement_mode_changes (scope, changed_at DESC);
