-- People and groups, as the client's IdP provisions them over SCIM (ADR 0031).

-- A person. user_name is the Human entity ID that policies and tokens use.
CREATE TABLE humans (
    scim_id UUID PRIMARY KEY,
    user_name TEXT NOT NULL,
    external_id TEXT,
    display_name TEXT,
    active BOOLEAN NOT NULL DEFAULT true,
    -- The SCIM resource as the IdP last wrote it (never a password), returned on GET.
    resource JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- A group. display_name is the Group entity ID, e.g. Group::"security".
CREATE TABLE groups (
    scim_id UUID PRIMARY KEY,
    display_name TEXT NOT NULL,
    external_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- SCIM compares userName and displayName case-insensitively (RFC 7643).
CREATE UNIQUE INDEX humans_user_name ON humans (lower(user_name));
CREATE UNIQUE INDEX groups_display_name ON groups (lower(display_name));

-- Members are people or other groups (nested groups).
CREATE TABLE group_members (
    group_id UUID NOT NULL REFERENCES groups (scim_id) ON DELETE CASCADE,
    member_id UUID NOT NULL,
    PRIMARY KEY (group_id, member_id)
);
CREATE INDEX group_members_member ON group_members (member_id);

-- Moves on every change, so PDPs know when to refetch the snapshot.
CREATE TABLE snapshot_version (
    singleton BOOLEAN PRIMARY KEY DEFAULT true CHECK (singleton),
    version BIGINT NOT NULL
);
INSERT INTO snapshot_version (version) VALUES (0);
