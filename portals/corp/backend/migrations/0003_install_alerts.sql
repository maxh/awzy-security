-- Fleet alerts (ADR 0043): the health each install was last alerted on (or accepted quietly,
-- like its first healthy heartbeat), and a change being held before it's sent.
ALTER TABLE installs
    ADD COLUMN alerted_health TEXT,
    ADD COLUMN held_health TEXT,
    ADD COLUMN held_since TIMESTAMPTZ;
