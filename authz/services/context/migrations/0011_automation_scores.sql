-- How much a person's requests look like software using their credentials, from awzy-reasoner,
-- beside their risk score (ADR 0085). Null for a service account, or when there was too little to
-- tell.
ALTER TABLE risk_scores ADD COLUMN automation_score integer;
ALTER TABLE risk_scores ADD COLUMN automation_reasons jsonb NOT NULL DEFAULT '[]';
