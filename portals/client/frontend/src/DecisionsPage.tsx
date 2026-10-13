import { EmptyState, messageOf, oneOf, PageHeader, PageLoader, Section } from "@awzy/ui";
import { Alert, Button, Drawer, Group, Menu, Select, Table, Text, TextInput } from "@mantine/core";
import { IconDownload, IconListSearch, IconSearch } from "@tabler/icons-react";
import { type SyntheticEvent, Suspense, useState } from "react";
import { fetchQuery, graphql, useLazyLoadQuery, usePaginationFragment, useRelayEnvironment } from "react-relay";

import type { DecisionsPage_decisions$key } from "./__generated__/DecisionsPage_decisions.graphql";
import type { DecisionExportFormat, DecisionsPageExportQuery } from "./__generated__/DecisionsPageExportQuery.graphql";
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
        <Export filter={filter} />
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
  const [since, setSince] = useState("");
  const [until, setUntil] = useState("");

  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    onApply({
      principalId: principalId.trim() || null,
      resourceId: resourceId.trim() || null,
      verdict: oneOf(VERDICTS, verdict),
      mode: oneOf(MODES, mode),
      since: utc(since),
      until: utc(until),
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
        <TextInput
          id="filter-since"
          label="From"
          description="Default: a day before To"
          type="datetime-local"
          value={since}
          onChange={(e) => setSince(e.currentTarget.value)}
        />
        <TextInput
          id="filter-until"
          label="To"
          description="Default: now; at most 30 days after From"
          type="datetime-local"
          value={until}
          onChange={(e) => setUntil(e.currentTarget.value)}
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
        description="Nothing in this period matches these filters. Without From and To, it's the last 24 hours."
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

/** A `datetime-local` input's value, in the viewer's time zone, as a UTC timestamp. */
function utc(local: string): string | null {
  if (!local) return null;
  const at = new Date(local);
  return Number.isNaN(at.getTime()) ? null : at.toISOString();
}

const exportQuery = graphql`
  query DecisionsPageExportQuery($filter: DecisionFilter, $format: DecisionExportFormat!) {
    decisionExport(filter: $filter, format: $format) {
      filename
      count
      truncated
      includesContext
      csv
      json
    }
  }
`;

/**
 * Downloads what the filters select, for an auditor to keep. The backend records each export, and
 * includes the request context only for those who may see it.
 */
function Export({ filter }: { filter: DecisionFilter }) {
  const environment = useRelayEnvironment();
  const [exporting, setExporting] = useState(false);
  const [result, setResult] = useState<{ count: number; truncated: boolean; includesContext: boolean } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = (format: DecisionExportFormat) => {
    setExporting(true);
    setError(null);
    setResult(null);
    fetchQuery<DecisionsPageExportQuery>(environment, exportQuery, { filter, format }, { fetchPolicy: "network-only" })
      .toPromise()
      .then((data) => {
        const file = data?.decisionExport;
        if (!file) return;
        // A byte-order mark, so a spreadsheet reads the CSV as UTF-8.
        const blob =
          typeof file.csv === "string"
            ? new Blob(["\uFEFF", file.csv], { type: "text/csv" })
            : new Blob([file.json ?? ""], { type: "application/json" });
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = file.filename;
        a.click();
        URL.revokeObjectURL(url);
        setResult({ count: file.count, truncated: file.truncated, includesContext: file.includesContext });
      })
      .catch((e: unknown) => setError(messageOf(e)))
      .finally(() => setExporting(false));
  };

  return (
    <Group px="md" py="xs" gap="sm" style={{ borderBottom: "1px solid var(--mantine-color-default-border)" }}>
      <Menu position="bottom-start">
        <Menu.Target>
          <Button size="xs" variant="default" loading={exporting} leftSection={<IconDownload size={14} />}>
            Export
          </Button>
        </Menu.Target>
        <Menu.Dropdown>
          <Menu.Item onClick={() => run("CSV")}>CSV, for a spreadsheet</Menu.Item>
          <Menu.Item onClick={() => run("JSON")}>JSON</Menu.Item>
        </Menu.Dropdown>
      </Menu>
      <Text size="xs" c="dimmed">
        Up to 50,000 decisions matching these filters. Each export is recorded.
      </Text>
      {result && (
        <Text size="xs">
          Exported {result.count.toLocaleString()} decisions
          {result.includesContext
            ? ", with their request context"
            : ", without request context, which a policy hides from you"}
          .
        </Text>
      )}
      {result?.truncated && (
        <Alert color="yellow" p="xs" w="100%">
          More decisions matched than one export holds. Narrow the filters or the period to get the rest.
        </Alert>
      )}
      {error && (
        <Alert color="red" p="xs" w="100%">
          {error}
        </Alert>
      )}
    </Group>
  );
}
