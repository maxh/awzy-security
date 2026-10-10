import { Anchor, Badge, Card, Center, Group, Loader, SimpleGrid, Stack, Table, Text, Title } from "@mantine/core";
import { Link, useParams } from "@tanstack/react-router";
import { Suspense } from "react";
import { graphql, useLazyLoadQuery } from "react-relay";

import type { InstallPageQuery } from "./__generated__/InstallPageQuery.graphql";
import { ago, channelLabel, formatTime, healthOf, shortDigest } from "./format";
import { InstallCharts } from "./InstallCharts";

/** One install: the deployed units inside it, its numbers, and its upgrade history. */
export function InstallPage() {
  const { installId } = useParams({ from: "/installs/$installId" });
  return (
    <Stack gap="md">
      <Anchor component={Link} to="/installs" size="sm">
        ← All installs
      </Anchor>
      <Suspense
        fallback={
          <Center p="xl">
            <Loader />
          </Center>
        }
      >
        <Install id={installId} />
      </Suspense>
    </Stack>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <Card withBorder padding="sm">
      <Text size="xs" c="dimmed">
        {label}
      </Text>
      <Text fw={600}>{value}</Text>
    </Card>
  );
}

function num(v: number | null | undefined, unit = "", digits = 1): string {
  return v == null ? "—" : `${v.toFixed(digits)}${unit}`;
}

function Install({ id }: { id: string }) {
  const data = useLazyLoadQuery<InstallPageQuery>(
    graphql`
      query InstallPageQuery($id: ID!) {
        node(id: $id) {
          ... on Install {
            client
            environment
            cloud
            region
            channel
            pinnedVersion
            health
            chartVersion
            lastSeenAt
            createdBy
            createdAt
            services {
              name
              version
              imageDigest
              replicasReady
              replicasDesired
              restarts24h
              healthy
            }
            metrics {
              decisionsPerSecond
              pdpP99Ms
              errorRate
              kafkaLag
              clickhouseDiskUsedPercent
            }
            versionHistory {
              chartVersion
              firstSeenAt
            }
          }
        }
      }
    `,
    { id },
    { fetchPolicy: "store-and-network" },
  );
  const i = data.node;
  if (!i || i.client === undefined) return <Text>No install with ID {id}.</Text>;
  const health = healthOf(i.health ?? "");
  const m = i.metrics;

  return (
    <Stack gap="lg">
      <Group gap="sm">
        <Title order={3}>
          {i.client} / {i.environment}
        </Title>
        <Badge color={health.color} variant="light">
          {health.label}
        </Badge>
      </Group>
      <Text size="sm" c="dimmed">
        {i.cloud} {i.region} · {channelLabel(i.channel ?? "", i.pinnedVersion)} channel · chart{" "}
        {i.chartVersion ?? "unknown"} · last heartbeat {ago(i.lastSeenAt)} · registered by {i.createdBy}{" "}
        {i.createdAt ? formatTime(i.createdAt) : ""}
      </Text>

      <SimpleGrid cols={{ base: 2, md: 5 }}>
        <Metric
          label="Decisions/s"
          value={m?.decisionsPerSecond == null ? "—" : m.decisionsPerSecond.toLocaleString()}
        />
        <Metric label="PDP p99" value={num(m?.pdpP99Ms, " ms")} />
        <Metric label="Error rate" value={m?.errorRate == null ? "—" : `${(m.errorRate * 100).toFixed(2)}%`} />
        <Metric label="Kafka lag" value={m?.kafkaLag == null ? "—" : m.kafkaLag.toLocaleString()} />
        <Metric label="ClickHouse disk" value={num(m?.clickhouseDiskUsedPercent, "%", 0)} />
      </SimpleGrid>

      <InstallCharts id={id} />

      <Stack gap="xs">
        <Title order={4}>Deployed services</Title>
        {(i.services ?? []).length === 0 ? (
          <Text c="dimmed">This install hasn't reported its services yet.</Text>
        ) : (
          <Table>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>Service</Table.Th>
                <Table.Th>Version</Table.Th>
                <Table.Th>Image</Table.Th>
                <Table.Th>Ready</Table.Th>
                <Table.Th>Restarts (24h)</Table.Th>
                <Table.Th>Health</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {(i.services ?? []).map((s) => (
                <Table.Tr key={s.name}>
                  <Table.Td ff="monospace">{s.name}</Table.Td>
                  <Table.Td>{s.version ?? "—"}</Table.Td>
                  <Table.Td ff="monospace">{shortDigest(s.imageDigest) || "—"}</Table.Td>
                  <Table.Td>
                    {s.replicasReady}/{s.replicasDesired}
                  </Table.Td>
                  <Table.Td>{s.restarts24h ?? "—"}</Table.Td>
                  <Table.Td>
                    <Badge color={s.healthy ? "teal" : "red"} variant="light">
                      {s.healthy ? "ready" : "not ready"}
                    </Badge>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        )}
      </Stack>

      <Stack gap="xs">
        <Title order={4}>Version history</Title>
        {(i.versionHistory ?? []).length === 0 ? (
          <Text c="dimmed">No chart version reported yet.</Text>
        ) : (
          <Table>
            <Table.Tbody>
              {(i.versionHistory ?? []).map((v) => (
                <Table.Tr key={v.chartVersion}>
                  <Table.Td>{v.chartVersion}</Table.Td>
                  <Table.Td>first seen {formatTime(v.firstSeenAt)}</Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        )}
      </Stack>
    </Stack>
  );
}
