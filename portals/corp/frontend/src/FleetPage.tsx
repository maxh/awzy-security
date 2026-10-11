import { EmptyState, oneOf, PageHeader, PageLoader, Section, StatusBadge } from "@awzy/ui";
import {
  ActionIcon,
  Alert,
  Anchor,
  Autocomplete,
  Box,
  Button,
  Code,
  CopyButton,
  Group,
  Modal,
  Select,
  Stack,
  Table,
  Text,
  TextInput,
  Tooltip,
} from "@mantine/core";
import { IconCheck, IconCopy, IconPlus, IconRefresh, IconServer2 } from "@tabler/icons-react";
import { getRouteApi, Link, useRouter } from "@tanstack/react-router";
import { type FormEvent, Suspense, useState } from "react";
import { graphql, type PreloadedQuery, useMutation, usePreloadedQuery } from "react-relay";

import type { FleetPageQuery } from "./__generated__/FleetPageQuery.graphql";
import type { FleetPageRegisterMutation } from "./__generated__/FleetPageRegisterMutation.graphql";
import { ago, CHANNEL_NAMES, channelLabel, CHANNELS, formatTime, healthOf } from "./format";
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
          <Fleet
            queryRef={queryRef}
            onRegister={register}
            registering={registering}
            onRegisterClose={() => setRegistering(false)}
            onRegistered={refresh}
          />
        </Suspense>
      </Section>
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

function Fleet({
  queryRef,
  onRegister,
  registering,
  onRegisterClose,
  onRegistered,
}: {
  queryRef: PreloadedQuery<FleetPageQuery>;
  onRegister: () => void;
  registering: boolean;
  onRegisterClose: () => void;
  onRegistered: () => void;
}) {
  const data = usePreloadedQuery<FleetPageQuery>(
    graphql`
      query FleetPageQuery {
        cloudRegions {
          cloud
          regions
        }
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
  // Rendered here, where the clouds' regions and the installs already registered are loaded.
  const dialog = (
    <RegisterInstall
      opened={registering}
      onClose={onRegisterClose}
      onRegistered={onRegistered}
      cloudRegions={data.cloudRegions}
      registered={installs}
    />
  );
  if (installs.length === 0) {
    return (
      <>
        {dialog}
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
      </>
    );
  }
  return (
    <>
    {dialog}
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
    </>
  );
}

function RegisterInstall({
  opened,
  onClose,
  onRegistered,
  cloudRegions,
  registered,
}: {
  opened: boolean;
  onClose: () => void;
  onRegistered: () => void;
  /** Each cloud's regions, the first its default (from the corp portal, which checks them too). */
  cloudRegions: readonly { readonly cloud: string; readonly regions: readonly string[] }[];
  /** The installs already registered, for the environments each client has. */
  registered: readonly { readonly client: string; readonly environment: string }[];
}) {
  const regionsOf = (c: string | null) => cloudRegions.find((r) => r.cloud === c)?.regions ?? [];
  const [client, setClient] = useState("");
  const [environment, setEnvironment] = useState("prod");
  const [cloud, setCloud] = useState<string | null>("aws");
  const [region, setRegion] = useState<string | null>(regionsOf("aws")[0] ?? null);
  // A client's environments are its own names; suggest the usual ones and those it already has.
  const taken = registered.filter((i) => i.client === client.trim()).map((i) => i.environment);
  const environments = [...new Set(["prod", "staging", "dev", "demo", ...taken])];
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
          region: region ?? "",
          channel: oneOf(CHANNEL_NAMES, channel) ?? "FAST",
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
          <Text size="sm">Install QID</Text>
          <Copyable value={result.id} />
          <Text size="sm">Reporting token</Text>
          <Copyable value={result.token} />
          {result.pullToken && (
            <>
              <Text size="sm">Pull token, for the release registry</Text>
              <Copyable value={result.pullToken} />
              <Text size="sm">
                All three as the JSON the example installs' bootstrap reads (awzy/install-credentials on AWS,
                awzy-install-credentials on Google Cloud)
              </Text>
              <Copyable
                value={JSON.stringify(
                  { install_id: result.id, pull_token: result.pullToken, reporting_token: result.token },
                  null,
                  2,
                )}
              />
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
              <Autocomplete
                label="Environment"
                data={environments}
                value={environment}
                onChange={setEnvironment}
                description={taken.length > 0 ? `Already registered: ${taken.join(", ")}` : undefined}
                error={taken.includes(environment.trim()) ? `${client.trim()} already has ${environment.trim()}` : undefined}
                required
              />
            </Group>
            <Group grow>
              <Select
                label="Cloud"
                data={cloudRegions.map((r) => r.cloud)}
                value={cloud}
                onChange={(c) => {
                  setCloud(c);
                  setRegion(regionsOf(c)[0] ?? null);
                }}
                allowDeselect={false}
              />
              <Select
                label="Region"
                data={[...regionsOf(cloud)]}
                value={region}
                onChange={setRegion}
                searchable
                allowDeselect={false}
                required
              />
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
              Client and environment are lowercase letters, digits, and hyphens. The client's name stays here; installs
              report only their ID.
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

/** A value shown once, with a button that copies it, so nobody has to select it by hand. */
function Copyable({ value }: { value: string }) {
  return (
    <Box pos="relative">
      <Code block pr={44}>
        {value}
      </Code>
      <CopyButton value={value}>
        {({ copied, copy }) => (
          <Tooltip label={copied ? "Copied" : "Copy"} withArrow position="left">
            <ActionIcon
              variant="subtle"
              color={copied ? "teal" : "gray"}
              onClick={copy}
              aria-label="Copy"
              pos="absolute"
              top={6}
              right={6}
            >
              {copied ? <IconCheck size={16} /> : <IconCopy size={16} />}
            </ActionIcon>
          </Tooltip>
        )}
      </CopyButton>
    </Box>
  );
}
