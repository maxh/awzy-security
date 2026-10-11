import { PageHeader, PageLoader, Section, StatusBadge, type Tone } from "@awzy/ui";
import {
  Alert,
  Button,
  Checkbox,
  Group,
  Modal,
  NumberInput,
  PasswordInput,
  SimpleGrid,
  Stack,
  Switch,
  Table,
  Text,
  Textarea,
  TextInput,
} from "@mantine/core";
import { getRouteApi, useRouter } from "@tanstack/react-router";
import { Suspense, useState } from "react";
import { graphql, type PreloadedQuery, usePreloadedQuery, useRelayEnvironment } from "react-relay";

import type {
  ConnectorName,
  ConnectorsPageConfigureMutation,
} from "./__generated__/ConnectorsPageConfigureMutation.graphql";
import type { ConnectorsPageQuery } from "./__generated__/ConnectorsPageQuery.graphql";
import { formatTime, Time } from "./format";
import { mutate, passChallenge } from "./securityKeyChallenge";
import { webAuthnMessage } from "./webauthn";

const CONNECTORS: { name: ConnectorName; key: string; title: string; credential: string }[] = [
  { name: "DRIVE", key: "drive", title: "Google Drive", credential: "Service account key (JSON)" },
  { name: "JIRA", key: "jira", title: "Jira", credential: "API token" },
];

const configure = graphql`
  mutation ConnectorsPageConfigureMutation($input: ConfigureConnectorInput!) {
    configureConnector(input: $input) {
      connector {
        name
      }
      challengeQid
    }
  }
`;

const route = getRouteApi("/connectors");

/** The systems of record awzy reads, how each is configured, and whether each is syncing (ADR 0104). */
export function ConnectorsPage() {
  const queryRef = route.useLoaderData();
  const router = useRouter();
  return (
    <>
      <PageHeader
        title="Connectors"
        description="The systems awzy reads resources, sharing, and work from, read-only: how each is set up, and whether it's syncing."
        actions={
          <Button size="xs" variant="default" onClick={() => void router.invalidate()}>
            Check again
          </Button>
        }
      />
      <Suspense fallback={<PageLoader />}>
        <Connectors queryRef={queryRef} />
      </Suspense>
    </>
  );
}

type Data = ConnectorsPageQuery["response"];
type Status = Data["connectors"][number];
type Config = Data["connectorConfigs"][number];

function health(c: Status | undefined, config: Config | undefined): { tone: Tone; label: string } {
  if (!config) return { tone: "neutral", label: "Not set up" };
  if (!config.enabled) return { tone: "neutral", label: "Off" };
  if (!c) return { tone: "neutral", label: "Starting" };
  if (c.lastError) return { tone: "danger", label: "Failing" };
  if (!c.lastSuccessAt) return { tone: "neutral", label: "First sync running" };
  // Two missed rounds is stale: the sync is stuck rather than between rounds.
  const age = (Date.now() - Date.parse(c.lastSuccessAt)) / 1000;
  return age > 2 * c.intervalSecs + 60 ? { tone: "warning", label: "Stale" } : { tone: "success", label: "Syncing" };
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <Table.Tr>
      <Table.Td w="45%">
        <Text size="sm" c="dimmed">
          {label}
        </Text>
      </Table.Td>
      <Table.Td>
        <Text size="sm" style={{ wordBreak: "break-word" }}>
          {children}
        </Text>
      </Table.Td>
    </Table.Tr>
  );
}

