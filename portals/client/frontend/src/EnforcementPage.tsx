import { PageHeader } from "@awzy/ui";
import { Alert, Badge, Button, Center, Group, Loader, Modal, Stack, Table, Text, Title } from "@mantine/core";
import { Suspense, useState } from "react";
import { graphql, useLazyLoadQuery, useMutation } from "react-relay";

import type { EnforcementPageQuery } from "./__generated__/EnforcementPageQuery.graphql";
import type { EnforcementPageSetModeMutation } from "./__generated__/EnforcementPageSetModeMutation.graphql";
import { entityLabel, formatTime } from "./format";

/** Monitor vs. enforce (ADR 0007): what enforcing would change, and the switch. */
export function EnforcementPage() {
  const [fetchKey, setFetchKey] = useState(0);
  return (
    <Stack gap="md">
      <PageHeader
        title="Enforcement"
        description="Whether decisions are enforced or only recorded, and what enforcing would have denied (ADR 0007)."
      />
      <Suspense
        fallback={
          <Center p="xl">
            <Loader />
          </Center>
        }
      >
        <Enforcement fetchKey={fetchKey} onChanged={() => setFetchKey((k) => k + 1)} />
      </Suspense>
    </Stack>
  );
}

function percent(part: number, total: number): string {
  return total === 0 ? "0%" : `${((part / total) * 100).toFixed(1)}%`;
}

