import { EmptyState, PageHeader, PageLoader, Section, StatusBadge, type Tone } from "@awzy/ui";
import { Button, SimpleGrid, Stack, Table, Text } from "@mantine/core";
import { IconPlugConnected } from "@tabler/icons-react";
import { Suspense, useState } from "react";
import { graphql, useLazyLoadQuery } from "react-relay";

import type { ConnectorsPageQuery } from "./__generated__/ConnectorsPageQuery.graphql";
import { Time } from "./format";

const LABELS: Record<string, string> = { drive: "Google Drive", jira: "Jira" };

/** The systems of record awzy reads, and whether each is syncing. */
export function ConnectorsPage() {
  const [fetchKey, setFetchKey] = useState(0);
  return (
    <>
      <PageHeader
        title="Connectors"
        description="The systems awzy reads resources, sharing, and work from, read-only, and whether each is syncing."
        actions={
          <Button size="xs" variant="default" onClick={() => setFetchKey((k) => k + 1)}>
            Check again
          </Button>
        }
      />
      <Suspense fallback={<PageLoader />}>
        <Connectors fetchKey={fetchKey} />
      </Suspense>
    </>
  );
}

type ConnectorRow = ConnectorsPageQuery["response"]["connectors"][number];

function health(c: ConnectorRow): { tone: Tone; label: string } {
  if (c.lastError) return { tone: "danger", label: "Failing" };
  if (!c.lastSuccessAt) return { tone: "neutral", label: "First sync running" };
  // Two missed rounds is stale: the sync is stuck rather than between rounds.
  const age = (Date.now() - Date.parse(c.lastSuccessAt)) / 1000;
  return age > 2 * c.intervalSecs + 60 ? { tone: "warning", label: "Stale" } : { tone: "success", label: "Syncing" };
}

function Connectors({ fetchKey }: { fetchKey: number }) {
  const { connectors } = useLazyLoadQuery<ConnectorsPageQuery>(
    graphql`
      query ConnectorsPageQuery {
        connectors {
          name
          settings {
            name
            value
          }
          intervalSecs
          lastSuccessAt
          lastFailureAt
          lastError
          failures
          written
          deleted
        }
      }
    `,
    {},
    { fetchKey, fetchPolicy: "network-only" },
  );
  if (connectors.length === 0) {
    return (
      <EmptyState
        icon={<IconPlugConnected size={22} />}
        title="No connectors configured"
        description="Google Drive and Jira are set up in the chart's values (connectors.drive and connectors.jira), with their credentials in Secrets."
      />
    );
  }
  return (
    <SimpleGrid cols={{ base: 1, lg: 2 }}>
      {connectors.map((c) => {
        const h = health(c);
        return (
          <Section
            key={c.name}
            title={LABELS[c.name] ?? c.name}
            description={`Syncs every ${Math.round(c.intervalSecs / 60)} minutes.`}
            actions={<StatusBadge tone={h.tone}>{h.label}</StatusBadge>}
          >
            <Stack gap="sm">
              <Table>
                <Table.Tbody>
                  {c.settings.map((s) => (
                    <Table.Tr key={s.name}>
                      <Table.Td w="45%">
                        <Text size="sm" c="dimmed">
                          {s.name}
                        </Text>
                      </Table.Td>
                      <Table.Td>
                        <Text size="sm" style={{ wordBreak: "break-word" }}>
                          {s.value}
                        </Text>
                      </Table.Td>
                    </Table.Tr>
                  ))}
                  <Table.Tr>
                    <Table.Td>
                      <Text size="sm" c="dimmed">
                        Last synced
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      <Text size="sm">{c.lastSuccessAt ? <Time at={c.lastSuccessAt} /> : "not yet"}</Text>
                    </Table.Td>
                  </Table.Tr>
                  <Table.Tr>
                    <Table.Td>
                      <Text size="sm" c="dimmed">
                        Since it started
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      <Text size="sm">
                        {c.written} written, {c.deleted} deleted, {c.failures} failed rounds
                      </Text>
                    </Table.Td>
                  </Table.Tr>
                </Table.Tbody>
              </Table>
              {c.lastError && (
                <Text size="sm" c="red">
                  {c.lastFailureAt && (
                    <>
                      <Time at={c.lastFailureAt} />:{" "}
                    </>
                  )}
                  {c.lastError}
                </Text>
              )}
            </Stack>
          </Section>
        );
      })}
    </SimpleGrid>
  );
}
