-- Automated containments while their scope is in monitor (ADR 0066): recorded, never sent to
-- PDPs. An accessor may have one monitored and one enforced containment active at once.
ALTER TABLE containments ADD COLUMN monitor boolean NOT NULL DEFAULT false;

DROP INDEX containments_one_active;
CREATE UNIQUE INDEX containments_one_active ON containments (subject_type, subject_id, monitor) WHERE lifted_at IS NULL;