function Enforcement({ fetchKey, onChanged }: { fetchKey: number; onChanged: () => void }) {
  const data = useLazyLoadQuery<EnforcementPageQuery>(
    graphql`
      query EnforcementPageQuery {
        canSwitch: viewerCan(action: ADMINISTER, area: ENFORCEMENT)
        enforcement {
          mode
          changedBy
          changedAt
        }
        enforcementSummary {
          since
          total
          monitored
          wouldDeny
          wouldChallenge
          enforcedDeny
          enforcedChallenge
        }
        wouldDeny: decisions(first: 5, filter: { mode: MONITOR, verdict: DENY }) {
          edges {
            node {
              id
              at
              subject {
                type
                id
              }
              action
              resource {
                type
                id
              }
              policies
            }
          }
        }
        enforcementHistory(first: 10) {
          mode
          changedBy
          changedAt
        }
      }
    `,
    {},
    { fetchKey, fetchPolicy: "store-and-network" },
  );
  const [commit, isSaving] = useMutation<EnforcementPageSetModeMutation>(graphql`
    mutation EnforcementPageSetModeMutation($input: SetEnforcementModeInput!) {
      setEnforcementMode(input: $input) {
        enforcement {
          mode
          changedBy
          changedAt
        }
      }
    }
  `);
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const { enforcement: current, enforcementSummary: s } = data;
  const enforcing = current.mode === "ENFORCE";
  const target = enforcing ? "MONITOR" : "ENFORCE";
  const switchMode = () =>
    commit({
      variables: { input: { mode: target } },
      onCompleted: () => {
        setConfirming(false);
        setError(null);
        onChanged();
      },
      onError: (e) => setError(e.message),
    });

  return (
    <Stack gap="lg">
      <Group gap="sm" align="center">
        <Badge size="xl" color={enforcing ? "red" : "blue"} variant="light">
          {enforcing ? "Enforcing" : "Monitoring"}
        </Badge>
        <Text c="dimmed">
          {current.changedAt
            ? `since ${formatTime(current.changedAt)} by ${current.changedBy}`
            : "New installs start in monitor mode and stay there until an admin switches them."}
        </Text>
      </Group>
      <Text>
        {enforcing
          ? "Enforcement points apply every verdict: denies block, challenges are sent."
          : "awzy decides every request and records the verdict, but enforcement points allow everything and send no challenges. Review what would have been denied before you enforce."}
      </Text>

      <Stack gap="xs">
        <Title order={4}>Since {formatTime(s.since)}</Title>
        <Table withTableBorder w="fit-content">
          <Table.Tbody>
            <Table.Tr>
              <Table.Td>Decisions</Table.Td>
              <Table.Td ta="right">{s.total}</Table.Td>
              <Table.Td />
            </Table.Tr>
            <Table.Tr>
              <Table.Td>Would have been denied (monitored)</Table.Td>
              <Table.Td ta="right">{s.wouldDeny}</Table.Td>
              <Table.Td c="dimmed">{percent(s.wouldDeny, s.monitored)} of monitored</Table.Td>
            </Table.Tr>
            <Table.Tr>
              <Table.Td>Would have been challenged (monitored)</Table.Td>
              <Table.Td ta="right">{s.wouldChallenge}</Table.Td>
              <Table.Td c="dimmed">{percent(s.wouldChallenge, s.monitored)} of monitored</Table.Td>
            </Table.Tr>
            <Table.Tr>
              <Table.Td>Denied (enforced)</Table.Td>
              <Table.Td ta="right">{s.enforcedDeny}</Table.Td>
              <Table.Td />
            </Table.Tr>
            <Table.Tr>
              <Table.Td>Challenged (enforced)</Table.Td>
              <Table.Td ta="right">{s.enforcedChallenge}</Table.Td>
              <Table.Td />
            </Table.Tr>
          </Table.Tbody>
        </Table>
      </Stack>

      {!enforcing && (
        <Stack gap="xs">
          <Title order={4}>Recent would-deny decisions</Title>
          {data.wouldDeny.edges.length === 0 ? (
            <Text c="dimmed">None in the last 24 hours.</Text>
          ) : (
            <Table>
              <Table.Tbody>
                {data.wouldDeny.edges.map(({ node: d }) => (
                  <Table.Tr key={d.id}>
                    <Table.Td style={{ whiteSpace: "nowrap" }}>{formatTime(d.at)}</Table.Td>
                    <Table.Td>{entityLabel(d.subject)}</Table.Td>
                    <Table.Td>{d.action}</Table.Td>
                    <Table.Td>{entityLabel(d.resource)}</Table.Td>
                    <Table.Td>
                      <Text size="sm" ff="monospace">
                        {d.policies.join(", ") || "denied by default"}
                      </Text>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          )}
          <Text size="sm" c="dimmed">
            The Decisions page lists all of them: filter by mode Monitored and verdict Deny.
          </Text>
        </Stack>
      )}

      <Group>
        {data.canSwitch ? (
          <Button color={enforcing ? "blue" : "red"} onClick={() => setConfirming(true)}>
            {enforcing ? "Switch back to monitor" : "Switch to enforce"}
          </Button>
        ) : (
          <Text size="sm" c="dimmed">
            You can't switch modes. Policy decides who can, through the PDP.
          </Text>
        )}
      </Group>

      <Modal
        opened={confirming}
        onClose={() => setConfirming(false)}
        title={enforcing ? "Switch back to monitor?" : "Start enforcing?"}
      >
        <Stack gap="sm">
          <Text>
            {enforcing
              ? "Enforcement points will allow every request again and stop sending challenges. Decisions are still recorded."
              : `Enforcement points will start applying verdicts within about 10 seconds. At the last 24 hours' rate, ${percent(
                  s.wouldDeny,
                  s.monitored,
                )} of requests would be denied and ${percent(s.wouldChallenge, s.monitored)} challenged.`}
          </Text>
          {error && (
            <Alert color="red" title="Not switched">
              {error}
            </Alert>
          )}
          <Group justify="flex-end">
            <Button variant="default" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
            <Button color={enforcing ? "blue" : "red"} loading={isSaving} onClick={switchMode}>
              {enforcing ? "Switch to monitor" : "Enforce"}
            </Button>
          </Group>
        </Stack>
      </Modal>

      <Stack gap="xs">
        <Title order={4}>History</Title>
        {data.enforcementHistory.length === 0 ? (
          <Text c="dimmed">Never switched.</Text>
        ) : (
          <Table>
            <Table.Tbody>
              {data.enforcementHistory.map((c, i) => (
                <Table.Tr key={i}>
                  <Table.Td>{c.changedAt ? formatTime(c.changedAt) : ""}</Table.Td>
                  <Table.Td>{c.mode === "ENFORCE" ? "Enforce" : "Monitor"}</Table.Td>
                  <Table.Td>{c.changedBy}</Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        )}
      </Stack>
    </Stack>
  );
}
