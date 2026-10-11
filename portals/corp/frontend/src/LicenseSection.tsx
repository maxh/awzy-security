import { isOneOf, Section, StatusBadge } from "@awzy/ui";
import { Alert, Button, Chip, Group, Modal, NumberInput, Select, Stack, Table, Text, TextInput } from "@mantine/core";
import { useRouter } from "@tanstack/react-router";
import { useState } from "react";
import { graphql, useFragment, useMutation } from "react-relay";

import type {
  Entitlement,
  LicensePlan,
  LicenseSectionSetMutation,
} from "./__generated__/LicenseSectionSetMutation.graphql";
import type { LicenseSection_install$key } from "./__generated__/LicenseSection_install.graphql";
import { formatTime } from "./format";

const PLANS: { value: LicensePlan; label: string }[] = [
  { value: "TRIAL", label: "Trial" },
  { value: "STANDARD", label: "Standard" },
  { value: "ENTERPRISE", label: "Enterprise" },
];

const ENTITLEMENTS: { value: Entitlement; label: string }[] = [
  { value: "AWZY_BOT", label: "Awzy Bot" },
  { value: "INVESTIGATIONS", label: "Investigations" },
  { value: "DRIVE", label: "Google Drive" },
  { value: "JIRA", label: "Jira" },
];

const ENTITLEMENT_NAMES: readonly Entitlement[] = ENTITLEMENTS.map((e) => e.value);

const label = (list: { value: string; label: string }[], v: string) => list.find((x) => x.value === v)?.label ?? v;

/** What the client is licensed for, shared by all its installs, and a way for staff to set it. */
export function LicenseSection({ install }: { install: LicenseSection_install$key }) {
  const i = useFragment(
    graphql`
      fragment LicenseSection_install on Install {
        client
        license {
          plan
          humans
          expiresAt
          expired
          entitlements
          updatedBy {
            label
          }
          updatedAt
        }
      }
    `,
    install,
  );
  const [editing, setEditing] = useState(false);
  const l = i.license;
  return (
    <Section
      title="License"
      description={`What ${i.client} is licensed for, shared by all its installs.`}
      actions={
        <Button size="xs" variant="default" onClick={() => setEditing(true)}>
          {l ? "Change" : "Set license"}
        </Button>
      }
    >
      {l ? (
        <Table>
          <Table.Tbody>
            <Table.Tr>
              <Table.Td w={180}>Plan</Table.Td>
              <Table.Td>
                <Group gap="xs">
                  {label(PLANS, l.plan)}
                  {l.expired && <StatusBadge tone="danger">Expired</StatusBadge>}
                </Group>
              </Table.Td>
            </Table.Tr>
            <Table.Tr>
              <Table.Td>People covered</Table.Td>
              <Table.Td>{l.humans}</Table.Td>
            </Table.Tr>
            <Table.Tr>
              <Table.Td>Ends</Table.Td>
              <Table.Td>{l.expiresAt ? formatTime(l.expiresAt) : "never"}</Table.Td>
            </Table.Tr>
            <Table.Tr>
              <Table.Td>Includes</Table.Td>
              <Table.Td>
                {l.entitlements.length
                  ? l.entitlements.map((e) => label(ENTITLEMENTS, e)).join(", ")
                  : "the base product"}
              </Table.Td>
            </Table.Tr>
            <Table.Tr>
              <Table.Td>Set</Table.Td>
              <Table.Td c="dimmed">
                by {l.updatedBy.label} on {formatTime(l.updatedAt)}
              </Table.Td>
            </Table.Tr>
          </Table.Tbody>
        </Table>
      ) : (
        <Text size="sm" c="dimmed">
          No license recorded for {i.client} yet.
        </Text>
      )}
      {editing && (
        <EditLicense
          client={i.client}
          current={
            l
              ? { plan: l.plan, humans: l.humans, expiresAt: l.expiresAt ?? null, entitlements: [...l.entitlements] }
              : null
          }
          onClose={() => setEditing(false)}
        />
      )}
    </Section>
  );
}

type Current = { plan: LicensePlan; humans: number; expiresAt: string | null; entitlements: Entitlement[] };

function EditLicense({ client, current, onClose }: { client: string; current: Current | null; onClose: () => void }) {
  const [plan, setPlan] = useState<LicensePlan>(current?.plan ?? "TRIAL");
  const [humans, setHumans] = useState<number>(current?.humans ?? 25);
  // A date, as the input shows it; it ends at the start of that day, UTC.
  const [ends, setEnds] = useState(current?.expiresAt?.slice(0, 10) ?? "");
  const [entitlements, setEntitlements] = useState<string[]>(current?.entitlements ?? []);
  const [error, setError] = useState<string | null>(null);
  const router = useRouter();
  const [commit, isSaving] = useMutation<LicenseSectionSetMutation>(graphql`
    mutation LicenseSectionSetMutation($input: SetLicenseInput!) {
      setLicense(input: $input) {
        client
        plan
        humans
        expiresAt
        expired
        entitlements
        updatedBy {
          label
        }
        updatedAt
      }
    }
  `);
  const save = () =>
    commit({
      variables: {
        input: {
          client,
          plan,
          humans,
          expiresAt: ends ? `${ends}T00:00:00Z` : null,
          entitlements: entitlements.filter((e) => isOneOf(ENTITLEMENT_NAMES, e)),
        },
      },
      // The license isn't a Node, so the page loads its query again to show it.
      onCompleted: () => {
        void router.invalidate();
        onClose();
      },
      onError: (e) => setError(e.message),
    });
  return (
    <Modal opened onClose={onClose} title={<Text fw={600}>License for {client}</Text>}>
      <Stack gap="sm">
        <Select label="Plan" data={PLANS} value={plan} onChange={(v) => v && setPlan(v)} allowDeselect={false} />
        <NumberInput label="People covered" min={1} value={humans} onChange={(v) => setHumans(Number(v) || 1)} />
        <TextInput
          label="Ends"
          description="Leave empty for a license that doesn't end."
          type="date"
          value={ends}
          onChange={(e) => setEnds(e.currentTarget.value)}
        />
        <div>
          <Text size="sm" fw={500} mb={4}>
            Includes
          </Text>
          <Chip.Group multiple value={entitlements} onChange={setEntitlements}>
            <Group gap="xs">
              {ENTITLEMENTS.map((e) => (
                <Chip key={e.value} value={e.value} size="sm">
                  {e.label}
                </Chip>
              ))}
            </Group>
          </Chip.Group>
        </div>
        {error && (
          <Alert color="red" title="Not saved">
            {error}
          </Alert>
        )}
        <Group justify="flex-end">
          <Button variant="default" onClick={onClose}>
            Cancel
          </Button>
          <Button loading={isSaving} onClick={save}>
            Save
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}
