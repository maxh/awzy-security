import { EmptyState, PageHeader, PageLoader, Section, Stat, StatusBadge } from "@awzy/ui";
import { Group, SimpleGrid, Stack, Table, Text } from "@mantine/core";
import { IconBox, IconHistory } from "@tabler/icons-react";
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
    <Suspense fallback={<PageLoader />}>
      <Install id={installId} />
    </Suspense>
  );
}

function num(v: number | null | undefined, unit = "", digits = 1): string {
  return v == null ? "—" : `${v.toFixed(digits)}${unit}`;
}

const back = { label: "Installs", renderRoot: (props: Record<string, unknown>) => <Link to="/installs" {...props} /> };

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
  if (!i || i.client === undefined) {
    return (
      <>
        <PageHeader title="Install not found" back={back} />
        <Text c="dimmed">No install has the ID {id}.</Text>
      </>
    );
  }
  const health = healthOf(i.health ?? "");
  const m = i.metrics;
  const services = i.services ?? [];
  const history = i.versionHistory ?? [];

  return (
    <>
      <PageHeader
        back={back}
        title={
          <>
            {i.client}{" "}
            <Text span inherit c="dimmed" fw={500}>
              / {i.environment}
            </Text>
          </>
        }
        badge={<StatusBadge tone={health.tone}>{health.label}</StatusBadge>}
        description={
          <>
            {i.cloud?.toUpperCase()} {i.region} · {channelLabel(i.channel ?? "", i.pinnedVersion)} channel · last
            heartbeat {ago(i.lastSeenAt)} · registered by {i.createdBy}
            {i.createdAt ? ` on ${formatTime(i.createdAt)}` : ""}
          </>
        }
      />
      <Stack gap="lg">
        <SimpleGrid cols={{ base: 2, md: 3, lg: 6 }}>
          <Stat
            label="Chart"
            value={
              <Text ff="monospace" fz="sm" fw={600}>
                {i.chartVersion ?? "—"}
              </Text>
            }
          />
          <Stat
            label="Decisions/s"
            value={m?.decisionsPerSecond == null ? "—" : m.decisionsPerSecond.toLocaleString()}
          />
          <Stat label="PDP p99" value={num(m?.pdpP99Ms, " ms")} />
          <Stat label="Error rate" value={m?.errorRate == null ? "—" : `${(m.errorRate * 100).toFixed(2)}%`} />
          <Stat label="Kafka lag" value={m?.kafkaLag == null ? "—" : m.kafkaLag.toLocaleString()} />
          <Stat label="ClickHouse disk" value={num(m?.clickhouseDiskUsedPercent, "%", 0)} />
        </SimpleGrid>

        <InstallCharts id={id} />

        <Section title="Deployed services" description="What the install's reporter saw running." flush>
          {services.length === 0 ? (
            <EmptyState icon={<IconBox size={22} />} title="No services reported yet" />
          ) : (
            <Table.ScrollContainer minWidth={720}>
              <Table style={{ whiteSpace: "nowrap" }}>
                <Table.Thead>
                  <Table.Tr>
                    <Table.Th>Service</Table.Th>
                    <Table.Th>Health</Table.Th>
                    <Table.Th>Version</Table.Th>
                    <Table.Th>Image</Table.Th>
                    <Table.Th ta="right">Ready</Table.Th>
                    <Table.Th ta="right">Restarts (24h)</Table.Th>
                  </Table.Tr>
                </Table.Thead>
                <Table.Tbody>
                  {services.map((s) => (
                    <Table.Tr key={s.name}>
                      <Table.Td ff="monospace" fz="sm" fw={500}>
                        {s.name}
                      </Table.Td>
                      <Table.Td>
                        <StatusBadge tone={s.healthy ? "success" : "danger"}>
                          {s.healthy ? "Ready" : "Not ready"}
                        </StatusBadge>
                      </Table.Td>
                      <Table.Td ff="monospace" fz="xs">
                        {s.version ?? "—"}
                      </Table.Td>
                      <Table.Td ff="monospace" fz="xs" c="dimmed">
                        {shortDigest(s.imageDigest) || "—"}
                      </Table.Td>
                      <Table.Td ta="right">
                        {s.replicasReady}/{s.replicasDesired}
                      </Table.Td>
                      <Table.Td ta="right">{s.restarts24h ?? "—"}</Table.Td>
                    </Table.Tr>
                  ))}
                </Table.Tbody>
              </Table>
            </Table.ScrollContainer>
          )}
        </Section>

        <Section title="Version history" description="Each chart version the install reported, newest first." flush>
          {history.length === 0 ? (
            <EmptyState icon={<IconHistory size={22} />} title="No chart version reported yet" />
          ) : (
            <Table>
              <Table.Tbody>
                {history.map((v) => (
                  <Table.Tr key={v.chartVersion}>
                    <Table.Td ff="monospace" fz="sm">
                      {v.chartVersion}
                    </Table.Td>
                    <Table.Td c="dimmed" fz="sm">
                      <Group gap={4}>First seen {formatTime(v.firstSeenAt)}</Group>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          )}
        </Section>
      </Stack>
    </>
  );
}
