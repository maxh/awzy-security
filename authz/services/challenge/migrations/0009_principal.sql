-- Who a challenge or containment is about is the principal (ADR 0079).
ALTER TABLE challenges RENAME COLUMN subject_type TO principal_type; -- awzy: contract (no install yet; ADR 0079)
ALTER TABLE challenges RENAME COLUMN subject_id TO principal_id; -- awzy: contract (no install yet; ADR 0079)
ALTER TABLE containments RENAME COLUMN subject_type TO principal_type; -- awzy: contract (no install yet; ADR 0079)
ALTER TABLE containments RENAME COLUMN subject_id TO principal_id; -- awzy: contract (no install yet; ADR 0079)
