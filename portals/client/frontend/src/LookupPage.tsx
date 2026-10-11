import { EmptyState, PageHeader, PageLoader, Section, StatusBadge } from "@awzy/ui";
import { Anchor, Button, Code, Group, SegmentedControl, Stack, Table, Text, TextInput } from "@mantine/core";
import { IconSearch } from "@tabler/icons-react";
import { type ReactNode, Suspense, useState } from "react";
import { graphql, useLazyLoadQuery } from "react-relay";

import { Hidden, prettyJson, prettyObject } from "./format";
import type { LookupPagePrincipalQuery, PrincipalType } from "./__generated__/LookupPagePrincipalQuery.graphql";
import type { LookupPageResourceQuery } from "./__generated__/LookupPageResourceQuery.graphql";
import type { LookupPageSearchQuery } from "./__generated__/LookupPageSearchQuery.graphql";

/** Look up a principal or a resource as the PDPs see it: attributes, groups, risk, tiers, sharing. */
export function LookupPage() {
  const [kind, setKind] = useState<"principal" | "resource">("principal");
  return (
    <>
      <PageHeader
        title="Look up"
        description="A person, service account, or resource as awzy's policy decision points see it."
      />
      <Stack gap="md">
        <SegmentedControl
          w={320}
          value={kind}
          onChange={(v) => setKind(v)}
          data={[
            { value: "principal", label: "Principal" },
            { value: "resource", label: "Resource" },
          ]}
        />
        {kind === "principal" ? <PrincipalLookup /> : <ResourceLookup />}
      </Stack>
    </>
  );
}

