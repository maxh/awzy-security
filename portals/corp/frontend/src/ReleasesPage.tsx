import { Badge, Center, Group, Loader, Stack, Table, Text, Title } from "@mantine/core";
import { Link } from "@tanstack/react-router";
import { Suspense } from "react";
import { graphql, useLazyLoadQuery } from "react-relay";

import type { ReleasesPageQuery } from "./__generated__/ReleasesPageQuery.graphql";
import { channelLabel, healthOf } from "./format";

/** Which chart versions are running, and where: the rollout view. */
export function ReleasesPage() {
  return (
    <Stack gap="md">
      <Title order={3}>Releases</Title>
      <Suspense
        fallback={
          <Center p="xl">
            <Loader />
          </Center>
        }
      >
        <Releases />
      </Suspense>
    </Stack>
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
  if (data.releases.length === 0) return <Text c="dimmed">No install has reported a chart version yet.</Text>;
  return (
    <Table>
      <Table.Thead>
        <Table.Tr>
          <Table.Th>Version</Table.Th>
          <Table.Th>Installs</Table.Th>
          <Table.Th>Running it</Table.Th>
        </Table.Tr>
      </Table.Thead>
      <Table.Tbody>
        {data.releases.map((r) => (
          <Table.Tr key={r.version}>
            <Table.Td ff="monospace">{r.version}</Table.Td>
            <Table.Td>{r.installCount}</Table.Td>
            <Table.Td>
              <Group gap="xs">
                {r.installs.map((i) => (
                  <Badge key={i.id} color={healthOf(i.health).color} variant="light" style={{ textTransform: "none" }}>
                    <Link to="/installs/$installId" params={{ installId: i.id }} style={{ color: "inherit" }}>
                      {i.client}/{i.environment} · {channelLabel(i.channel, i.pinnedVersion)}
                    </Link>
                  </Badge>
                ))}
              </Group>
            </Table.Td>
          </Table.Tr>
        ))}
      </Table.Tbody>
    </Table>
  );
}
