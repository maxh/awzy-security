import { EmptyState, PageHeader, PageLoader, Section, StatusBadge, type Tone } from "@awzy/ui";
import { Alert, Button, Drawer, Group, List, SegmentedControl, Stack, Table, Text, Textarea } from "@mantine/core";
import { IconSearch } from "@tabler/icons-react";
import { Suspense, useState } from "react";
import { graphql, useLazyLoadQuery, useMutation } from "react-relay";

import type { InvestigationsPageQuery, InvestigationStatus } from "./__generated__/InvestigationsPageQuery.graphql";
import type { InvestigationsPageCloseMutation } from "./__generated__/InvestigationsPageCloseMutation.graphql";
import { Entity, formatTime, Time } from "./format";

/** awzy-reasoner's investigations (ADR 0071): what a model found, and what awzy's checks let it do. */
export function InvestigationsPage() {
  const [filter, setFilter] = useState<"NEEDS_REVIEW" | "ALL">("NEEDS_REVIEW");
  const [fetchKey, setFetchKey] = useState(0);
  return (
    <>
      <PageHeader
        title="Investigations"
        description="awzy-reasoner looks into the principals it flags with a model on your own cloud. Its findings are checked in code before they can do anything (ADR 0067, 0071)."
      />
      <Stack gap="md">
        <SegmentedControl
          w="fit-content"
          value={filter}
          onChange={setFilter}
          data={[
            { label: "Needs review", value: "NEEDS_REVIEW" },
            { label: "All", value: "ALL" },
          ]}
        />
        <Suspense fallback={<PageLoader />}>
          <Investigations
            status={filter === "ALL" ? null : filter}
            fetchKey={fetchKey}
            onChanged={() => setFetchKey((k) => k + 1)}
          />
        </Suspense>
      </Stack>
    </>
  );
}

const VERDICT_TONE: Record<string, Tone> = {
  MALICIOUS: "danger",
  SUSPICIOUS: "warning",
  BENIGN: "success",
  INCONCLUSIVE: "neutral",
};

const words = (s: string) => s.charAt(0) + s.slice(1).toLowerCase().replace("_", " ");

