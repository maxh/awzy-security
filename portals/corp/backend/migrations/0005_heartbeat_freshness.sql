-- Each install's freshness, for the Install page's charts: seconds since the worst PDP's
-- containment feed, policy sync, and principals sync, and the worst connector's sync, were
-- current; and the calls awzy-gateway made without its PDP since the last heartbeat.
ALTER TABLE heartbeat_samples
    ADD COLUMN containment_feed_age_seconds DOUBLE PRECISION,
    ADD COLUMN policy_sync_age_seconds DOUBLE PRECISION,
    ADD COLUMN principals_sync_age_seconds DOUBLE PRECISION,
    ADD COLUMN connector_sync_age_seconds DOUBLE PRECISION,
    ADD COLUMN gateway_pdp_unreachable BIGINT;
