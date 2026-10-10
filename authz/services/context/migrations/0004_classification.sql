-- What awzy-classify decided for each resource (demo plan step 4, ADR 0047). `sensitivity` stays
-- what the connector sent; policies see the higher of the two.
ALTER TABLE resources
    -- public | internal | confidential | crown_jewels, or NULL when nothing placed it.
    ADD COLUMN classified_sensitivity TEXT,
    -- label | content | metadata (| model, later): what decided it.
    ADD COLUMN classified_by TEXT,
    -- One line on why, for the client portal and audits.
    ADD COLUMN classification_reason TEXT,
    -- When it was classified. Older than updated_at means the resource changed since.
    ADD COLUMN classified_at TIMESTAMPTZ;

-- The classifier's queue: resources never classified, or changed since.
CREATE INDEX resources_to_classify ON resources (updated_at)
    WHERE classified_at IS NULL OR classified_at < updated_at;