function Connectors({ queryRef }: { queryRef: PreloadedQuery<ConnectorsPageQuery> }) {
  const data = usePreloadedQuery<ConnectorsPageQuery>(
    graphql`
      query ConnectorsPageQuery {
        canConfigure: viewerCan(action: ADMINISTER, area: ENFORCEMENT)
        connectorCredentialsSupported
        connectorConfigs {
          name
          enabled
          settings
          credentialSetAt
          updatedBy
          updatedAt
        }
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
    queryRef,
  );
  const [editing, setEditing] = useState<(typeof CONNECTORS)[number] | null>(null);
  return (
    <>
      <SimpleGrid cols={{ base: 1, lg: 2 }}>
        {CONNECTORS.map((k) => {
          const config = data.connectorConfigs.find((c) => c.name === k.name);
          const c = data.connectors.find((s) => s.name === k.key);
          const h = health(c, config);
          return (
            <Section
              key={k.key}
              title={k.title}
              description={c ? `Syncs every ${Math.round(c.intervalSecs / 60)} minutes.` : undefined}
              actions={
                <Group gap="xs">
                  <StatusBadge tone={h.tone}>{h.label}</StatusBadge>
                  {data.canConfigure && (
                    <Button size="xs" variant="default" onClick={() => setEditing(k)}>
                      {config ? "Change" : "Set up"}
                    </Button>
                  )}
                </Group>
              }
            >
              {!config ? (
                <Text size="sm" c="dimmed">
                  Not set up yet.
                </Text>
              ) : (
                <Stack gap="sm">
                  <Table>
                    <Table.Tbody>
                      {(c?.settings ?? []).map((s) => (
                        <Row key={s.name} label={s.name}>
                          {s.value}
                        </Row>
                      ))}
                      <Row label="Credential">
                        {config.credentialSetAt ? `set ${formatTime(config.credentialSetAt)}` : "not set"}
                      </Row>
                      {c && (
                        <>
                          <Row label="Last synced">{c.lastSuccessAt ? <Time at={c.lastSuccessAt} /> : "not yet"}</Row>
                          <Row label="Since it started">
                            {c.written} written, {c.deleted} deleted, {c.failures} failed rounds
                          </Row>
                        </>
                      )}
                      <Row label="Changed">
                        by {config.updatedBy} on {formatTime(config.updatedAt)}
                      </Row>
                    </Table.Tbody>
                  </Table>
                  {c?.lastError && (
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
              )}
            </Section>
          );
        })}
      </SimpleGrid>
      {editing && (
        <Configure
          connector={editing}
          current={data.connectorConfigs.find((c) => c.name === editing.name)}
          credentialsSupported={data.connectorCredentialsSupported}
          onClose={() => setEditing(null)}
        />
      )}
    </>
  );
}

const str = (v: unknown) => (typeof v === "string" ? v : "");

function Configure({
  connector,
  current,
  credentialsSupported,
  onClose,
}: {
  connector: (typeof CONNECTORS)[number];
  current: Config | undefined;
  credentialsSupported: boolean;
  onClose: () => void;
}) {
  const environment = useRelayEnvironment();
  const router = useRouter();
  const s = current?.settings ?? {};
  const [enabled, setEnabled] = useState(current?.enabled ?? true);
  const [fields, setFields] = useState<Record<string, string>>({
    subject: str(s.subject),
    admin_subject: str(s.admin_subject),
    domain: str(s.domain),
    url: str(s.url),
    user: str(s.user),
    projects: Array.isArray(s.projects) ? (s.projects as unknown[]).map(str).join(", ") : "",
  });
  const [watchers, setWatchers] = useState(s.watchers === true);
  const [minutes, setMinutes] = useState<number>(typeof s.interval_secs === "number" ? s.interval_secs / 60 : 5);
  const [credential, setCredential] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const field = (name: string) => ({
    value: fields[name] ?? "",
    onChange: (e: React.ChangeEvent<HTMLInputElement>) => setFields({ ...fields, [name]: e.currentTarget.value }),
  });

  const settings: Record<string, unknown> =
    connector.name === "DRIVE"
      ? {
          subject: fields.subject,
          admin_subject: fields.admin_subject || undefined,
          domain: fields.domain || undefined,
        }
      : {
          url: fields.url,
          user: fields.user,
          projects: (fields.projects ?? "")
            .split(",")
            .map((p) => p.trim())
            .filter(Boolean),
          watchers,
        };
  settings.interval_secs = Math.max(1, Math.round(minutes)) * 60;

  const save = async () => {
    setSaving(true);
    const input = {
      input: { name: connector.name, enabled, settings, credential: credential.trim() || null },
    };
    try {
      let { configureConnector: res } = await mutate<ConnectorsPageConfigureMutation>(environment, configure, input);
      if (res.challengeQid) {
        await passChallenge(environment, res.challengeQid);
        ({ configureConnector: res } = await mutate<ConnectorsPageConfigureMutation>(environment, configure, input));
      }
      if (!res.connector) throw new Error("awzy still asks for a challenge before saving; try again.");
      onClose();
      void router.invalidate();
    } catch (e) {
      setError(webAuthnMessage(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal opened onClose={onClose} title={<Text fw={600}>{connector.title}</Text>} size="lg">
      <Stack gap="sm">
        <Switch label="On" checked={enabled} onChange={(e) => setEnabled(e.currentTarget.checked)} />
        {connector.name === "DRIVE" ? (
          <>
            <TextInput
              label="Acts as"
              description="The user the connector reads as: a Content manager of each shared drive to cover."
              placeholder="awzy-sync@example.com"
              required
              {...field("subject")}
            />
            <TextInput
              label="Reads groups as"
              description="Optional: an admin, to name the groups files are shared with."
              {...field("admin_subject")}
            />
            <TextInput
              label="Domain"
              description="Optional: your Google Workspace domain, if it isn't the user's."
              {...field("domain")}
            />
          </>
        ) : (
          <>
            <TextInput label="Site" placeholder="https://example.atlassian.net" required {...field("url")} />
            <TextInput
              label="Account"
              description="The Atlassian account the connector signs in as."
              required
              {...field("user")}
            />
            <TextInput
              label="Projects"
              description="Project keys, comma-separated; empty for every project it can browse."
              {...field("projects")}
            />
            <Checkbox
              label="Watchers count as working on an issue (one more Jira call per issue)"
              checked={watchers}
              onChange={(e) => setWatchers(e.currentTarget.checked)}
            />
          </>
        )}
        <NumberInput
          label="Sync every (minutes)"
          min={1}
          max={1440}
          value={minutes}
          onChange={(v) => setMinutes(Number(v) || 5)}
        />
        {credentialsSupported ? (
          connector.name === "DRIVE" ? (
            <Textarea
              label={connector.credential}
              description={
                current?.credentialSetAt
                  ? "Leave empty to keep the one set. It's stored in your cloud's secrets manager and never shown again."
                  : "Stored in your cloud's secrets manager and never shown again."
              }
              autosize
              minRows={4}
              value={credential}
              onChange={(e) => setCredential(e.currentTarget.value)}
            />
          ) : (
            <PasswordInput
              label={connector.credential}
              description={
                current?.credentialSetAt
                  ? "Leave empty to keep the one set. It's stored in your cloud's secrets manager and never shown again."
                  : "Stored in your cloud's secrets manager and never shown again."
              }
              value={credential}
              onChange={(e) => setCredential(e.currentTarget.value)}
            />
          )
        ) : (
          <Alert color="yellow" title="Credentials can't be set here">
            This install has no secrets manager set up for connector credentials.
          </Alert>
        )}
        {error && (
          <Alert color="red" title="Not saved">
            {error}
          </Alert>
        )}
        <Group justify="flex-end">
          <Button variant="default" onClick={onClose}>
            Cancel
          </Button>
          <Button loading={saving} onClick={() => void save()}>
            Save
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}
