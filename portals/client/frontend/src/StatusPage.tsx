import { PageHeader, PageLoader, Section, StatusBadge } from "@awzy/ui";
import { Button, Group, Table, Text } from "@mantine/core";
import { Suspense, useState } from "react";
import { graphql, useLazyLoadQuery } from "react-relay";

import type { StatusPageQuery } from "./__generated__/StatusPageQuery.graphql";

/** Whether each of awzy's services is ready, as the portal finds them: a first look at what's wrong. */
export function StatusPage() {
  const [fetchKey, setFetchKey] = useState(0);
  return (
    <>
      <PageHeader
        title="Status"
        description="Whether each of awzy's services in your install is ready, and the release it runs, as the portal reaches them."
      />
      <Suspense fallback={<PageLoader />}>
        <Status fetchKey={fetchKey} onRefresh={() => setFetchKey((k) => k + 1)} />
      </Suspense>
    </>
  );
}

function Status({ fetchKey, onRefresh }: { fetchKey: number; onRefresh: () => void }) {
  const data = useLazyLoadQuery<StatusPageQuery>(
    graphql`
      query StatusPageQuery {
        installStatus {
          name
          ready
          version
          detail
        }
      }
    `,
    {},
    { fetchKey, fetchPolicy: "network-only" },
  );
  const down = data.installStatus.filter((c) => !c.ready);
  return (
    <Section
      title={down.length === 0 ? "Everything is ready" : `${down.length} not ready`}
      actions={
        <Button size="xs" variant="default" onClick={onRefresh}>
          Check again
        </Button>
      }
      flush
    >
      <Table>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>Service</Table.Th>
            <Table.Th>Status</Table.Th>
            <Table.Th>Release</Table.Th>
            <Table.Th>Detail</Table.Th>
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {data.installStatus.map((c) => (
            <Table.Tr key={c.name}>
              <Table.Td>
                <Text size="sm" ff="monospace">
                  {c.name}
                </Text>
              </Table.Td>
              <Table.Td>
                <StatusBadge tone={c.ready ? "success" : "danger"}>{c.ready ? "Ready" : "Not ready"}</StatusBadge>
              </Table.Td>
              <Table.Td>
                <Text size="sm" c="dimmed" ff="monospace">
                  {c.version ?? "—"}
                </Text>
              </Table.Td>
              <Table.Td>
                <Text size="sm" c="dimmed">
                  {c.detail ?? ""}
                </Text>
              </Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
      {down.length > 0 && (
        <Group p="md">
          <Text size="sm" c="dimmed">
            A service that stays not ready is usually waiting on another: the PDP on awzy-policy and awzy-context,
            awzy-challenge and awzy-context on Postgres. Your platform team can see more with kubectl.
          </Text>
        </Group>
      )}
    </Section>
  );
}
