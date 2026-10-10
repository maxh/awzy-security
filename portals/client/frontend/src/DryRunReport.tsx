import { StatusBadge, type Tone } from "@awzy/ui";
import { Alert, Center, Group, Loader, Stack, Table, Text } from "@mantine/core";
import { Suspense } from "react";
import { graphql, useLazyLoadQuery } from "react-relay";

import type { DryRunReportQuery } from "./__generated__/DryRunReportQuery.graphql";

const VERDICT_TONE: Record<string, Tone> = { allow: "success", deny: "danger" };

function VerdictBadge({ verdict }: { verdict: string }) {
  const tone = VERDICT_TONE[verdict] ?? (verdict.startsWith("challenge") ? "warning" : "neutral");
  return <StatusBadge tone={tone}>{verdict}</StatusBadge>;
}

/**
 * What a draft would change (ADR 0049): the last week's requests replayed under the active bundle
 * and the draft, with today's people and resources.
 */
export function DryRunReport({ draftQid }: { draftQid: string }) {
  return (
    <Suspense
      fallback={
        <Center p="xl">
          <Stack align="center" gap="xs">
            <Loader />
            <Text size="sm" c="dimmed">
              Replaying the last 7 days of requests…
            </Text>
          </Stack>
        </Center>
      }
    >
      <Report draftQid={draftQid} />
    </Suspense>
  );
}

function Report({ draftQid }: { draftQid: string }) {
  const { policyDryRun: r } = useLazyLoadQuery<DryRunReportQuery>(
    graphql`
      query DryRunReportQuery($draftQid: ID!) {
        policyDryRun(draftQid: $draftQid) {
          days
          comparedWithVersion
          decisions
          distinctRequests
          replayedRequests
          truncated
          unchangedDecisions
          skippedRequests
          skippedReason
          changes {
            from
            to
            decisions
            requests
            examples {
              subjectId
              onBehalfOfId
              action
              resourceId
              decisions
              policiesBefore
              policiesAfter
            }
          }
        }
      }
    `,
    { draftQid },
    { fetchPolicy: "network-only" },
  );
  const changed = r.changes.reduce((n, c) => n + c.decisions, 0);
  const against = r.comparedWithVersion ? `version ${r.comparedWithVersion}` : "the built-in default policies";

  return (
    <Stack gap="md">
      <Text size="sm">
        Over the last {r.days} days, <b>{r.decisions.toLocaleString()}</b> decisions (
        {r.distinctRequests.toLocaleString()} distinct requests), replayed under {against} and under this draft with
        today&apos;s people and resources.{" "}
        {changed === 0 ? (
          <b>No verdict would change.</b>
        ) : (
          <>
            <b>{changed.toLocaleString()}</b> would come out differently.
          </>
        )}
      </Text>
      {r.truncated && (
        <Alert color="gray" p="xs">
          Only the {r.replayedRequests.toLocaleString()} most frequent distinct requests were replayed.
        </Alert>
      )}
      {r.skippedRequests > 0 && (
        <Alert color="yellow" p="xs" title={`${r.skippedRequests} requests couldn't be replayed`}>
          <Text size="sm">{r.skippedReason}</Text>
        </Alert>
      )}
      {r.changes.map((c) => (
        <Stack key={`${c.from}->${c.to}`} gap={6}>
          <Group gap="xs">
            <VerdictBadge verdict={c.from} />
            <Text size="sm">→</Text>
            <VerdictBadge verdict={c.to} />
            <Text size="sm">
              {c.decisions.toLocaleString()} decisions, {c.requests.toLocaleString()} distinct requests
            </Text>
          </Group>
          <Table fz="sm" withTableBorder highlightOnHover={false}>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>Who</Table.Th>
                <Table.Th>Action</Table.Th>
                <Table.Th>Resource</Table.Th>
                <Table.Th ta="right">Times</Table.Th>
                <Table.Th>Decided by now → with the draft</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {c.examples.map((e, i) => (
                <Table.Tr key={i}>
                  <Table.Td>
                    {e.subjectId}
                    {e.onBehalfOfId ? ` for ${e.onBehalfOfId}` : ""}
                  </Table.Td>
                  <Table.Td>{e.action}</Table.Td>
                  <Table.Td ff="monospace">{e.resourceId}</Table.Td>
                  <Table.Td ta="right">{e.decisions.toLocaleString()}</Table.Td>
                  <Table.Td ff="monospace">
                    {e.policiesBefore.join(", ") || "(none)"} → {e.policiesAfter.join(", ") || "(none)"}
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Stack>
      ))}
      <Text size="xs" c="dimmed">
        Requests are replayed with the people, groups, and resources awzy knows today, so this shows what the policy
        change does, not what happened then.
      </Text>
    </Stack>
  );
}
