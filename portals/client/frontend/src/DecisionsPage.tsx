import { EmptyState, oneOf, PageHeader, PageLoader, Section } from "@awzy/ui";
import { Button, Drawer, Group, Select, Table, Text, TextInput } from "@mantine/core";
import { IconListSearch, IconSearch } from "@tabler/icons-react";
import { type FormEvent, Suspense, useState } from "react";
import { graphql, useLazyLoadQuery, usePaginationFragment } from "react-relay";

import type { DecisionsPage_decisions$key } from "./__generated__/DecisionsPage_decisions.graphql";
import type { DecisionFilter, DecisionsPageQuery } from "./__generated__/DecisionsPageQuery.graphql";
import { DecisionDetail } from "./DecisionDetail";
import { Entity, Time, VerdictBadge } from "./format";

type Verdict = NonNullable<DecisionFilter["verdict"]>;
type Mode = NonNullable<DecisionFilter["mode"]>;

const VERDICTS: readonly Verdict[] = ["ALLOW", "DENY", "CHALLENGE"];
const MODES: readonly Mode[] = ["MONITOR", "ENFORCE"];

export function DecisionsPage() {
  const [filter, setFilter] = useState<DecisionFilter>({});
  const [selected, setSelected] = useState<string | null>(null);

  return (
    <>
      <PageHeader
        title="Decisions"
        description="Every allow, deny, and challenge the PDPs decided, with the policies behind each one."
      />
      <Section flush>
        <Filters onApply={setFilter} />
        <Suspense fallback={<PageLoader />}>
          <DecisionList filter={filter} onSelect={setSelected} />
        </Suspense>
      </Section>
      <Drawer
        opened={selected !== null}
        onClose={() => setSelected(null)}
        position="right"
        size="xl"
        title={<Text fw={600}>Decision</Text>}
      >
        {selected && (
          <Suspense fallback={<PageLoader />}>
            <DecisionDetail id={selected} />
          </Suspense>
        )}
      </Drawer>
    </>
  );
}

function Filters({ onApply }: { onApply: (f: DecisionFilter) => void }) {
  const [principalId, setPrincipalId] = useState("");
  const [resourceId, setResourceId] = useState("");
  const [verdict, setVerdict] = useState<string | null>(null);
  const [mode, setMode] = useState<string | null>(null);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    onApply({
      principalId: principalId.trim() || null,
      resourceId: resourceId.trim() || null,
      verdict: oneOf(VERDICTS, verdict),
      mode: oneOf(MODES, mode),
    });
  };

  return (
    <form onSubmit={submit} style={{ borderBottom: "1px solid var(--mantine-color-default-border)" }}>
      <Group align="end" gap="sm" p="md">
        <TextInput
          id="filter-principal"
          label="Principal ID"
          placeholder="alice"
          value={principalId}
          onChange={(e) => setPrincipalId(e.currentTarget.value)}
        />
        <TextInput
          id="filter-resource"
          label="Resource ID"
          placeholder="design-doc"
          value={resourceId}
          onChange={(e) => setResourceId(e.currentTarget.value)}
        />
        <Select
          id="filter-verdict"
          label="Verdict"
          placeholder="Any"
          clearable
          data={[
            { value: "ALLOW", label: "Allow" },
            { value: "DENY", label: "Deny" },
            { value: "CHALLENGE", label: "Challenge" },
          ]}
          value={verdict}
          onChange={setVerdict}
        />
        <Select
          id="filter-mode"
          label="Mode"
          placeholder="Any"
          clearable
          data={[
            { value: "MONITOR", label: "Monitored" },
            { value: "ENFORCE", label: "Enforced" },
          ]}
          value={mode}
          onChange={setMode}
        />
        <Button type="submit" variant="light" leftSection={<IconSearch size={16} />}>
          Search
        </Button>
      </Group>
    </form>
  );
}

function DecisionList({ filter, onSelect }: { filter: DecisionFilter; onSelect: (id: string) => void }) {
  const query = useLazyLoadQuery<DecisionsPageQuery>(
    graphql`
      query DecisionsPageQuery($filter: DecisionFilter) {
        ...DecisionsPage_decisions @arguments(filter: $filter)
      }
    `,
    { filter },
    { fetchPolicy: "store-and-network" },
  );
  const { data, loadNext, hasNext, isLoadingNext } = usePaginationFragment<
    DecisionsPageQuery,
    DecisionsPage_decisions$key
  >(
    graphql`
      fragment DecisionsPage_decisions on Query
      @argumentDefinitions(
        filter: { type: "DecisionFilter" }
        first: { type: "Int", defaultValue: 50 }
        after: { type: "String" }
      )
      @refetchable(queryName: "DecisionsPagePaginationQuery") {
        decisions(filter: $filter, first: $first, after: $after) @connection(key: "DecisionsPage_decisions") {
          edges {
            node {
              id
              at
              principal {
                type
                id
              }
              onBehalfOf {
                human {
                  type
                  id
                }
              }
              action
              resource {
                type
                id
              }
              verdict
              mode
              challenges
              policies
            }
          }
        }
      }
    `,
    query,
  );

  const edges = data.decisions.edges;
  if (edges.length === 0) {
    return (
      <EmptyState
        icon={<IconListSearch size={22} />}
        title="No matching decisions"
        description="Nothing in the last 24 hours matches these filters."
      />
    );
  }

  return (
    <>
      <Table.ScrollContainer minWidth={900}>
        <Table style={{ whiteSpace: "nowrap" }}>
          <Table.Thead>
            <Table.Tr>
              <Table.Th>Time</Table.Th>
              <Table.Th>Verdict</Table.Th>
              <Table.Th>Principal</Table.Th>
              <Table.Th>Action</Table.Th>
              <Table.Th>Resource</Table.Th>
              <Table.Th>Policies</Table.Th>
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {edges.map(({ node: d }) => (
              <Table.Tr key={d.id} onClick={() => onSelect(d.id)} style={{ cursor: "pointer" }}>
                <Table.Td>
                  <Time at={d.at} />
                </Table.Td>
                <Table.Td>
                  <VerdictBadge verdict={d.verdict} mode={d.mode} challenges={d.challenges} />
                </Table.Td>
                <Table.Td>
                  <Entity e={d.principal} />
                  {d.onBehalfOf && (
                    <Text size="xs" c="dimmed">
                      for {d.onBehalfOf.human.id}
                    </Text>
                  )}
                </Table.Td>
                <Table.Td>{d.action}</Table.Td>
                <Table.Td>
                  <Entity e={d.resource} />
                </Table.Td>
                <Table.Td ff="monospace" fz="xs" c={d.policies.length ? undefined : "dimmed"}>
                  {d.policies.join(", ") || "none"}
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      </Table.ScrollContainer>
      {hasNext && (
        <Group justify="center" p="md">
          <Button variant="default" loading={isLoadingNext} onClick={() => loadNext(50)}>
            Load more
          </Button>
        </Group>
      )}
    </>
  );
}
