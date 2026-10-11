-- The policies that asked for each challenge, by ID. A passed challenge sets aside only these, so
-- it never answers a policy that fired after it passed (ADR 0080).
ALTER TABLE challenges ADD COLUMN policies text[] NOT NULL DEFAULT '{}';
