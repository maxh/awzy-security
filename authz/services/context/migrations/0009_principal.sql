-- Whose risk score or investigation it is: the principal (ADR 0079).
ALTER TABLE risk_scores RENAME COLUMN subject_type TO principal_type; -- awzy: contract (no install yet; ADR 0079)
ALTER TABLE risk_scores RENAME COLUMN subject_id TO principal_id; -- awzy: contract (no install yet; ADR 0079)
ALTER TABLE investigations RENAME COLUMN subject_type TO principal_type; -- awzy: contract (no install yet; ADR 0079)
ALTER TABLE investigations RENAME COLUMN subject_id TO principal_id; -- awzy: contract (no install yet; ADR 0079)
ALTER INDEX investigations_by_subject RENAME TO investigations_by_principal; -- awzy: contract (no install yet; ADR 0079)
