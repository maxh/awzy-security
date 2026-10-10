import { EmptyState, PageHeader, PageLoader, Section, StatusBadge } from "@awzy/ui";
import { Anchor, Group, Table } from "@mantine/core";
import { IconPackage } from "@tabler/icons-react";
import { Link } from "@tanstack/react-router";
import { Suspense } from "react";
import { graphql, useLazyLoadQuery } from "react-relay";

import type { ReleasesPageQuery } from "./__generated__/ReleasesPageQuery.graphql";
import { channelLabel, healthOf } from "./format";

/** Which chart versions are running, and where: the rollout view. */
export function ReleasesPage() {
  return (
    <>
      <PageHeader title="Releases" description="Which chart versions installs are running, and where." />
      <Section flush>
        <Suspense fallback={<PageLoader />}>
          <Releases />
        </Suspense>
      </Section>
    </>
  );
}
function Releases() {
  const data = useLazyLoadQuery<ReleasesPageQuery>(
    graphql`
      query ReleasesPageQuery {
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
  if (data.releases.length === 0) {
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
        {data.releases.map((r) => (
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