function Investigations({
  status,
  fetchKey,
  onChanged,
}: {
  status: InvestigationStatus | null;
  fetchKey: number;
  onChanged: () => void;
}) {
  const data = useLazyLoadQuery<InvestigationsPageQuery>(
    graphql`
      query InvestigationsPageQuery($status: InvestigationStatus) {
        canClose: viewerCan(action: ADMINISTER, area: INVESTIGATIONS)
        investigations(status: $status, first: 100) {
          id
          principal {
            type
            id
          }
          trigger
          floor
          status
          verdict
          risk
          summary
          evidence {
            evidenceId
            why
          }
          containment
          notes
          model
          inputTokens
          outputTokens
          finishedAt
          closedBy {
            label
          }
          closedAt
          closeReason
        }
      }
    `,
    { status },
    { fetchKey, fetchPolicy: "store-and-network" },
  );
  const [open, setOpen] = useState<string | null>(null);
  const selected = data.investigations.find((i) => i.id === open) ?? null;

  if (data.investigations.length === 0) {
    return (
      <EmptyState
        icon={<IconSearch size={22} />}
        title={status ? "Nothing waiting for review" : "No investigations yet"}
        description="awzy-reasoner investigates principals whose risk is elevated or who had a challenge rejected, once investigations are turned on."
      />
    );
  }

  return (
    <Section title={status ? "Needs review" : "All investigations"} flush>
      <Table.ScrollContainer minWidth={760}>
        <Table highlightOnHover style={{ cursor: "pointer" }}>
          <Table.Thead>
            <Table.Tr>
              <Table.Th>Finished</Table.Th>
              <Table.Th>Principal</Table.Th>
              <Table.Th>Verdict</Table.Th>
              <Table.Th>Risk</Table.Th>
              <Table.Th>Containment</Table.Th>
              <Table.Th>Status</Table.Th>
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {data.investigations.map((i) => (
              <Table.Tr key={i.id} onClick={() => setOpen(i.id)}>
                <Table.Td>
                  <Time at={i.finishedAt} />
                </Table.Td>
                <Table.Td>
                  <Entity e={i.principal} />
                </Table.Td>
                <Table.Td>
                  <StatusBadge tone={VERDICT_TONE[i.verdict] ?? "neutral"}>{words(i.verdict)}</StatusBadge>
                </Table.Td>
                <Table.Td>
                  {i.risk}
                  {i.risk > i.floor ? (
                    <Text span size="xs" c="dimmed">
                      {" "}
                      (scores said {i.floor})
                    </Text>
                  ) : null}
                </Table.Td>
                <Table.Td>{i.containment === "NONE" ? "" : words(i.containment)}</Table.Td>
                <Table.Td>
                  <StatusBadge tone={i.status === "NEEDS_REVIEW" ? "warning" : "neutral"}>
                    {words(i.status)}
                  </StatusBadge>
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      </Table.ScrollContainer>
      <Drawer
        opened={selected !== null}
        onClose={() => setOpen(null)}
        position="right"
        size="lg"
        title={<Text fw={600}>Investigation</Text>}
      >
        {selected && <Detail investigation={selected} canClose={data.canClose} onClosed={onChanged} />}
      </Drawer>
    </Section>
  );
}

type Row = InvestigationsPageQuery["response"]["investigations"][number];

function Detail({
  investigation: i,
  canClose,
  onClosed,
}: {
  investigation: Row;
  canClose: boolean;
  onClosed: () => void;
}) {
  const [commit, isClosing] = useMutation<InvestigationsPageCloseMutation>(graphql`
    mutation InvestigationsPageCloseMutation($input: CloseInvestigationInput!) {
      closeInvestigation(input: $input) {
        investigation {
          id
          status
          closedBy {
            label
          }
          closedAt
          closeReason
        }
      }
    }
  `);
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);
  const close = () =>
    commit({
      variables: { input: { investigationQid: i.id, reason } },
      onCompleted: () => {
        setReason("");
        setError(null);
        onClosed();
      },
      onError: (e) => setError(e.message),
    });

  return (
    <Stack gap="md">
      <Group gap="sm">
        <Entity e={i.principal} />
        <StatusBadge tone={VERDICT_TONE[i.verdict] ?? "neutral"}>{words(i.verdict)}</StatusBadge>
        <Text size="sm">risk {i.risk}</Text>
      </Group>
      <Text size="sm">{i.summary || "The model gave no summary."}</Text>
      <Text size="xs" c="dimmed">
        Opened because: {i.trigger}
      </Text>

      <div>
        <Text fw={600} size="sm" mb={4}>
          Evidence it relied on
        </Text>
        {i.evidence.length === 0 ? (
          <Text size="sm" c="dimmed">
            None that awzy's records back.
          </Text>
        ) : (
          <List size="sm" spacing={4}>
            {i.evidence.map((e) => (
              <List.Item key={e.evidenceId}>
                <Text span ff="monospace" fz="xs">
                  {e.evidenceId}
                </Text>
                {e.why ? `: ${e.why}` : ""}
              </List.Item>
            ))}
          </List>
        )}
      </div>

      {i.notes.length > 0 && (
        <Alert color="gray" title="What awzy's checks changed">
          <List size="sm" spacing={2}>
            {i.notes.map((n) => (
              <List.Item key={n}>{n}</List.Item>
            ))}
          </List>
        </Alert>
      )}

      <Text size="sm">
        Containment:{" "}
        {i.containment === "RECOMMEND"
          ? "recommended, and sent to automated containments (applied only when they're enforced)."
          : i.containment === "PROPOSE"
            ? "proposed for you to decide: the principal is privileged. Contain them on the Challenges page if you agree."
            : "none."}
      </Text>
      <Text size="xs" c="dimmed">
        {i.model}, {i.inputTokens.toLocaleString()} tokens in and {i.outputTokens.toLocaleString()} out
      </Text>

      {i.status === "CLOSED" ? (
        <Alert color="gray" title="Closed">
          {i.closedBy?.label ?? "Someone"}
          {i.closedAt ? `, ${formatTime(i.closedAt)}` : ""}: {i.closeReason}
        </Alert>
      ) : canClose ? (
        <Stack gap="xs">
          <Textarea
            label="Close it"
            placeholder="What your review found"
            value={reason}
            onChange={(e) => setReason(e.currentTarget.value)}
            autosize
            minRows={2}
          />
          {error && (
            <Alert color="red" title="Not closed">
              {error}
            </Alert>
          )}
          <Group justify="flex-end">
            <Button loading={isClosing} disabled={!reason.trim()} onClick={close}>
              Close investigation
            </Button>
          </Group>
        </Stack>
      ) : null}
    </Stack>
  );
}
