-- What kinds of data awzy-classify found in each resource (ADR 0123), beside its tier: a list of
-- {data_type, by, reason}, empty when it found none. What a connector or app sent stays in
-- `attributes.data_types`; policies see both.
ALTER TABLE resources ADD COLUMN classified_data_types JSONB NOT NULL DEFAULT '[]';

-- Classify everything again, so resources classified before data types existed get theirs.
UPDATE resources SET classified_at = NULL;
