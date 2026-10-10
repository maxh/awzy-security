-- The base policies renamed by ADR 0074 under their new IDs in every decision already logged
-- (ADR 0075), so the log reads the same before and after the rename. The PDP logs the new IDs
-- from now on, even when a bundle published before the rename decided. A mutation: ClickHouse
-- rewrites the affected parts in the background.
ALTER TABLE decisions
    UPDATE
        policies = arrayMap(p -> multiIf(
            p = 'people-answer-their-challenges', 'humans-answer-their-challenges',
            p = 'deactivated-people', 'deactivated-humans',
            p = 'crown-jewels-need-a-person-to-approve', 'crown-jewels-need-a-human-to-approve',
            p), policies),
        on_behalf_of_policies = arrayMap(p -> multiIf(
            p = 'people-answer-their-challenges', 'humans-answer-their-challenges',
            p = 'deactivated-people', 'deactivated-humans',
            p = 'crown-jewels-need-a-person-to-approve', 'crown-jewels-need-a-human-to-approve',
            p), on_behalf_of_policies)
    WHERE hasAny(policies, ['people-answer-their-challenges', 'deactivated-people', 'crown-jewels-need-a-person-to-approve'])
       OR hasAny(on_behalf_of_policies, ['people-answer-their-challenges', 'deactivated-people', 'crown-jewels-need-a-person-to-approve']);
