import { Section } from "@awzy/ui";
import { ActionIcon, Alert, Button, Checkbox, Group, Select, Stack, Table, Text, TextInput } from "@mantine/core";
import { IconTrash } from "@tabler/icons-react";
import { Suspense, useState } from "react";
import { graphql, useLazyLoadQuery, useRelayEnvironment } from "react-relay";

import type { OutageFallbackSectionQuery, SensitivityTier } from "./__generated__/OutageFallbackSectionQuery.graphql";
import type { OutageFallbackSectionSetMutation } from "./__generated__/OutageFallbackSectionSetMutation.graphql";
import { formatTime } from "./format";
import { mutate, passChallenge } from "./securityKeyChallenge";
import { webAuthnMessage } from "./webauthn";

const set = graphql`
  mutation OutageFallbackSectionSetMutation($input: SetOutageFallbackInput!) {
    setOutageFallback(input: $input) {
      outageFallback {
        version
      }
      challengeQid
    }
  }
`;

const TIERS: { value: SensitivityTier; label: string }[] = [
  { value: "PUBLIC", label: "Public" },
  { value: "INTERNAL", label: "Internal" },
  { value: "CONFIDENTIAL", label: "Confidential" },
  { value: "CROWN_JEWELS", label: "Crown jewels" },
];
const label = (t: SensitivityTier) => TIERS.find((x) => x.value === t)?.label ?? t;
// Confidential and crown jewels always fail closed.
const CAN_FAIL_OPEN: ReadonlySet<SensitivityTier> = new Set(["PUBLIC", "INTERNAL"]);

/** One row of the form; `id` keeps each row's inputs its own as rows are added and removed. */
type Row = { id: string; name: string; tier: SensitivityTier };
const newRow = (name: string, tier: SensitivityTier): Row => ({ id: crypto.randomUUID(), name, tier });

/**
 * What awzy-gateway does while the PDP is unreachable (ADR 0089): each MCP tool's tier, which
 * every check also uses, and which tiers replay a recent allow instead of failing closed.
 */
export function OutageFallbackSection({ canEdit }: { canEdit: boolean }) {
  const [fetchKey, setFetchKey] = useState(0);
  return (
    <Suspense
      fallback={
        <Section title="During a PDP outage">
          <Text size="sm" c="dimmed">
            Loading…
          </Text>
        </Section>
      }
    >
      <Fallback canEdit={canEdit} fetchKey={fetchKey} onSaved={() => setFetchKey((k) => k + 1)} />
    </Suspense>
  );
}

