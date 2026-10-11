import { routeQuery } from "@awzy/relay/routes";
import { PageHeader, PageLoader, Stat, StatusBadge } from "@awzy/ui";
import {
  Alert,
  Badge,
  Button,
  Card,
  Group,
  Modal,
  SimpleGrid,
  Stack,
  Switch,
  Table,
  Tabs,
  Text,
  TextInput,
} from "@mantine/core";
import { getRouteApi } from "@tanstack/react-router";
import { type FormEvent, Suspense, useState } from "react";
import { graphql, type PreloadedQuery, useLazyLoadQuery, useMutation, usePreloadedQuery } from "react-relay";

import type { DirectoryPageGroupsQuery } from "./__generated__/DirectoryPageGroupsQuery.graphql";
import type { DirectoryPageHumansQuery } from "./__generated__/DirectoryPageHumansQuery.graphql";
import type { DirectoryPageRegisterMutation } from "./__generated__/DirectoryPageRegisterMutation.graphql";
import type { DirectoryPageRemoveMutation } from "./__generated__/DirectoryPageRemoveMutation.graphql";
import type { DirectoryPageServiceAccountsQuery } from "./__generated__/DirectoryPageServiceAccountsQuery.graphql";
import type { DirectoryPageSummaryQuery } from "./__generated__/DirectoryPageSummaryQuery.graphql";
import DirectoryPageSummaryQueryNode from "./__generated__/DirectoryPageSummaryQuery.graphql";
import { formatTime } from "./format";

/** Started by the route's loader as navigation begins (ADR 0099). */
export const loadDirectoryPageSummary = routeQuery<DirectoryPageSummaryQuery>(DirectoryPageSummaryQueryNode);

const loading = <PageLoader />;

const route = getRouteApi("/directory");

/** People and groups from the IdP (read-only here), and the service accounts admins register. */
export function DirectoryPage() {
  const queryRef = route.useLoaderData();
  return (
    <Stack gap="md">
      <PageHeader
        title="Directory"
        description="The people and groups your IdP provisions, and the service accounts and AI agents registered here."
      />
      <Suspense fallback={loading}>
        <Summary queryRef={queryRef} />
      </Suspense>
      <Card padding="lg">
        <Tabs defaultValue="humans" keepMounted={false}>
          <Tabs.List>
            <Tabs.Tab value="humans">People</Tabs.Tab>
            <Tabs.Tab value="groups">Groups</Tabs.Tab>
            <Tabs.Tab value="service-accounts">Service accounts</Tabs.Tab>
          </Tabs.List>
          <Tabs.Panel value="humans" pt="md">
            <Humans />
          </Tabs.Panel>
          <Tabs.Panel value="groups" pt="md">
            <Suspense fallback={loading}>
              <Groups />
            </Suspense>
          </Tabs.Panel>
          <Tabs.Panel value="service-accounts" pt="md">
            <ServiceAccounts />
          </Tabs.Panel>
        </Tabs>
      </Card>
    </Stack>
  );
}

function Summary({ queryRef }: { queryRef: PreloadedQuery<DirectoryPageSummaryQuery> }) {
  const { directorySummary: s } = usePreloadedQuery<DirectoryPageSummaryQuery>(
    graphql`
      query DirectoryPageSummaryQuery {
        directorySummary {
          humans
          activeHumans
          groups
          serviceAccounts
          lastProvisionedAt
        }
      }
    `,
    queryRef,
  );
  return (
    <SimpleGrid cols={{ base: 2, md: 4 }}>
      <Stat label="People" value={s.activeHumans.toLocaleString()} hint={`active, of ${s.humans.toLocaleString()}`} />
      <Stat label="Groups" value={s.groups.toLocaleString()} hint="from your IdP over SCIM" />
      <Stat label="Service accounts" value={s.serviceAccounts.toLocaleString()} hint="registered here" />
      <Stat
        label="Last provisioned"
        value={
          <Text fz="md" fw={600}>
            {s.lastProvisionedAt ? formatTime(s.lastProvisionedAt) : "Never"}
          </Text>
        }
        hint="changes reach every PDP within seconds"
      />
    </SimpleGrid>
  );
}

function Humans() {
  const [search, setSearch] = useState("");
  const [applied, setApplied] = useState("");
  const submit = (e: FormEvent) => {
    e.preventDefault();
    setApplied(search.trim());
  };
  return (
    <Stack>
      <form onSubmit={submit}>
        <Group>
          <TextInput
            placeholder="Search by user name or display name"
            value={search}
            onChange={(e) => setSearch(e.currentTarget.value)}
            w={360}
          />
          <Button type="submit" variant="default">
            Search
          </Button>
        </Group>
      </form>
      <Suspense fallback={loading}>
        <HumansTable search={applied} />
      </Suspense>
    </Stack>
  );
}

