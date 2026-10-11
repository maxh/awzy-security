-- Which chart version each release channel names (ADR 0094): every publish moves `fast`, and a
-- promotion, approved by a second person, moves `stable`. Append-only: a channel's version is its
-- latest row, and the rows are the history of who moved it. The registry proxy reads it to show
-- each install only its channel's version.
CREATE TABLE release_channel_changes (
    id            UUID PRIMARY KEY,
    channel       TEXT NOT NULL CHECK (channel IN ('stable', 'fast')),
    chart_version TEXT NOT NULL,
    changed_by    TEXT NOT NULL,
    changed_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX release_channel_changes_latest ON release_channel_changes (channel, changed_at DESC);
