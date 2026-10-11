import { EmptyState, PageHeader, PageLoader, Section, StatusBadge } from "@awzy/ui";
import {
  Alert,
  Anchor,
  Button,
  Code,
  Group,
  Modal,
  Select,
  Stack,
  Table,
  Text,
  TextInput,
  Tooltip,
} from "@mantine/core";
import { IconPlus, IconRefresh, IconServer2 } from "@tabler/icons-react";
import { getRouteApi, Link, useRouter } from "@tanstack/react-router";
import { type FormEvent, Suspense, useState } from "react";
import { graphql, type PreloadedQuery, useMutation, usePreloadedQuery } from "react-relay";

import type { FleetPageQuery } from "./__generated__/FleetPageQuery.graphql";
import type { FleetPageRegisterMutation, ReleaseChannel } from "./__generated__/FleetPageRegisterMutation.graphql";
import { ago, CHANNELS, channelLabel, formatTime, healthOf } from "./format";
import { McpTokenButton } from "./McpTokenButton";

/** Every registered install, with its health and what it last reported (ADR 0026). */
const route = getRouteApi("/installs");

export function FleetPage() {
  const queryRef = route.useLoaderData();
  const router = useRouter();
  const refresh = () => void router.invalidate();
  const [registering, setRegistering] = useState(false);
  const register = () => setRegistering(true);
  return (
    <>
      <PageHeader
        title="Installs"
        description="Every registered install, with the health and numbers its last heartbeat reported."
        actions={
          <>
            <McpTokenButton />
            <Button variant="default" leftSection={<IconRefresh size={16} />} onClick={refresh}>
              Refresh
            </Button>
            <Button leftSection={<IconPlus size={16} />} onClick={register}>
              Register install
            </Button>
          </>
        }
      />
      <Section flush>
        <Suspense fallback={<PageLoader />}>
          <Fleet queryRef={queryRef} onRegister={register} />
        </Suspense>
      </Section>
      <RegisterInstall opened={registering} onClose={() => setRegistering(false)} onRegistered={refresh} />
    </>
  );
}

/** A figure, or a dimmed dash when the install hasn't reported it. */
function Figure({ children }: { children: string | null }) {
  return children == null ? (
    <Text c="dimmed" span inherit>
      —
    </Text>
  ) : (
    <Text span inherit style={{ fontVariantNumeric: "tabular-nums" }}>
      {children}
    </Text>
  );
}

function Fleet({ queryRef, onRegister }: { queryRef: PreloadedQuery<FleetPageQuery>; onRegister: () => void }) {
  const data = usePreloadedQuery<FleetPageQuery>(
    graphql`
      query FleetPageQuery {
        installs(first: 200) {
          edges {
            node {
              id
              client
              environment
              cloud
              region
              channel
              pinnedVersion
              health
              chartVersion
              lastSeenAt
              services {
                healthy
              }
              metrics {
                decisionsPerSecond
                pdpP99Ms
              }
            }
          }
        }
      }
    `,
    queryRef,
  );
  const installs = data.installs.edges.map((e) => e.node);
  if (installs.length === 0) {
    return (
      <EmptyState
        icon={<IconServer2 size={22} />}
        title="No installs yet"
        description="Register an install to get its ID and reporting token, then put both in the install's reporter settings."
        action={
          <Button leftSection={<IconPlus size={16} />} onClick={onRegister}>
            Register install
          </Button>
        }
      />
    );
  }
  return (
    <Table.ScrollContainer minWidth={900}>
      <Table style={{ whiteSpace: "nowrap" }}>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>Install</Table.Th>
            <Table.Th>Health</Table.Th>
            <Table.Th>Cloud</Table.Th>
            <Table.Th>Channel</Table.Th>
            <Table.Th>Version</Table.Th>
            <Table.Th>Services</Table.Th>
            <Table.Th ta="right">PDP p99</Table.Th>
            <Table.Th ta="right">Decisions/s</Table.Th>
            <Table.Th>Last seen</Table.Th>
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {installs.map((i) => {
            const health = healthOf(i.health);
            const healthy = i.services.filter((s) => s.healthy).length;
            return (
              <Table.Tr key={i.id}>
                <Table.Td>
                  <Anchor
                    fw={600}
                    underline="hover"
                    renderRoot={(props) => <Link to="/installs/$installId" params={{ installId: i.id }} {...props} />}
                  >
                    {i.client}
                  </Anchor>
                  <Text size="xs" c="dimmed">
                    {i.environment}
                  </Text>
                </Table.Td>
                <Table.Td>
                  <StatusBadge tone={health.tone}>{health.label}</StatusBadge>
                </Table.Td>
                <Table.Td>
                  <Text size="sm">{i.cloud.toUpperCase()}</Text>
                  <Text size="xs" c="dimmed">
                    {i.region}
                  </Text>
                </Table.Td>
                <Table.Td>{channelLabel(i.channel, i.pinnedVersion)}</Table.Td>
                <Table.Td ff="monospace" fz="xs">
                  <Figure>{i.chartVersion ?? null}</Figure>
                </Table.Td>
                <Table.Td>
                  <Figure>{i.services.length > 0 ? `${healthy}/${i.services.length} ready` : null}</Figure>
                </Table.Td>
                <Table.Td ta="right">
                  <Figure>{i.metrics?.pdpP99Ms != null ? `${i.metrics.pdpP99Ms.toFixed(1)} ms` : null}</Figure>
                </Table.Td>
                <Table.Td ta="right">
                  <Figure>
                    {i.metrics?.decisionsPerSecond != null ? i.metrics.decisionsPerSecond.toLocaleString() : null}
                  </Figure>
                </Table.Td>
                <Table.Td>
                  {i.lastSeenAt ? (
                    <Tooltip label={formatTime(i.lastSeenAt)}>
                      <Text size="sm" span>
                        {ago(i.lastSeenAt)}
                      </Text>
                    </Tooltip>
                  ) : (
                    <Text size="sm" c="dimmed" span>
                      Never
                    </Text>
                  )}
                </Table.Td>
              </Table.Tr>
            );
          })}
        </Table.Tbody>
      </Table>
    </Table.ScrollContainer>
  );
}