function HumansTable({ search }: { search: string }) {
  const { humans } = useLazyLoadQuery<DirectoryPageHumansQuery>(
    graphql`
      query DirectoryPageHumansQuery($search: String) {
        humans(search: $search, first: 200) {
          userName
          displayName
          active
          groups
          updatedAt
        }
      }
    `,
    { search: search || null },
    { fetchPolicy: "store-and-network" },
  );
  if (humans.length === 0)
    return (
      <Text c="dimmed">
        No people{search ? ` matching "${search}"` : " yet: set up SCIM provisioning in your IdP"}.
      </Text>
    );
  return (
    <Table>
      <Table.Thead>
        <Table.Tr>
          <Table.Th>User name</Table.Th>
          <Table.Th>Name</Table.Th>
          <Table.Th>Groups</Table.Th>
          <Table.Th>Status</Table.Th>
          <Table.Th>Changed</Table.Th>
        </Table.Tr>
      </Table.Thead>
      <Table.Tbody>
        {humans.map((p) => (
          <Table.Tr key={p.userName}>
            <Table.Td ff="monospace">{p.userName}</Table.Td>
            <Table.Td>{p.displayName ?? ""}</Table.Td>
            <Table.Td>
              <Group gap={4}>
                {p.groups.map((g) => (
                  <Badge key={g} variant="default">
                    {g}
                  </Badge>
                ))}
              </Group>
            </Table.Td>
            <Table.Td>
              <StatusBadge tone={p.active ? "success" : "danger"}>{p.active ? "Active" : "Deactivated"}</StatusBadge>
            </Table.Td>
            <Table.Td>{formatTime(p.updatedAt)}</Table.Td>
          </Table.Tr>
        ))}
      </Table.Tbody>
    </Table>
  );
}

function Groups() {
  const { groups } = useLazyLoadQuery<DirectoryPageGroupsQuery>(
    graphql`
      query DirectoryPageGroupsQuery {
        groups {
          name
          humans
          groups
          parents
          updatedAt
        }
      }
    `,
    {},
    { fetchPolicy: "store-and-network" },
  );
  if (groups.length === 0) return <Text c="dimmed">No groups yet.</Text>;
  return (
    <Table>
      <Table.Thead>
        <Table.Tr>
          <Table.Th>Group (as policies name it)</Table.Th>
          <Table.Th>People</Table.Th>
          <Table.Th>Nested groups</Table.Th>
          <Table.Th>Inside</Table.Th>
          <Table.Th>Changed</Table.Th>
        </Table.Tr>
      </Table.Thead>
      <Table.Tbody>
        {groups.map((g) => (
          <Table.Tr key={g.name}>
            <Table.Td ff="monospace">Group::"{g.name}"</Table.Td>
            <Table.Td>{g.humans}</Table.Td>
            <Table.Td>{g.groups}</Table.Td>
            <Table.Td>{g.parents.join(", ")}</Table.Td>
            <Table.Td>{formatTime(g.updatedAt)}</Table.Td>
          </Table.Tr>
        ))}
      </Table.Tbody>
    </Table>
  );
}

function ServiceAccounts() {
  const [fetchKey, setFetchKey] = useState(0);
  return (
    <Suspense fallback={loading}>
      <ServiceAccountsTable fetchKey={fetchKey} onChanged={() => setFetchKey((k) => k + 1)} />
    </Suspense>
  );
}

