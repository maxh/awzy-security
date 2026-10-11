import { Section, StatusBadge, type Tone } from "@awzy/ui";
import { Alert, Button, Group, Modal, Stack, Table, Text } from "@mantine/core";
import { useState } from "react";
import { graphql, useLazyLoadQuery, useMutation } from "react-relay";

import type { RepairKind, RepairsQuery } from "./__generated__/RepairsQuery.graphql";
import type { RepairsStartMutation } from "./__generated__/RepairsStartMutation.graphql";
import { formatTime } from "./format";

const KINDS: Record<RepairKind, { title: string; what: string }> = {
  CLICKHOUSE_DISK_CLEANUP: {
    title: "Free ClickHouse disk",
    what: "Drops parts ClickHouse set aside as broken, empties its own query and metric logs, and applies your retention to decisions and events now. Loses none of awzy's data.",
  },
  "%future added value": { title: "Repair", what: "" },
};

const STATUS: Record<string, { label: string; tone: Tone }> = {
  RUNNING: { label: "Running", tone: "info" },
  SUCCEEDED: { label: "Done", tone: "success" },
  FAILED: { label: "Failed", tone: "danger" },
};

/**
 * Repairs an admin can start (ADR 0102), each a Job your chart defines, and recent runs. For when
 * something's wrong and there's no one with kubectl at hand.
 */
export function Repairs() {
  const [fetchKey, setFetchKey] = useState(0);
  const data = useLazyLoadQuery<RepairsQuery>(
    graphql`
      query RepairsQuery {
        repairKinds
        repairRuns {
          name
          kind
          status
          startedBy
          startedAt
          finishedAt
        }
      }
    `,
    {},
    { fetchKey, fetchPolicy: "network-only" },
  );
  const [confirming, setConfirming] = useState<RepairKind | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [start, isStarting] = useMutation<RepairsStartMutation>(graphql`
    mutation RepairsStartMutation($kind: RepairKind!) {
      startRepair(kind: $kind) {
        name
      }
    }
  `);
  if (data.repairKinds.length === 0) return null;
  const running = (k: RepairKind) => data.repairRuns.some((r) => r.kind === k && r.status === "RUNNING");
  const go = (kind: RepairKind) =>
    start({
      variables: { kind },
      onCompleted: () => {
        setError(null);
        setConfirming(null);
        setFetchKey((n) => n + 1);
      },
      onError: (e) => setError(e.message),
    });
  return (
    <Section
      title="Repairs"
      description="Fixes you can start yourself, each run as a Job in your cluster that your platform team can follow with kubectl."
      actions={
        <Button size="xs" variant="default" onClick={() => setFetchKey((n) => n + 1)}>
          Check again
        </Button>
      }
    >
      <Stack gap="md">
        {data.repairKinds.map((k) => (
          <Group key={k} justify="space-between" align="flex-start" wrap="nowrap">
            <div>
              <Text fw={500}>{KINDS[k].title}</Text>
              <Text size="sm" c="dimmed">
                {KINDS[k].what}
              </Text>
            </div>
            <Button variant="default" disabled={running(k)} onClick={() => setConfirming(k)}>
              {running(k) ? "Running" : "Start"}
            </Button>
          </Group>
        ))}
        {data.repairRuns.length > 0 && (
          <Table.ScrollContainer minWidth={560}>
            <Table>
              <Table.Thead>
                <Table.Tr>
                  <Table.Th>Repair</Table.Th>
                  <Table.Th>Status</Table.Th>
                  <Table.Th>Started</Table.Th>
                  <Table.Th>Job</Table.Th>
                </Table.Tr>
              </Table.Thead>
              <Table.Tbody>
                {data.repairRuns.map((r) => (
                  <Table.Tr key={r.name}>
                    <Table.Td>{KINDS[r.kind].title}</Table.Td>
                    <Table.Td>
                      <StatusBadge tone={STATUS[r.status]?.tone ?? "neutral"}>
                        {STATUS[r.status]?.label ?? r.status}
                      </StatusBadge>
                    </Table.Td>
                    <Table.Td>
                      <Text size="sm">
                        {r.startedAt ? formatTime(r.startedAt) : ""}
                        {r.startedBy ? ` by ${r.startedBy}` : ""}
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      <Text size="sm" ff="monospace" c="dimmed">
                        {r.name}
                      </Text>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </Table.ScrollContainer>
        )}
      </Stack>
      <Modal
        opened={confirming !== null}
        onClose={() => setConfirming(null)}
        title={<Text fw={600}>{confirming ? KINDS[confirming].title : ""}?</Text>}
      >
        <Stack gap="sm">
          <Text size="sm">{confirming ? KINDS[confirming].what : ""}</Text>
          {error && (
            <Alert color="red" title="Not started">
              {error}
            </Alert>
          )}
          <Group justify="flex-end">
            <Button variant="default" onClick={() => setConfirming(null)}>
              Cancel
            </Button>
            <Button loading={isStarting} onClick={() => confirming && go(confirming)}>
              Start
            </Button>
          </Group>
        </Stack>
      </Modal>
    </Section>
  );
}
