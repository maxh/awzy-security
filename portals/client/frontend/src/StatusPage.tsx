import { PageHeader, PageLoader, Section, StatusBadge } from "@awzy/ui";
import { Alert, Anchor, Button, Code, Group, ScrollArea, Stack, Table, Text } from "@mantine/core";
import { getRouteApi, useRouter } from "@tanstack/react-router";
import { Suspense, useState } from "react";
import {
  fetchQuery,
  graphql,
  type PreloadedQuery,
  useFragment,
  usePreloadedQuery,
  useRelayEnvironment,
} from "react-relay";

import { prettyObject, Time } from "./format";
import { Repairs } from "./Repairs";
import type { StatusPage_heartbeatExport$key } from "./__generated__/StatusPage_heartbeatExport.graphql";
import type { StatusPageDiagnosticsQuery } from "./__generated__/StatusPageDiagnosticsQuery.graphql";
import type { StatusPageQuery } from "./__generated__/StatusPageQuery.graphql";

const route = getRouteApi("/status");

/** Whether each of awzy's services is ready, as the portal finds them: a first look at what's wrong. */
export function StatusPage() {
  const queryRef = route.useLoaderData();
  const router = useRouter();
  return (
    <>
      <PageHeader
        title="Status"
        description="Whether each of awzy's services in your install is ready, and the release it runs, as the portal reaches them."
      />
      <Suspense fallback={<PageLoader />}>
        <Status queryRef={queryRef} onRefresh={() => void router.invalidate()} />
      </Suspense>
    </>
  );
}

function Status({ queryRef, onRefresh }: { queryRef: PreloadedQuery<StatusPageQuery>; onRefresh: () => void }) {
  const data = usePreloadedQuery<StatusPageQuery>(
    graphql`
      query StatusPageQuery {
        canGenerateDiagnostics: viewerCan(action: ADMINISTER, area: ENFORCEMENT)
        installStatus {
          name
          ready
          version
          detail
          runbookUrl
        }
        heartbeatExport {
          ...StatusPage_heartbeatExport
        }
      }
    `,
    queryRef,
  );
  const down = data.installStatus.filter((c) => !c.ready);
  return (
    <>
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
                  {!c.ready && (
                    <Text size="sm" c="dimmed">
                      {c.detail}{" "}
                      <Anchor href={c.runbookUrl} target="_blank" rel="noreferrer" size="sm">
                        What to check
                      </Anchor>
                    </Text>
                  )}
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
        {down.length > 0 && (
          <Group p="md">
            <Text size="sm" c="dimmed">
              A service that stays not ready is usually waiting on another: the PDP on awzy-policy, awzy-context, and
              awzy-challenge, and those on Postgres. Your platform team can see more with kubectl.
            </Text>
          </Group>
        )}
      </Section>
      <HeartbeatExport heartbeatExport={data.heartbeatExport} />
      {data.canGenerateDiagnostics && (
        <Suspense fallback={<PageLoader />}>
          <Repairs />
        </Suspense>
      )}
      {data.canGenerateDiagnostics && <Diagnostics />}
    </>
  );
}

/**
 * What leaves the install for awzy: the heartbeat, whether it's on, and the last one exactly as
 * sent. Its fields are fixed (service health and aggregate metrics), never client data.
 */
function HeartbeatExport({ heartbeatExport }: { heartbeatExport: StatusPage_heartbeatExport$key }) {
  const e = useFragment(
    graphql`
      fragment StatusPage_heartbeatExport on HeartbeatExport {
        enabled
        corpUrl
        intervalSecs
        sentAt
        delivered
        error
        heartbeat
      }
    `,
    heartbeatExport,
  );
  if (!e.enabled) {
    return (
      <Section title="Sent to awzy" description="The heartbeat is the only thing your install sends awzy.">
        <Group gap="xs">
          <StatusBadge tone="neutral">Off</StatusBadge>
          <Text size="sm" c="dimmed">
            Reporting is turned off (reporter.enabled=false in the chart's values), so nothing is sent.
          </Text>
        </Group>
      </Section>
    );
  }
  return (
    <Section
      title="Sent to awzy"
      description={`The heartbeat is the only thing your install sends awzy${e.corpUrl ? `, to ${e.corpUrl}` : ""}${e.intervalSecs ? `, every ${e.intervalSecs} seconds` : ""}. Turn it off with reporter.enabled=false in the chart's values.`}
    >
      <Stack gap="sm">
        <Group gap="xs">
          <StatusBadge tone={e.delivered ? "success" : e.delivered === false || e.error ? "danger" : "neutral"}>
            {e.delivered ? "Delivered" : e.delivered === false ? "Not delivered" : e.error ? "Unknown" : "Not sent yet"}
          </StatusBadge>
          {e.sentAt && (
            <Text size="sm" c="dimmed">
              Last sent <Time at={e.sentAt} />
            </Text>
          )}
        </Group>
        {e.error && (
          <Text size="sm" c="dimmed">
            {e.error}
          </Text>
        )}
        {e.heartbeat && (
          <ScrollArea.Autosize mah={420}>
            <Code block>{prettyObject(e.heartbeat)}</Code>
          </ScrollArea.Autosize>
        )}
      </Stack>
    </Section>
  );
}

const diagnosticsQuery = graphql`
  query StatusPageDiagnosticsQuery {
    diagnosticsBundle
  }
`;

/**
 * A bundle of health and configuration to send awzy support: generated on request, shown in full
 * so the admin sees what it holds, and downloaded exactly as shown.
 */
function Diagnostics() {
  const environment = useRelayEnvironment();
  const [bundle, setBundle] = useState<string | null>(null);
  const [isGenerating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const generate = () => {
    setGenerating(true);
    setError(null);
    fetchQuery<StatusPageDiagnosticsQuery>(environment, diagnosticsQuery, {}, { fetchPolicy: "network-only" })
      .toPromise()
      .then((d) => setBundle(d?.diagnosticsBundle ?? null))
      .catch((e: Error) => setError(e.message))
      .finally(() => setGenerating(false));
  };

  const download = () => {
    if (bundle === null) return;
    const url = URL.createObjectURL(new Blob([bundle], { type: "application/json" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = `awzy-diagnostics-${new Date().toISOString().replace(/[:.]/g, "-")}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <Section
      title="Diagnostics bundle"
      actions={
        bundle === null ? (
          <Button size="xs" variant="default" loading={isGenerating} onClick={generate}>
            Generate
          </Button>
        ) : (
          <Group gap="xs">
            <Button size="xs" variant="default" onClick={() => setBundle(null)}>
              Discard
            </Button>
            <Button size="xs" onClick={download}>
              Download
            </Button>
          </Group>
        )
      }
    >
      <Stack gap="sm">
        <Text size="sm" c="dimmed">
          When awzy support asks for more than this page shows, generate a bundle, read it, and send them the file. It
          holds each service's status, the portal's release, and the names of the settings it runs with. It never holds
          your people, resources, decisions, or policies, or the value of any secret.
        </Text>
        {error && (
          <Alert color="red" variant="light">
            {error}
          </Alert>
        )}
        {bundle !== null && (
          <ScrollArea.Autosize mah={420}>
            <Code block>{bundle}</Code>
          </ScrollArea.Autosize>
        )}
      </Stack>
    </Section>
  );
}
