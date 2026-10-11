import { EmptyState, PageHeader, PageLoader, Section, StatusBadge } from "@awzy/ui";
import { Anchor, Group, Table, Text } from "@mantine/core";
import { IconPackage } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import { Suspense } from "react";
import { graphql, useLazyLoadQuery } from "react-relay";

import type { ReleasesPageQuery } from "./__generated__/ReleasesPageQuery.graphql";
import { channelLabel, formatTime, healthOf } from "./format";

/** What each release channel names, and which chart versions are running where: the rollout view. */
export function ReleasesPage() {
  return (
    <>
      <PageHeader
        title="Releases"
        description="The version each channel names, and which chart versions installs are running, and where."
      />
      <Suspense fallback={<PageLoader />}>
        <Releases />
      </Suspense>
    </>
  );
}

function Releases() {
  const data = useLazyLoadQuery<ReleasesPageQuery>(
    graphql`
      query ReleasesPageQuery {
        channels {
          channel
          chartVersion
          changedBy
          changedAt
        }
        releases {
          version
          installCount
          installs {
            id
            client
            environment
            channel
            pinnedVersion
            health
          }
        }
      }
    `,
    {},
    { fetchPolicy: "store-and-network" },
  );
  return (
    <>
      <Section title="Channels" flush>
        {data.channels.length === 0 ? (
          <Text size="sm" c="dimmed" p="md">
            No channel names a version yet. The next publish sets fast; stable is set by a promotion.
          </Text>
        ) : (
          <Table>
            <Table.Thead>
              <Table.Tr>
                <Table.Th w={120}>Channel</Table.Th>
                <Table.Th w={220}>Version</Table.Th>
                <Table.Th>Moved</Table.Th>
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {data.channels.map((c) => (
                <Table.Tr key={c.channel}>
                  <Table.Td>{channelLabel(c.channel, null)}</Table.Td>
                  <Table.Td ff="monospace" fz="sm">
                    {c.chartVersion}
                  </Table.Td>
                  <Table.Td>
                    <Text size="sm" c="dimmed">
                      {formatTime(c.changedAt)} by {c.changedBy}
                    </Text>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        )}
      </Section>
      <Section title="Running" flush>
        <Running releases={data.releases} />
      </Section>
    </>
  );
}

function Running({ releases }: { releases: ReleasesPageQuery["response"]["releases"] }) {
  if (releases.length === 0) {
    return (
      <EmptyState
        icon={<IconPackage size={22} />}
        title="No releases running yet"
        description="A version shows up here once an install reports running it."
      />
    );
  }
  return (
    <Table>
      <Table.Thead>
        <Table.Tr>
          <Table.Th w={220}>Version</Table.Th>
          <Table.Th ta="right" w={100}>
            Installs
          </Table.Th>
          <Table.Th>Running it</Table.Th>
        </Table.Tr>
      </Table.Thead>
      <Table.Tbody>
        {releases.map((r) => (
          <Table.Tr key={r.version}>
            <Table.Td ff="monospace" fz="sm">
              {r.version}
            </Table.Td>
            <Table.Td ta="right">{r.installCount}</Table.Td>
            <Table.Td>
              <Group gap="md">
                {r.installs.map((i) => (
                  <Group key={i.id} gap={6}>
                    <StatusBadge tone={healthOf(i.health).tone}>{healthOf(i.health).label}</StatusBadge>
                    <Anchor
                      size="sm"
                      underline="hover"
                      renderRoot={(props) => <Link to="/installs/$installId" params={{ installId: i.id }} {...props} />}
                    >
                      {i.client}/{i.environment}
                    </Anchor>
                    <span style={{ color: "var(--mantine-color-dimmed)", fontSize: 12 }}>
                      {channelLabel(i.channel, i.pinnedVersion)}
                    </span>
                  </Group>
                ))}
              </Group>
            </Table.Td>
          </Table.Tr>
        ))}
      </Table.Tbody>
    </Table>
  );
}
