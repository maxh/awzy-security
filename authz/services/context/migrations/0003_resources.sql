-- What accessors act on, and what they're working on (demo plan step 2). Written by connectors:
-- one row per item in a source, keyed by an ID the connector keeps stable.

-- Documents, folders, repositories, tickets: the Resource entities policies name.
CREATE TABLE resources (
    -- The Resource entity ID, e.g. "synthetic:doc:00042" or a Drive file's "gdrive:<id>".
    id TEXT PRIMARY KEY,
    -- The connector that wrote it, e.g. "synthetic" or "google-drive".
    source TEXT NOT NULL,
    -- document | folder | repository | ticket | ... (a lowercase slug).
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    -- The containing resource's ID (a folder), if any. Not a foreign key: connectors may write a
    -- child before its parent.
    parent TEXT,
    -- A person's userName or a service account ID.
    owner TEXT,
    -- public | internal | confidential | crown_jewels, or NULL until classified.
    sensitivity TEXT,
    labels TEXT[] NOT NULL DEFAULT '{}',
    -- Who it's shared with: [{"kind": "person" | "group" | "domain" | "anyone", "id": ..., "role": ...}]
    sharing JSONB NOT NULL DEFAULT '[]',
    -- A text excerpt for classification, when the connector can read one.
    text TEXT,
    url TEXT,
    -- Source-specific attributes awzy doesn't interpret yet.
    attributes JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX resources_parent ON resources (parent);
CREATE INDEX resources_owner ON resources (owner);
CREATE INDEX resources_source_kind ON resources (source, kind);

-- Tickets, accounts, projects, statements of work: who is assigned to what, and which resources
-- that work needs. Work-assignment policies are evaluated against these.
CREATE TABLE work_items (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    -- ticket | account | project | statement_of_work | ...
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    -- open | closed
    state TEXT NOT NULL DEFAULT 'open',
    -- People's userNames and service account IDs.
    assignees TEXT[] NOT NULL DEFAULT '{}',
    -- Resource IDs the work needs.
    resources TEXT[] NOT NULL DEFAULT '{}',
    attributes JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX work_items_assignees ON work_items USING GIN (assignees);
CREATE INDEX work_items_resources ON work_items USING GIN (resources);
