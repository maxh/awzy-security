import { EmptyState, PageHeader, PageLoader, Section, ShowMore, StatusBadge, type Tone } from "@awzy/ui";
import { Anchor, Group, Table, Text } from "@mantine/core";
import { IconPackage } from "@tabler/icons-react";
import { getRouteApi, Link } from "@tanstack/react-router";
import { Suspense } from "react";
import { graphql, type PreloadedQuery, usePaginationFragment, usePreloadedQuery } from "react-relay";

import type { ReleasesPage_running$key } from "./__generated__/ReleasesPage_running.graphql";
import type { ReleasesPagePaginationQuery } from "./__generated__/ReleasesPagePaginationQuery.graphql";
import type { ReleasesPageQuery } from "./__generated__/ReleasesPageQuery.graphql";
import { channelLabel, formatTime, healthOf } from "./format";

/** What each release channel names, and which chart versions are running where: the rollout view. */
const route = getRouteApi("/releases");

export function ReleasesPage() {
  const queryRef = route.useLoaderData();
  return (
    <>
      <PageHeader
        title="Releases"
        description="The version each channel names, and which chart versions installs are running, and where."
      />
      <Suspense fallback={<PageLoader />}>
        <Releases queryRef={queryRef} />
      </Suspense>
    </>
  );
}

function Releases({ queryRef }: { queryRef: PreloadedQuery<ReleasesPageQuery> }) {
  const data = usePreloadedQuery<ReleasesPageQuery>(
    graphql`
      query ReleasesPageQuery {
        channels {
          channel
          chartVersion
          changedBy
          changedAt
        }
        ...ReleasesPage_running
        installs(first: 200) {
          edges {
            node {
              id
              client
              environment
              channel
              pinnedVersion
              chartVersion
            }
          }
        }
      }
    `,
    queryRef,
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
      <Section title="Rollout" flush>
        <Rollout channels={data.channels} installs={data.installs.edges.map((e) => e.node)} />
      </Section>
      <Section title="Running" flush>
        <Running query={data} />
      </Section>
    </>
  );
}

/** How many of a release's installs it names; the count says how many more there are. */
const INSTALLS_NAMED = 10;

function Running({ query }: { query: ReleasesPage_running$key }) {
  const { data, loadNext, hasNext, isLoadingNext } = usePaginationFragment<
    ReleasesPagePaginationQuery,
    ReleasesPage_running$key
  >(
    graphql`
      fragment ReleasesPage_running on Query
      @argumentDefinitions(first: { type: "Int", defaultValue: 10 }, after: { type: "String" })
      @refetchable(queryName: "ReleasesPagePaginationQuery") {
        releases(first: $first, after: $after) @connection(key: "ReleasesPage_releases") {
          edges {
            node {
              version
              installCount
              installs(first: 10) {
                edges {
                  node {
                    id
                    client
                    environment
                    channel
                    pinnedVersion
                    health
                  }
                }
              }
            }
          }
        }
      }
    `,
    query,
  );
  const releases = data.releases.edges.map((e) => e.node);
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
    <>
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
                  {r.installs.edges.map(({ node: i }) => (
                    <Group key={i.id} gap={6}>
                      <StatusBadge tone={healthOf(i.health).tone}>{healthOf(i.health).label}</StatusBadge>
                      <Anchor
                        size="sm"
                        underline="hover"
                        renderRoot={(props) => (
                          <Link to="/installs/$installId" params={{ installId: i.id }} {...props} />
                        )}
                      >
                        {i.client}/{i.environment}
                      </Anchor>
                      <span style={{ color: "var(--mantine-color-dimmed)", fontSize: 12 }}>
                        {channelLabel(i.channel, i.pinnedVersion)}
                      </span>
                    </Group>
                  ))}
                  {r.installCount > INSTALLS_NAMED && (
                    <Text size="sm" c="dimmed">
                      and {r.installCount - INSTALLS_NAMED} more
                    </Text>
                  )}
                </Group>
              </Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
      <ShowMore hasMore={hasNext} loading={isLoadingNext} onMore={() => loadNext(20)} />
    </>
  );
}

type RolloutInstall = ReleasesPageQuery["response"]["installs"]["edges"][number]["node"];

/** Each install's target, from its pin or its channel, against what it last reported running. */
function Rollout({
  channels,
  installs,
}: {
  channels: ReleasesPageQuery["response"]["channels"];
  installs: ReadonlyArray<RolloutInstall>;
}) {
  if (installs.length === 0) {
    return (
      <Text size="sm" c="dimmed" p="md">
        No installs yet.
      </Text>
    );
  }
  const named = new Map(channels.map((c) => [c.channel as string, c.chartVersion]));
  const rows = installs.map((i) => {
    const target = i.channel === "PINNED" ? i.pinnedVersion : named.get(i.channel);
    const status: { tone: Tone; label: string } = !i.chartVersion
      ? { tone: "neutral", label: "Not reporting" }
      : !target
        ? { tone: "neutral", label: "No target" }
        : i.chartVersion === target
          ? { tone: "success", label: "Up to date" }
          : { tone: "warning", label: "Not yet" };
    return { i, target, status };
  });
  const behind = rows.filter((r) => r.status.label === "Not yet").length;
  return (
    <Table.ScrollContainer minWidth={640}>
      <Table>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>Install</Table.Th>
            <Table.Th>Follows</Table.Th>
            <Table.Th>Target</Table.Th>
            <Table.Th>Running</Table.Th>
            <Table.Th>{behind > 0 ? `${behind} not yet on target` : "Status"}</Table.Th>
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {rows.map(({ i, target, status }) => (
            <Table.Tr key={i.id}>
              <Table.Td>
                <Anchor
                  size="sm"
                  underline="hover"
                  renderRoot={(props) => <Link to="/installs/$installId" params={{ installId: i.id }} {...props} />}
                >
                  {i.client}/{i.environment}
                </Anchor>
              </Table.Td>
              <Table.Td>
                <Text size="sm">{channelLabel(i.channel, i.pinnedVersion)}</Text>
              </Table.Td>
              <Table.Td ff="monospace" fz="sm">
                {target ?? "—"}
              </Table.Td>
              <Table.Td ff="monospace" fz="sm">
                {i.chartVersion ?? "—"}
              </Table.Td>
              <Table.Td>
                <StatusBadge tone={status.tone}>{status.label}</StatusBadge>
              </Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
    </Table.ScrollContainer>
  );
}