function RegisterInstall({
  opened,
  onClose,
  onRegistered,
}: {
  opened: boolean;
  onClose: () => void;
  onRegistered: () => void;
}) {
  const [client, setClient] = useState("");
  const [environment, setEnvironment] = useState("prod");
  const [cloud, setCloud] = useState<string | null>("aws");
  const [region, setRegion] = useState("us-east-2");
  const [channel, setChannel] = useState<string | null>("FAST");
  const [pinned, setPinned] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<{ id: string; token: string; pullToken: string | null } | null>(null);
  const [commit, isSaving] = useMutation<FleetPageRegisterMutation>(graphql`
    mutation FleetPageRegisterMutation($input: RegisterInstallInput!) {
      registerInstall(input: $input) {
        install {
          id
        }
        reportingToken
        pullToken
      }
    }
  `);

  const close = () => {
    setResult(null);
    setError(null);
    onClose();
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    commit({
      variables: {
        input: {
          client: client.trim(),
          environment: environment.trim(),
          cloud: cloud ?? "aws",
          region: region.trim(),
          channel: (channel ?? "FAST") as ReleaseChannel,
          pinnedVersion: channel === "PINNED" ? pinned.trim() : null,
        },
      },
      onCompleted: (res) => {
        setError(null);
        setResult({
          id: res.registerInstall.install.id,
          token: res.registerInstall.reportingToken,
          pullToken: res.registerInstall.pullToken ?? null,
        });
        onRegistered();
      },
      onError: (err) => setError(err.message),
    });
  };

  return (
    <Modal opened={opened} onClose={close} title={<Text fw={600}>Register an install</Text>} size="lg">
      {result ? (
        <Stack>
          <Alert color="yellow" title="Copy the tokens now">
            They're shown only this once. Store them where the install's bootstrap reads them, in the client's own
            secret store: never in a repository or a chat.
          </Alert>
          <Text size="sm">Install ID</Text>
          <Code block>{result.id}</Code>
          <Text size="sm">Reporting token</Text>
          <Code block>{result.token}</Code>
          {result.pullToken && (
            <>
              <Text size="sm">Pull token, for the release registry</Text>
              <Code block>{result.pullToken}</Code>
              <Text size="sm">All three as the JSON the example bootstrap reads (awzy/install-credentials)</Text>
              <Code block>
                {JSON.stringify(
                  { install_id: result.id, pull_token: result.pullToken, reporting_token: result.token },
                  null,
                  2,
                )}
              </Code>
            </>
          )}
          <Group justify="flex-end">
            <Button onClick={close}>Done</Button>
          </Group>
        </Stack>
      ) : (
        <form onSubmit={submit}>
          <Stack>
            {error && <Alert color="red">{error}</Alert>}
            <Group grow>
              <TextInput
                label="Client"
                placeholder="acme"
                value={client}
                onChange={(e) => setClient(e.currentTarget.value)}
                required
              />
              <TextInput
                label="Environment"
                value={environment}
                onChange={(e) => setEnvironment(e.currentTarget.value)}
                required
              />
            </Group>
            <Group grow>
              <Select
                label="Cloud"
                data={["aws", "gcp", "azure"]}
                value={cloud}
                onChange={setCloud}
                allowDeselect={false}
              />
              <TextInput label="Region" value={region} onChange={(e) => setRegion(e.currentTarget.value)} required />
            </Group>
            <Group grow>
              <Select
                label="Release channel"
                data={CHANNELS}
                value={channel}
                onChange={setChannel}
                allowDeselect={false}
              />
              {channel === "PINNED" && (
                <TextInput
                  label="Pinned version"
                  placeholder="0.4.2-g0123456789ab"
                  value={pinned}
                  onChange={(e) => setPinned(e.currentTarget.value)}
                  required
                />
              )}
            </Group>
            <Text size="xs" c="dimmed">
              Client, environment, and region are lowercase letters, digits, and hyphens. The client's name stays here;
              installs report only their ID.
            </Text>
            <Group justify="flex-end">
              <Button type="submit" loading={isSaving}>
                Register
              </Button>
            </Group>
          </Stack>
        </form>
      )}
    </Modal>
  );
}