function Fallback({ canEdit, fetchKey, onSaved }: { canEdit: boolean; fetchKey: number; onSaved: () => void }) {
  const { outageFallback: f } = useLazyLoadQuery<OutageFallbackSectionQuery>(
    graphql`
      query OutageFallbackSectionQuery {
        outageFallback {
          version
          tools {
            tool
            tier
          }
          servers {
            server
            tier
          }
          failOpenTiers
          changedBy {
            label
          }
          changedAt
        }
      }
    `,
    {},
    { fetchKey, fetchPolicy: "network-only" },
  );
  const environment = useRelayEnvironment();
  const [editing, setEditing] = useState(false);
  const [tools, setTools] = useState<Row[]>([]);
  const [servers, setServers] = useState<Row[]>([]);
  const [failOpen, setFailOpen] = useState<SensitivityTier[]>([]);
  const [isSaving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const edit = () => {
    setTools(f.tools.map((t) => newRow(t.tool, t.tier)));
    setServers(f.servers.map((s) => newRow(s.server, s.tier)));
    setFailOpen([...f.failOpenTiers]);
    setError(null);
    setEditing(true);
  };

  // Saving asks for the admin's security key, like switching the mode.
  const save = async () => {
    setSaving(true);
    try {
      const input = {
        input: {
          tools: tools.filter((t) => t.name.trim()).map((t) => ({ tool: t.name.trim(), tier: t.tier })),
          servers: servers.filter((s) => s.name.trim()).map((s) => ({ server: s.name.trim(), tier: s.tier })),
          failOpenTiers: failOpen,
        },
      };
      let { setOutageFallback: res } = await mutate<OutageFallbackSectionSetMutation>(environment, set, input);
      if (res.challengeQid) {
        await passChallenge(environment, res.challengeQid);
        ({ setOutageFallback: res } = await mutate<OutageFallbackSectionSetMutation>(environment, set, input));
      }
      if (!res.outageFallback) throw new Error("awzy still asks for a challenge before saving; try again.");
      setEditing(false);
      onSaved();
    } catch (e) {
      setError(webAuthnMessage(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Section
      title="During a PDP outage"
      actions={
        canEdit && !editing ? (
          <Button variant="default" onClick={edit}>
            Edit
          </Button>
        ) : undefined
      }
    >
      <Stack gap="sm">
        <Text size="sm" maw={760}>
          Each MCP tool's sensitivity tier goes with every check awzy-gateway makes. If the PDP can't be reached, a tool
          classified here by name in a tier that fails open replays a recent allow for the same caller; everything else
          fails closed. Confidential and crown jewels always fail closed, and a tool nobody classified counts as
          confidential.
        </Text>
        <Text size="sm" c="dimmed">
          {f.version === 0
            ? "Never set: every tool counts as confidential and fails closed."
            : `Version ${f.version}, set ${f.changedAt ? formatTime(f.changedAt) : ""} by ${f.changedBy?.label ?? "unknown"}.`}
        </Text>
        {error && (
          <Alert color="red" variant="light">
            {error}
          </Alert>
        )}
        {editing ? (
          <Editor
            tools={tools}
            setTools={setTools}
            servers={servers}
            setServers={setServers}
            failOpen={failOpen}
            setFailOpen={setFailOpen}
          />
        ) : (
          <Current tools={f.tools} servers={f.servers} failOpen={f.failOpenTiers} />
        )}
        {editing && (
          <Group justify="flex-end">
            <Button variant="default" onClick={() => setEditing(false)}>
              Cancel
            </Button>
            <Button loading={isSaving} onClick={() => void save()}>
              Save
            </Button>
          </Group>
        )}
      </Stack>
    </Section>
  );
}

function Current({
  tools,
  servers,
  failOpen,
}: {
  tools: readonly { tool: string; tier: SensitivityTier }[];
  servers: readonly { server: string; tier: SensitivityTier }[];
  failOpen: readonly SensitivityTier[];
}) {
  return (
    <Stack gap="xs">
      <Text size="sm">
        <b>Fails open:</b> {failOpen.length ? failOpen.map(label).join(", ") : "nothing"}
      </Text>
      {tools.length > 0 && (
        <Table>
          <Table.Thead>
            <Table.Tr>
              <Table.Th>Tool</Table.Th>
              <Table.Th>Tier</Table.Th>
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {tools.map((t) => (
              <Table.Tr key={t.tool}>
                <Table.Td ff="monospace">{t.tool}</Table.Td>
                <Table.Td>{label(t.tier)}</Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      )}
      {servers.map((s) => (
        <Text key={s.server} size="sm" c="dimmed">
          Other tools on <code>{s.server}</code>: {label(s.tier)}, and they fail closed.
        </Text>
      ))}
    </Stack>
  );
}

function Editor({
  tools,
  setTools,
  servers,
  setServers,
  failOpen,
  setFailOpen,
}: {
  tools: Row[];
  setTools: (rows: Row[]) => void;
  servers: Row[];
  setServers: (rows: Row[]) => void;
  failOpen: SensitivityTier[];
  setFailOpen: (tiers: SensitivityTier[]) => void;
}) {
  return (
    <Stack gap="md">
      <Checkbox.Group label="Tiers that fail open" value={failOpen} onChange={setFailOpen}>
        <Group mt="xs">
          {TIERS.map((t) => (
            <Checkbox key={t.value} value={t.value} label={t.label} disabled={!CAN_FAIL_OPEN.has(t.value)} />
          ))}
        </Group>
      </Checkbox.Group>
      <Rows title="Tools, as <server>/<tool>" placeholder="drive/search" rows={tools} setRows={setTools} />
      <Rows title="A server's other tools" placeholder="drive" rows={servers} setRows={setServers} />
    </Stack>
  );
}

function Rows({
  title,
  placeholder,
  rows,
  setRows,
}: {
  title: string;
  placeholder: string;
  rows: Row[];
  setRows: (rows: Row[]) => void;
}) {
  const update = (i: number, row: Partial<Row>) => setRows(rows.map((r, j) => (j === i ? { ...r, ...row } : r)));
  return (
    <Stack gap="xs">
      <Text size="sm" fw={500}>
        {title}
      </Text>
      {rows.map((r, i) => (
        <Group key={r.id} gap="xs" wrap="nowrap">
          <TextInput
            aria-label="Name"
            placeholder={placeholder}
            value={r.name}
            onChange={(e) => update(i, { name: e.currentTarget.value })}
            style={{ flex: 1 }}
          />
          <Select
            aria-label="Tier"
            data={TIERS}
            value={r.tier}
            allowDeselect={false}
            onChange={(v) => v && update(i, { tier: v })}
            w={160}
          />
          <ActionIcon
            variant="subtle"
            color="gray"
            aria-label="Remove"
            onClick={() => setRows(rows.filter((_, j) => j !== i))}
          >
            <IconTrash size={16} />
          </ActionIcon>
        </Group>
      ))}
      <Group>
        <Button size="xs" variant="light" onClick={() => setRows([...rows, newRow("", "CONFIDENTIAL")])}>
          Add
        </Button>
      </Group>
    </Stack>
  );
}
