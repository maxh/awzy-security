-- Every new or changed challenge, by ID, for the client portal's live Challenges page: awzy-
-- challenge follows the channel and streams the challenge to whoever is watching it.
CREATE FUNCTION notify_challenge() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('awzy_challenges', NEW.id::text);
    RETURN NULL;
END
$$;
CREATE TRIGGER challenges_notify AFTER INSERT OR UPDATE ON challenges
    FOR EACH ROW EXECUTE FUNCTION notify_challenge();
