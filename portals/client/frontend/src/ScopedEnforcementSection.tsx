import { Section, StatusBadge } from "@awzy/ui";
import { Alert, Button, Group, SegmentedControl, Select, Stack, Table, Text, TextInput } from "@mantine/core";
import { useState } from "react";
import { graphql, useLazyLoadQuery, useRelayEnvironment } from "react-relay";

import type { ScopeKind, ScopedEnforcementSectionQuery } from "./__generated__/ScopedEnforcementSectionQuery.graphql";
import type { ScopedEnforcementSectionSetMutation } from "./__generated__/ScopedEnforcementSectionSetMutation.graphql";
import { formatTime } from "./format";
import { mutate, passChallenge } from "./securityKeyChallenge";
import { webAuthnMessage } from "./webauthn";

const set = graphql`
  mutation ScopedEnforcementSectionSetMutation($input: SetScopedEnforcementModeInput!) {
    setScopedEnforcementMode(input: $input) {
      enforcement {
        mode
      }
      challengeQid
    }
  }
`;

type Mode = "MONITOR" | "ENFORCE";
type Kind = Exclude<ScopeKind, "%future added value">;

const KIND_LABEL: Record<Kind, string> = { ACTION: "Action", TOOL: "Tool" };

/**
 * Modes for one action or one MCP tool (ADR 0109), which a check takes over the install's: its
 * tool's first, then its action's. So agents' writes can be enforced while their reads stay in
 * monitor, and a new tool tried in monitor under an enforcing install.
 */
export function ScopedEnforcementSection({ canSwitch, onChanged }: { canSwitch: boolean; onChanged: () => void }) {
  const [fetchKey, setFetchKey] = useState(0);
  const data = useLazyLoadQuery<ScopedEnforcementSectionQuery>(
    graphql`
      query ScopedEnforcementSectionQuery {
        enforcementScopes {
          kind
          name
          setting {
            mode
            changedAt
            changedBy {
              label
            }
          }
          total
          wouldDeny
          wouldChallenge
        }
      }
    `,
    {},
    { fetchKey, fetchPolicy: "store-and-network" },
  );
  const environment = useRelayEnvironment();
  const [kind, setKind] = useState<Kind>("ACTION");
  const [name, setName] = useState("");
  const [mode, setMode] = useState<Mode>("ENFORCE");
  const [saving, setSaving] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Like the install's switch: the first try may return a security-key challenge, a touch of the
  // key passes it, and the second try switches.
  const switchTo = async (target: { kind: Kind; name: string }, next: Mode) => {
    setSaving(`${target.kind}:${target.name}`);
    try {
      const input = { input: { target, mode: next } };
      let { setScopedEnforcementMode: res } = await mutate<ScopedEnforcementSectionSetMutation>(
        environment,
        set,
        input,
      );
      if (res.challengeQid) {
        await passChallenge(environment, res.challengeQid);
        ({ setScopedEnforcementMode: res } = await mutate<ScopedEnforcementSectionSetMutation>(
          environment,
          set,
          input,
        ));
      }
      if (!res.enforcement) throw new Error("awzy still asks for a challenge before switching; try again.");
      setError(null);
      setName("");
      setFetchKey((k) => k + 1);
      onChanged();
    } catch (e) {
      setError(webAuthnMessage(e));
    } finally {
      setSaving(null);
    }
  };

  const scopes = data.enforcementScopes;
  return (
    <Section
      title="By action or tool"
      description="A mode for one action or one tool, which its checks take over the install's: the tool's first, then the action's."
    >
      <Stack gap="sm">
        {scopes.length === 0 ? (
          <Text size="sm" c="dimmed">
            Every check follows the install&apos;s mode.
          </Text>
        ) : (
          <Table>
            <Table.Thead>
              <Table.Tr>
                <Table.Th>Applies to</Table.Th>
                <Table.Th>Mode</Table.Th>
                <Table.Th>Last 24 hours</Table.Th>
                <Table.Th />
              </Table.Tr>
            </Table.Thead>
            <Table.Tbody>
              {scopes.map((s) => {
                const enforcing = s.setting.mode === "ENFORCE";
                const k: Kind = s.kind === "TOOL" ? "TOOL" : "ACTION";
                return (
                  <Table.Tr key={`${s.kind}:${s.name}`}>
                    <Table.Td>
                      <Text size="sm">
                        {KIND_LABEL[k]}{" "}
                        <Text span ff="monospace">
                          {s.name}
                        </Text>
                      </Text>
                      <Text size="xs" c="dimmed">
                        {s.setting.changedAt
                          ? `since ${formatTime(s.setting.changedAt)} by ${s.setting.changedBy?.label ?? "unknown"}`
                          : ""}
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      <StatusBadge tone={enforcing ? "danger" : "info"}>
                        {enforcing ? "Enforcing" : "Monitoring"}
                      </StatusBadge>
                    </Table.Td>
                    <Table.Td>
                      <Text size="sm">
                        {s.total} checks
                        {s.wouldDeny + s.wouldChallenge > 0
                          ? `; would have denied ${s.wouldDeny} and challenged ${s.wouldChallenge}`
                          : ""}
                      </Text>
                    </Table.Td>
                    <Table.Td>
                      {canSwitch && (
                        <Button
                          size="xs"
                          variant="light"
                          color={enforcing ? "blue" : "red"}
                          loading={saving === `${k}:${s.name}`}
                          onClick={() => void switchTo({ kind: k, name: s.name }, enforcing ? "MONITOR" : "ENFORCE")}
                        >
                          {enforcing ? "Monitor" : "Enforce"}
                        </Button>
                      )}
                    </Table.Td>
                  </Table.Tr>
                );
              })}
            </Table.Tbody>
          </Table>
        )}
        {canSwitch && (
          <Group align="flex-end">
            <Select
              label="Applies to"
              data={[
                { value: "ACTION", label: "An action" },
                { value: "TOOL", label: "A tool" },
              ]}
              value={kind}
              onChange={(v) => setKind(v === "TOOL" ? "TOOL" : "ACTION")}
              allowDeselect={false}
              w={150}
            />
            <TextInput
              label={kind === "TOOL" ? "Tool (<server>/<tool>)" : "Action"}
              placeholder={kind === "TOOL" ? "larkspur-drive/read_document" : "write"}
              value={name}
              onChange={(e) => setName(e.currentTarget.value)}
              w={280}
            />
            <SegmentedControl
              value={mode}
              onChange={(v) => setMode(v === "MONITOR" ? "MONITOR" : "ENFORCE")}
              data={[
                { value: "MONITOR", label: "Monitor" },
                { value: "ENFORCE", label: "Enforce" },
              ]}
            />
            <Button
              disabled={!name.trim()}
              loading={saving === `${kind}:${name.trim()}`}
              onClick={() => void switchTo({ kind, name: name.trim() }, mode)}
            >
              Set
            </Button>
          </Group>
        )}
        {error && (
          <Alert color="red" title="Not switched">
            {error}
          </Alert>
        )}
        {canSwitch && (
          <Text size="xs" c="dimmed">
            You&apos;ll touch your security key to confirm, as for the install&apos;s mode.
          </Text>
        )}
      </Stack>
    </Section>
  );
}
