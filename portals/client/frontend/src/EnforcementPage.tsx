import { EmptyState, PageHeader, PageLoader, Section, Stat, StatusBadge } from "@awzy/ui";
import { Alert, Button, Group, Modal, SimpleGrid, Stack, Table, Text } from "@mantine/core";
import { IconHistory, IconShieldCheck } from "@tabler/icons-react";
import { Suspense, useState } from "react";
import { graphql, useLazyLoadQuery, useMutation } from "react-relay";

import type { EnforcementPageQuery } from "./__generated__/EnforcementPageQuery.graphql";
import type { EnforcementPageSetModeMutation } from "./__generated__/EnforcementPageSetModeMutation.graphql";
import { Entity, formatTime, Time } from "./format";

/** Monitor or enforce for the whole install (ADR 0007), and what enforcing would change. */
export function EnforcementPage() {
  const [fetchKey, setFetchKey] = useState(0);
  return (
    <>
      <PageHeader
        title="Enforcement"
        description="Whether decisions are enforced or only recorded, and what enforcing would have denied (ADR 0007)."
      />
      <Suspense fallback={<PageLoader />}>
        <Enforcement fetchKey={fetchKey} onChanged={() => setFetchKey((k) => k + 1)} />
      </Suspense>
    </>
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
          changedBy {
            label
          }
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
          changedBy {
            label
          }
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
          changedBy {
            label
          }
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
      <Section
        title="Mode"
        actions={
          data.canSwitch ? (
            <Button color={enforcing ? "blue" : "red"} onClick={() => setConfirming(true)}>
              {enforcing ? "Switch back to monitor" : "Switch to enforce"}
            </Button>
          ) : undefined
        }
      >
        <Stack gap="xs">
          <Group gap="sm">
            <StatusBadge tone={enforcing ? "danger" : "info"}>{enforcing ? "Enforcing" : "Monitoring"}</StatusBadge>
            <Text size="sm" c="dimmed">
              {current.changedAt
                ? `since ${formatTime(current.changedAt)} by ${current.changedBy?.label ?? "unknown"}`
                : "New installs start in monitor mode and stay there until an admin switches them."}
            </Text>
          </Group>
          <Text size="sm" maw={760}>
            {enforcing
              ? "Enforcement points apply every verdict: denies block, challenges are sent."
              : "awzy decides every request and records the verdict, but enforcement points allow everything and send no challenges. Review what would have been denied before you enforce."}
          </Text>
          {!data.canSwitch && (
            <Text size="xs" c="dimmed">
              You can't switch modes. Policy decides who can, through the PDP.
            </Text>
          )}
        </Stack>
      </Section>

      <div>
        <Text size="sm" c="dimmed" mb="xs">
          Since {formatTime(s.since)}
        </Text>
        <SimpleGrid cols={{ base: 2, md: 5 }}>
          <Stat label="Decisions" value={s.total.toLocaleString()} />
          <Stat
            label="Would deny"
            value={s.wouldDeny.toLocaleString()}
            hint={`${percent(s.wouldDeny, s.monitored)} of monitored`}
          />
          <Stat
            label="Would challenge"
            value={s.wouldChallenge.toLocaleString()}
            hint={`${percent(s.wouldChallenge, s.monitored)} of monitored`}
          />
          <Stat label="Denied" value={s.enforcedDeny.toLocaleString()} hint="enforced" />
          <Stat label="Challenged" value={s.enforcedChallenge.toLocaleString()} hint="enforced" />
        </SimpleGrid>
      </div>

      {!enforcing && (
        <Section
          title="Recent would-deny decisions"
          description="The Decisions page lists all of them: filter by mode Monitored and verdict Deny."
          flush
        >
          {data.wouldDeny.edges.length === 0 ? (
            <EmptyState icon={<IconShieldCheck size={22} />} title="None in the last 24 hours" />
          ) : (
            <Table.ScrollContainer minWidth={720}>
              <Table style={{ whiteSpace: "nowrap" }}>
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>Time</Table.Th>
                    <Table.Th>Subject</Table.Th>
                    <Table.Th>Action</Table.Th>
                    <Table.Th>Resource</Table.Th>
                    <Table.Th>Policies</Table.Th>
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {data.wouldDeny.edges.map(({ node: d }) => (
                    <Table.Tr key={d.id}>
                      <Table.Td>
                        <Time at={d.at} />
                      </Table.Td>
                      <Table.Td>
                        <Entity e={d.subject} />
                      </Table.Td>
                      <Table.Td>{d.action}</Table.Td>
                      <Table.Td>
                        <Entity e={d.resource} />
                      </Table.Td>
                      <Table.Td ff="monospace" fz="xs">
                        {d.policies.join(", ") || "denied by default"}
                      </Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            </Table.ScrollContainer>
          )}
        </Section>
      )}

      <Modal
        opened={confirming}
        onClose={() => setConfirming(false)}
        title={<Text fw={600}>{enforcing ? "Switch back to monitor?" : "Start enforcing?"}</Text>}
      >
        <Stack gap="sm">
          <Text size="sm">
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

      <Section title="History" flush>
        {data.enforcementHistory.length === 0 ? (
          <EmptyState icon={<IconHistory size={22} />} title="Never switched" />
        ) : (
          <Table>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>When</Table.Th>
                <Table.Th>Mode</Table.Th>
                <Table.Th>By</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {data.enforcementHistory.map((c, i) => (
                <Table.Tr key={i}>
                  <Table.Td>{c.changedAt ? formatTime(c.changedAt) : ""}</Table.Td>
                  <Table.Td>
                    <StatusBadge tone={c.mode === "ENFORCE" ? "danger" : "info"}>
                      {c.mode === "ENFORCE" ? "Enforce" : "Monitor"}
                    </StatusBadge>
                  </Table.Td>
                  <Table.Td>{c.changedBy?.label}</Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        )}
      </Section>
    </Stack>
  );
}