function ServiceAccountsTable({ fetchKey, onChanged }: { fetchKey: number; onChanged: () => void }) {
  const data = useLazyLoadQuery<DirectoryPageServiceAccountsQuery>(
    graphql`
      query DirectoryPageServiceAccountsQuery {
        canAdminister: viewerCan(action: ADMINISTER, area: DIRECTORY)
        serviceAccounts {
          id
          owner
          ai
          groups
          description
          updatedBy {
            label
          }
          updatedAt
        }
      }
    `,
    {},
    { fetchKey, fetchPolicy: "store-and-network" },
  );
  const [registering, setRegistering] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [commitRemove, isRemoving] = useMutation<DirectoryPageRemoveMutation>(graphql`
    mutation DirectoryPageRemoveMutation($input: RemoveServiceAccountInput!) {
      removeServiceAccount(input: $input) {
        removedId
      }
    }
  `);
  const remove = (id: string) => {
    if (!window.confirm(`Remove ${id}? From then on the PDP denies everything it does.`)) return;
    commitRemove({ variables: { input: { id } }, onCompleted: onChanged, onError: (e) => setError(e.message) });
  };

  return (
    <Stack>
      <Group justify="space-between">
        <Text size="sm" c="dimmed">
          Agents, backend services, and scheduled jobs. The PDP denies any service account that isn't registered here.
        </Text>
        {data.canAdminister && <Button onClick={() => setRegistering(true)}>Register service account</Button>}
      </Group>
      {error && <Alert color="red">{error}</Alert>}
      {data.serviceAccounts.length === 0 ? (
        <Text c="dimmed">No service accounts registered.</Text>
      ) : (
        <Table>
          <Table.Thead>
            <Table.Tr>
              <Table.Th>ID</Table.Th>
              <Table.Th>Owner</Table.Th>
              <Table.Th>Kind</Table.Th>
              <Table.Th>Groups</Table.Th>
              <Table.Th>Description</Table.Th>
              <Table.Th>Changed</Table.Th>
              <Table.Th />
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {data.serviceAccounts.map((sa) => (
              <Table.Tr key={sa.id}>
                <Table.Td ff="monospace">{sa.id}</Table.Td>
                <Table.Td>{sa.owner}</Table.Td>
                <Table.Td>
                  <Badge color={sa.ai ? "grape" : "gray"} variant="light">
                    {sa.ai ? "AI agent" : "Software"}
                  </Badge>
                </Table.Td>
                <Table.Td>{sa.groups.join(", ")}</Table.Td>
                <Table.Td>{sa.description}</Table.Td>
                <Table.Td>
                  {formatTime(sa.updatedAt)} by {sa.updatedBy?.label ?? "unknown"}
                </Table.Td>
                <Table.Td>
                  {data.canAdminister && (
                    <Button size="xs" variant="subtle" color="red" loading={isRemoving} onClick={() => remove(sa.id)}>
                      Remove
                    </Button>
                  )}
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      )}
      <RegisterServiceAccount opened={registering} onClose={() => setRegistering(false)} onRegistered={onChanged} />
    </Stack>
  );
}

function RegisterServiceAccount({
  opened,
  onClose,
  onRegistered,
}: {
  opened: boolean;
  onClose: () => void;
  onRegistered: () => void;
}) {
  const [id, setId] = useState("");
  const [owner, setOwner] = useState("");
  const [ai, setAi] = useState(true);
  const [groups, setGroups] = useState("");
  const [description, setDescription] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [commit, isSaving] = useMutation<DirectoryPageRegisterMutation>(graphql`
    mutation DirectoryPageRegisterMutation($input: RegisterServiceAccountInput!) {
      registerServiceAccount(input: $input) {
        serviceAccount {
          id
        }
      }
    }
  `);
  const submit = (e: FormEvent) => {
    e.preventDefault();
    commit({
      variables: {
        input: {
          id: id.trim(),
          owner: owner.trim(),
          ai,
          groups: groups
            .split(",")
            .map((g) => g.trim())
            .filter(Boolean),
          description: description.trim(),
        },
      },
      onCompleted: () => {
        setError(null);
        onRegistered();
        onClose();
      },
      onError: (err) => setError(err.message),
    });
  };
  return (
    <Modal opened={opened} onClose={onClose} title="Register a service account" size="lg">
      <form onSubmit={submit}>
        <Stack>
          {error && <Alert color="red">{error}</Alert>}
          <TextInput
            label="ID"
            description="As its tokens name it: their sub, or act.sub when it acts for a person"
            placeholder="sales-genie"
            value={id}
            onChange={(e) => setId(e.currentTarget.value)}
            required
          />
          <TextInput
            label="Owner"
            description="The accountable person's user name; must be provisioned by your IdP"
            value={owner}
            onChange={(e) => setOwner(e.currentTarget.value)}
            required
          />
          <Switch label="AI agent" checked={ai} onChange={(e) => setAi(e.currentTarget.checked)} />
          <TextInput
            label="Groups"
            description="Comma-separated, as policies name them"
            value={groups}
            onChange={(e) => setGroups(e.currentTarget.value)}
          />
          <TextInput label="Description" value={description} onChange={(e) => setDescription(e.currentTarget.value)} />
          <Group justify="flex-end">
            <Button type="submit" loading={isSaving}>
              Register
            </Button>
          </Group>
        </Stack>
      </form>
    </Modal>
  );
}
