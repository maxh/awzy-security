import {
  Badge,
  Button,
  Center,
  Drawer,
  Group,
  Loader,
  Select,
  Stack,
  Table,
  Text,
  TextInput,
  Title,
} from "@mantine/core";
import { type FormEvent, Suspense, useState } from "react";
import { graphql, useLazyLoadQuery, usePaginationFragment } from "react-relay";

import type { DecisionsPage_decisions$key } from "./__generated__/DecisionsPage_decisions.graphql";
import type { DecisionFilter, DecisionsPageQuery } from "./__generated__/DecisionsPageQuery.graphql";
import { DecisionDetail } from "./DecisionDetail";
import { entityLabel, formatTime, VERDICT_COLOR, verdictLabel } from "./format";

type Verdict = NonNullable<DecisionFilter["verdict"]>;
type Mode = NonNullable<DecisionFilter["mode"]>;

export function DecisionsPage() {
  const [filter, setFilter] = useState<DecisionFilter>({});
  const [selected, setSelected] = useState<string | null>(null);

  return (
    <Stack gap="md">
      <Title order={3}>Decisions</Title>
      <Filters onApply={setFilter} />
      <Suspense
        fallback={
          <Center p="xl">
            <Loader />
          </Center>
        }
      >
        <DecisionList filter={filter} onSelect={setSelected} />
      </Suspense>
      <Drawer opened={selected !== null} onClose={() => setSelected(null)} position="right" size="xl" title="Decision">
        {selected && (
          <Suspense fallback={<Loader />}>
            <DecisionDetail id={selected} />
          </Suspense>
        )}
      </Drawer>
    </Stack>
  );
}

function Filters({ onApply }: { onApply: (f: DecisionFilter) => void }) {
  const [subjectId, setSubjectId] = useState("");
  const [resourceId, setResourceId] = useState("");
  const [verdict, setVerdict] = useState<string | null>(null);
  const [mode, setMode] = useState<string | null>(null);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    onApply({
      subjectId: subjectId.trim() || null,
      resourceId: resourceId.trim() || null,
      verdict: (verdict as Verdict | null) ?? null,
      mode: (mode as Mode | null) ?? null,
    });
  };

  return (
    <form onSubmit={submit}>
      <Group align="end" gap="sm">
        <TextInput
          id="filter-subject"
          label="Subject ID"
          placeholder="alice"
          value={subjectId}
          onChange={(e) => setSubjectId(e.currentTarget.value)}
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
        <Button type="submit">Search</Button>
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
              subject {
                type
                id
              }
              onBehalfOf {
                person {
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
    return <Text c="dimmed">No decisions in the last 24 hours match these filters.</Text>;
  }

  return (
    <Stack gap="sm">
      <Table highlightOnHover striped>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>Time</Table.Th>
            <Table.Th>Subject</Table.Th>
            <Table.Th>Action</Table.Th>
            <Table.Th>Resource</Table.Th>
            <Table.Th>Verdict</Table.Th>
            <Table.Th>Policies</Table.Th>
          </Table.Tr>
        </Table.Thead>
        <Table.Tbody>
          {edges.map(({ node: d }) => (
            <Table.Tr key={d.id} onClick={() => onSelect(d.id)} style={{ cursor: "pointer" }}>
              <Table.Td style={{ whiteSpace: "nowrap" }}>{formatTime(d.at)}</Table.Td>
              <Table.Td>
                {entityLabel(d.subject)}
                {d.onBehalfOf && (
                  <Text size="xs" c="dimmed">
                    for {entityLabel(d.onBehalfOf.person)}
                  </Text>
                )}
              </Table.Td>
              <Table.Td>{d.action}</Table.Td>
              <Table.Td>{entityLabel(d.resource)}</Table.Td>
              <Table.Td>
                <Badge
                  color={VERDICT_COLOR[d.verdict as keyof typeof VERDICT_COLOR]}
                  variant={d.mode === "MONITOR" ? "outline" : "light"}
                >
                  {verdictLabel(d.verdict, d.mode, d.challenges)}
                </Badge>
              </Table.Td>
              <Table.Td>
                <Text size="sm" ff="monospace">
                  {d.policies.join(", ") || "none"}
                </Text>
              </Table.Td>
            </Table.Tr>
          ))}
        </Table.Tbody>
      </Table>
      {hasNext && (
        <Group>
          <Button variant="default" loading={isLoadingNext} onClick={() => loadNext(50)}>
            Load more
          </Button>
        </Group>
      )}
    </Stack>
  );
}
