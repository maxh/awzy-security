-- The numbers from each heartbeat, for the Install page's charts (ADR 0041). Kept 30 days;
-- recording a heartbeat deletes the install's older samples.
CREATE TABLE heartbeat_samples (
    install_id UUID NOT NULL REFERENCES installs (id),
    received_at TIMESTAMPTZ NOT NULL,
    decisions_per_second DOUBLE PRECISION,
    pdp_p99_ms DOUBLE PRECISION,
    -- 0 to 1.
    error_rate DOUBLE PRECISION,
    kafka_lag BIGINT,
    clickhouse_disk_used_percent DOUBLE PRECISION,
    -- Services that were ready with every desired replica, out of those reported.
    services_healthy INTEGER NOT NULL,
    services_total INTEGER NOT NULL,
    PRIMARY KEY (install_id, received_at)
);