function PrincipalLookup() {
  const [type, setType] = useState<PrincipalType>("HUMAN");
  const [draft, setDraft] = useState("");
  const [id, setId] = useState<string | null>(null);
  return (
    <Stack gap="md">
      <Group align="flex-end">
        <SegmentedControl
          value={type}
          onChange={(v) => setType(v)}
          data={[
            { value: "HUMAN", label: "Person" },
            { value: "SERVICE_ACCOUNT", label: "Service account" },
          ]}
        />
        <TextInput
          label="ID"
          placeholder="alice, or sales-genie"
          value={draft}
          onChange={(e) => setDraft(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && setId(draft.trim() || null)}
          w={320}
        />
        <Button leftSection={<IconSearch size={16} />} onClick={() => setId(draft.trim() || null)}>
          Look up
        </Button>
      </Group>
      {id && (
        <Suspense fallback={<PageLoader />}>
          <Principal type={type} id={id} />
        </Suspense>
      )}
    </Stack>
  );
}

function Principal({ type, id }: { type: PrincipalType; id: string }) {
  const { principal: p } = useLazyLoadQuery<LookupPagePrincipalQuery>(
    graphql`
      query LookupPagePrincipalQuery($type: PrincipalType!, $id: String!) {
        principal(type: $type, id: $id) {
          known
          attributes
          groups
          riskScore
        }
      }
    `,
    { type, id },
    { fetchPolicy: "network-only" },
  );
  if (!p.known) {
    return (
      <EmptyState
        icon={<IconSearch size={22} />}
        title={`awzy doesn't know ${id}`}
        description="Not provisioned by your IdP, or not a registered service account."
      />
    );
  }
  return (
    <Section title={id}>
      <Stack gap="sm">
        <Group gap="xs">
          <Text size="sm" fw={500}>
            Risk
          </Text>
          {p.riskScore == null ? (
            <Text size="sm" c="dimmed">
              not scored, or hidden by a policy
            </Text>
          ) : (
            <StatusBadge tone={p.riskScore >= 70 ? "danger" : p.riskScore >= 40 ? "warning" : "success"}>
              {p.riskScore}
            </StatusBadge>
          )}
        </Group>
        <div>
          <Text size="sm" fw={500}>
            Groups
          </Text>
          <Text size="sm">{p.groups.length ? p.groups.join(", ") : "none"}</Text>
        </div>
        <div>
          <Text size="sm" fw={500}>
            Attributes
          </Text>
          {p.attributes == null ? <Hidden /> : <Code block>{prettyObject(p.attributes)}</Code>}
        </div>
      </Stack>
    </Section>
  );
}

function ResourceLookup() {
  const [draft, setDraft] = useState("");
  const [query, setQuery] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  return (
    <Stack gap="md">
      <Group align="flex-end">
        <TextInput
          label="Search"
          placeholder="Part of a resource's name or ID"
          value={draft}
          onChange={(e) => setDraft(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && setQuery(draft.trim() || null)}
          w={420}
        />
        <Button leftSection={<IconSearch size={16} />} onClick={() => setQuery(draft.trim() || null)}>
          Search
        </Button>
      </Group>
      {query && (
        <Suspense fallback={<PageLoader />}>
          <Results query={query} onSelect={setSelected} />
        </Suspense>
      )}
      {selected && (
        <Suspense fallback={<PageLoader />}>
          <Resource id={selected} />
        </Suspense>
      )}
    </Stack>
  );
}

function Results({ query, onSelect }: { query: string; onSelect: (id: string) => void }) {
  const { searchResources } = useLazyLoadQuery<LookupPageSearchQuery>(
    graphql`
      query LookupPageSearchQuery($query: String!) {
        searchResources(query: $query) {
          id
          name
          kind
          source
          owner
          sensitivity
        }
      }
    `,
    { query },
    { fetchPolicy: "network-only" },
  );
  if (searchResources.length === 0) {
    return <EmptyState icon={<IconSearch size={22} />} title="Nothing matches" />;
  }
  return (
    <Section flush>
      <Table highlightOnHover>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>Name</Table.Th>
            <Table.Th>Kind</Table.Th>
            <Table.Th>Owner</Table.Th>
            <Table.Th>Sensitivity</Table.Th>
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {searchResources.map((r) => (
            <Table.Tr key={r.id} style={{ cursor: "pointer" }} onClick={() => onSelect(r.id)}>
              <Table.Td>
                <Text size="sm">{r.name}</Text>
                <Text size="xs" c="dimmed" ff="monospace">
                  {r.id}
                </Text>
              </Table.Td>
              <Table.Td>
                <Text size="sm">
                  {r.kind} ({r.source})
                </Text>
              </Table.Td>
              <Table.Td>
                <Text size="sm">{r.owner ?? ""}</Text>
              </Table.Td>
              <Table.Td>
                <Text size="sm">{r.sensitivity ?? "unclassified"}</Text>
              </Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
    </Section>
  );
}

function Resource({ id }: { id: string }) {
  const { resource: r } = useLazyLoadQuery<LookupPageResourceQuery>(
    graphql`
      query LookupPageResourceQuery($id: String!) {
        resource(id: $id) {
          id
          name
          kind
          source
          owner
          sensitivity
          classification {
            sensitivity
            by
            reason
          }
          sharing
          labels
          url
          folders
          assignees
        }
      }
    `,
    { id },
    { fetchPolicy: "network-only" },
  );
  if (!r) return <EmptyState icon={<IconSearch size={22} />} title={`awzy doesn't know ${id}`} />;
  return (
    <Section title={r.name} description={r.id}>
      <Table>
        <Table.Tbody>
          {row("Kind", `${r.kind} (${r.source})`)}
          {row("Owner", r.owner ?? "none")}
          {row("Connector's tier", r.sensitivity ?? "none")}
          {row(
            "Classifier",
            r.classification
              ? `${r.classification.sensitivity ?? "not placed yet"}${r.classification.by ? `, by ${r.classification.by}` : ""}: ${r.classification.reason}`
              : "not classified yet",
          )}
          {row("Labels", r.labels.join(", ") || "none")}
          {row("Folders above", r.folders.join(" › ") || "none")}
          {row("Assigned to work on it", r.assignees.join(", ") || "nobody")}
          {row("Sharing", r.sharing == null ? <Hidden /> : <Code block>{prettyJson(r.sharing)}</Code>)}
          {r.url &&
            row(
              "Open",
              <Anchor href={r.url} target="_blank" rel="noreferrer" size="sm">
                {r.url}
              </Anchor>,
            )}
        </Table.Tbody>
      </Table>
    </Section>
  );
}

/** One labeled row of a resource's details. */
function row(label: string, value: ReactNode) {
  return (
    <Table.Tr>
      <Table.Td w={180}>
        <Text size="sm" fw={500}>
          {label}
        </Text>
      </Table.Td>
      <Table.Td>{value}</Table.Td>
    </Table.Tr>
  );
}
