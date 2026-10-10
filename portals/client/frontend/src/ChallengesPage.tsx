import {
  Alert,
  Badge,
  Button,
  Card,
  Center,
  Group,
  Loader,
  Modal,
  Select,
  Stack,
  Table,
  Text,
  Textarea,
  TextInput,
  Title,
} from "@mantine/core";
import { Suspense, useState } from "react";
import { graphql, useLazyLoadQuery, useMutation } from "react-relay";

import type { ChallengesPageAdminQuery } from "./__generated__/ChallengesPageAdminQuery.graphql";
import type { ChallengesPageContainMutation } from "./__generated__/ChallengesPageContainMutation.graphql";
import type { ChallengesPageDecideMutation } from "./__generated__/ChallengesPageDecideMutation.graphql";
import type { ChallengesPageJustifyMutation } from "./__generated__/ChallengesPageJustifyMutation.graphql";
import type { ChallengesPageLiftMutation } from "./__generated__/ChallengesPageLiftMutation.graphql";
import type { ChallengesPageQuery } from "./__generated__/ChallengesPageQuery.graphql";
import { entityLabel, formatTime } from "./format";

/** The shortest justification awzy-challenge accepts. */
const MIN_JUSTIFICATION = 20;

const STATUS_COLOR = { PENDING: "yellow", PASSED: "teal", REJECTED: "red", EXPIRED: "gray" } as const;

type Ref = { readonly type: string; readonly id: string } | null | undefined;

/** "alice read Resource::board-deck", or "sales-genie for bob read …". */
function requestLabel(c: { subject: Ref; onBehalfOf: Ref; action: string; resource: Ref }): string {
  const who = c.subject ? c.subject.id : "someone";
  const forWhom = c.onBehalfOf ? ` for ${c.onBehalfOf.id}` : "";
  return `${who}${forWhom} ${c.action} ${c.resource ? entityLabel(c.resource) : ""}`;
}

function kindLabel(kind: string): string {
  return kind.toLowerCase();
}

function Loading() {
  return (
    <Center p="xl">
      <Loader />
    </Center>
  );
}

/** What happened to the justification just submitted: the request it was for, and whether that
 * passed the challenge or something else is still needed. */
type Justified = { request: string; grantExpiresAt: string | null; stillNeeded: ReadonlyArray<string> };

/** Challenges (ADR 0054): answer your own; admins approve requests and manage containments. */
export function ChallengesPage() {
  const [fetchKey, setFetchKey] = useState(0);
  // Kept outside Suspense so the refetch after submitting doesn't clear it.
  const [justified, setJustified] = useState<Justified | null>(null);
  const refresh = () => setFetchKey((k) => k + 1);
  return (
    <Stack gap="md">
      <Title order={3}>Challenges</Title>
      {justified && <JustifiedNotice justified={justified} onClose={() => setJustified(null)} />}
      <Suspense fallback={<Loading />}>
        <Challenges
          fetchKey={fetchKey}
          onChanged={refresh}
          onJustified={(j) => {
            setJustified(j);
            refresh();
          }}
        />
      </Suspense>
    </Stack>
  );
}

/** The next step after a justification: retry the request, or wait for what's left. */
function JustifiedNotice({ justified: j, onClose }: { justified: Justified; onClose: () => void }) {
  if (j.stillNeeded.length > 0) {
    return (
      <Alert color="blue" title="Justification recorded" withCloseButton onClose={onClose}>
        {j.request} still needs {j.stillNeeded.join(" and ")}. Retry it once that's done.
      </Alert>
    );
  }
  return (
    <Alert color="teal" title="Challenge passed: retry the request" withCloseButton onClose={onClose}>
      Run {j.request} again
      {j.grantExpiresAt ? ` before ${formatTime(j.grantExpiresAt)}` : ""}. awzy allows that exact request until then;
      anything else is checked as usual.
    </Alert>
  );
}

function Challenges({
  fetchKey,
  onChanged,
  onJustified,
}: {
  fetchKey: number;
  onChanged: () => void;
  onJustified: (j: Justified) => void;
}) {
  const data = useLazyLoadQuery<ChallengesPageQuery>(
    graphql`
      query ChallengesPageQuery {
        canAdminister: viewerCan(action: ADMINISTER, area: CHALLENGES)
        pending: myChallenges(status: PENDING) {
          id
          subject {
            type
            id
          }
          onBehalfOf {
            type
            id
          }
          action
          resource {
            type
            id
          }
          kinds
          passed
          expiresAt
        }
        recent: myChallenges(first: 20) {
          id
          subject {
            type
            id
          }
          onBehalfOf {
            type
            id
          }
          action
          resource {
            type
            id
          }
          status
          createdAt
          decidedBy
          grantExpiresAt
        }
      }
    `,
    {},
    { fetchKey, fetchPolicy: "store-and-network" },
  );
  const recent = data.recent.filter((c) => c.status !== "PENDING");
  // When the page loaded: a pass that has since run out no longer invites a retry.
  const [loadedAt] = useState(() => Date.now());

  return (
    <Stack gap="lg">
      <Stack gap="xs">
        <Title order={4}>Waiting for you</Title>
        {data.pending.length === 0 ? (
          <Text c="dimmed">
            Nothing to answer. When awzy challenges a request you made, or one an agent made for you, it shows up here;
            answer it, then retry the request.
          </Text>
        ) : (
          data.pending.map((c) => <PendingChallenge key={c.id} challenge={c} onJustified={onJustified} />)
        )}
      </Stack>

      {recent.length > 0 && (
        <Stack gap="xs">
          <Title order={4}>Recent</Title>
          <Table>
            <Table.Tbody>
              {recent.map((c) => (
                <Table.Tr key={c.id}>
                  <Table.Td style={{ whiteSpace: "nowrap" }}>{c.createdAt ? formatTime(c.createdAt) : ""}</Table.Td>
                  <Table.Td>{requestLabel(c)}</Table.Td>
                  <Table.Td>
                    <Badge color={STATUS_COLOR[c.status as keyof typeof STATUS_COLOR] ?? "gray"} variant="light">
                      {c.status.toLowerCase()}
                    </Badge>
                  </Table.Td>
                  <Table.Td c="dimmed">
                    {c.status === "PASSED" && c.grantExpiresAt
                      ? Date.parse(c.grantExpiresAt) > loadedAt
                        ? `retry the request before ${formatTime(c.grantExpiresAt)}`
                        : `was allowed until ${formatTime(c.grantExpiresAt)}`
                      : ""}
                    {c.status === "REJECTED" && c.decidedBy ? `rejected by ${c.decidedBy}` : ""}
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Stack>
      )}

      {data.canAdminister && (
        <Suspense fallback={<Loading />}>
          <Admin fetchKey={fetchKey} onChanged={onChanged} />
        </Suspense>
      )}
    </Stack>
  );
}

type Pending = ChallengesPageQuery["response"]["pending"][number];

function PendingChallenge({ challenge: c, onJustified }: { challenge: Pending; onJustified: (j: Justified) => void }) {
  const [commit, isSaving] = useMutation<ChallengesPageJustifyMutation>(graphql`
    mutation ChallengesPageJustifyMutation($input: JustifyChallengeInput!) {
      justifyChallenge(input: $input) {
        challenge {
          id
          status
          kinds
          passed
          grantExpiresAt
        }
      }
    }
  `);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const needs = (kind: string) => c.kinds.includes(kind as never) && !c.passed.includes(kind as never);

  const justify = () =>
    commit({
      variables: { input: { id: c.id, justification: text } },
      onCompleted: ({ justifyChallenge: { challenge: done } }) => {
        setError(null);
        onJustified({
          request: requestLabel(c),
          grantExpiresAt: done.status === "PASSED" ? (done.grantExpiresAt ?? null) : null,
          stillNeeded: done.kinds.filter((k) => !done.passed.includes(k)).map(kindLabel),
        });
      },
      onError: (e) => setError(e.message),
    });

  return (
    <Card withBorder>
      <Stack gap="sm">
        <Group justify="space-between">
          <Text fw={600}>{requestLabel(c)}</Text>
          <Text size="sm" c="dimmed">
            {c.expiresAt ? `expires ${formatTime(c.expiresAt)}` : ""}
          </Text>
        </Group>
        <Group gap="xs">
          {c.kinds.map((k) => (
            <Badge key={k} color={c.passed.includes(k) ? "teal" : "yellow"} variant="light">
              {kindLabel(k)}
              {c.passed.includes(k) ? " ✓" : ""}
            </Badge>
          ))}
        </Group>
        {needs("JUSTIFICATION") && (
          <Stack gap="xs">
            <Textarea
              label="Why do you need this access?"
              description="Recorded with the request for the security team to review."
              autosize
              minRows={2}
              value={text}
              onChange={(e) => setText(e.currentTarget.value)}
            />
            <Group>
              <Button loading={isSaving} disabled={text.trim().length < MIN_JUSTIFICATION} onClick={justify}>
                Submit justification
              </Button>
              {text.trim().length < MIN_JUSTIFICATION && (
                <Text size="sm" c="dimmed">
                  At least {MIN_JUSTIFICATION} characters.
                </Text>
              )}
            </Group>
          </Stack>
        )}
        {needs("APPROVAL") && (
          <Text size="sm">
            Waiting for approval from someone policy allows to approve it, such as the security team.
          </Text>
        )}
        {(needs("VERIFICATION") || needs("BIOMETRIC")) && (
          <Text size="sm" c="dimmed">
            Security-key and biometric checks aren't available yet, so this challenge can't be passed here.
          </Text>
        )}
        {error && (
          <Alert color="red" title="Not recorded">
            {error}
          </Alert>
        )}
      </Stack>
    </Card>
  );
}

/** Approvals and containments, for those who administer challenges. */
function Admin({ fetchKey, onChanged }: { fetchKey: number; onChanged: () => void }) {
  const data = useLazyLoadQuery<ChallengesPageAdminQuery>(
    graphql`
      query ChallengesPageAdminQuery {
        pending: challenges(status: PENDING) {
          id
          subject {
            type
            id
          }
          onBehalfOf {
            type
            id
          }
          action
          resource {
            type
            id
          }
          kinds
          passed
          answererId
          justification
          createdAt
        }
        containments {
          id
          subject {
            type
            id
          }
          reason
          createdBy
          createdAt
        }
      }
    `,
    {},
    { fetchKey, fetchPolicy: "store-and-network" },
  );
  const approvals = data.pending.filter((c) => c.kinds.includes("APPROVAL") && !c.passed.includes("APPROVAL"));

  return (
    <Stack gap="lg">
      <Stack gap="xs">
        <Title order={4}>Waiting for approval</Title>
        {approvals.length === 0 ? (
          <Text c="dimmed">No requests are waiting for approval.</Text>
        ) : (
          <Table>
            <Table.Tbody>
              {approvals.map((c) => (
                <Approval key={c.id} challenge={c} onChanged={onChanged} />
              ))}
            </Table.Tbody>
          </Table>
        )}
      </Stack>
      <Containments containments={data.containments} onChanged={onChanged} />
    </Stack>
  );
}

type AdminPending = ChallengesPageAdminQuery["response"]["pending"][number];

function Approval({ challenge: c, onChanged }: { challenge: AdminPending; onChanged: () => void }) {
  const [commit, isSaving] = useMutation<ChallengesPageDecideMutation>(graphql`
    mutation ChallengesPageDecideMutation($input: DecideChallengeInput!) {
      decideChallenge(input: $input) {
        challenge {
          id
          status
        }
      }
    }
  `);
  const [note, setNote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const decide = (approve: boolean) =>
    commit({
      variables: { input: { id: c.id, approve, note } },
      onCompleted: () => {
        setError(null);
        onChanged();
      },
      onError: (e) => setError(e.message),
    });

  return (
    <Table.Tr>
      <Table.Td style={{ whiteSpace: "nowrap" }}>{c.createdAt ? formatTime(c.createdAt) : ""}</Table.Td>
      <Table.Td>
        <Stack gap={4}>
          <Text>{requestLabel(c)}</Text>
          {c.justification && (
            <Text size="sm" c="dimmed">
              “{c.justification}”
            </Text>
          )}
          {error && (
            <Text size="sm" c="red">
              {error}
            </Text>
          )}
        </Stack>
      </Table.Td>
      <Table.Td>
        <TextInput
          size="xs"
          placeholder="Note (optional)"
          value={note}
          onChange={(e) => setNote(e.currentTarget.value)}
        />
      </Table.Td>
      <Table.Td>
        <Group gap="xs" wrap="nowrap">
          <Button size="xs" color="teal" loading={isSaving} onClick={() => decide(true)}>
            Approve
          </Button>
          <Button size="xs" color="red" variant="light" loading={isSaving} onClick={() => decide(false)}>
            Reject
          </Button>
        </Group>
      </Table.Td>
    </Table.Tr>
  );
}

type ContainmentRow = ChallengesPageAdminQuery["response"]["containments"][number];

function Containments({
  containments,
  onChanged,
}: {
  containments: ReadonlyArray<ContainmentRow>;
  onChanged: () => void;
}) {
  const [contain, isContaining] = useMutation<ChallengesPageContainMutation>(graphql`
    mutation ChallengesPageContainMutation($input: ContainAccessorInput!) {
      containAccessor(input: $input) {
        containment {
          id
        }
      }
    }
  `);
  const [lift, isLifting] = useMutation<ChallengesPageLiftMutation>(graphql`
    mutation ChallengesPageLiftMutation($input: LiftContainmentInput!) {
      liftContainment(input: $input) {
        containment {
          id
          liftedAt
        }
      }
    }
  `);
  const [type, setType] = useState<string | null>("ServiceAccount");
  const [id, setId] = useState("");
  const [reason, setReason] = useState("");
  const [lifting, setLifting] = useState<ContainmentRow | null>(null);
  const [liftReason, setLiftReason] = useState("");
  const [error, setError] = useState<string | null>(null);

  const submit = () =>
    contain({
      variables: { input: { subject: { type: type ?? "ServiceAccount", id: id.trim() }, reason } },
      onCompleted: () => {
        setId("");
        setReason("");
        setError(null);
        onChanged();
      },
      onError: (e) => setError(e.message),
    });
  const submitLift = () =>
    lifting &&
    lift({
      variables: { input: { id: lifting.id, reason: liftReason } },
      onCompleted: () => {
        setLifting(null);
        setLiftReason("");
        setError(null);
        onChanged();
      },
      onError: (e) => setError(e.message),
    });

  return (
    <Stack gap="xs">
      <Title order={4}>Containments</Title>
      <Text size="sm" c="dimmed">
        A contained accessor is denied everything until a person lifts it. Every PDP applies a change within seconds. In
        monitor mode it's recorded as a would-deny.
      </Text>
      {containments.length === 0 ? (
        <Text c="dimmed">No one is contained.</Text>
      ) : (
        <Table>
          <Table.Tbody>
            {containments.map((c) => (
              <Table.Tr key={c.id}>
                <Table.Td>{c.subject ? entityLabel(c.subject) : ""}</Table.Td>
                <Table.Td>{c.reason}</Table.Td>
                <Table.Td c="dimmed" style={{ whiteSpace: "nowrap" }}>
                  {c.createdBy}
                  {c.createdAt ? `, ${formatTime(c.createdAt)}` : ""}
                </Table.Td>
                <Table.Td>
                  <Button size="xs" variant="light" onClick={() => setLifting(c)}>
                    Lift
                  </Button>
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      )}
      <Group align="flex-end">
        <Select
          label="Contain"
          data={["ServiceAccount", "Human"]}
          value={type}
          onChange={setType}
          allowDeselect={false}
          w={170}
        />
        <TextInput label="ID" placeholder="sales-genie" value={id} onChange={(e) => setId(e.currentTarget.value)} />
        <TextInput
          label="Reason"
          placeholder="What you saw"
          style={{ flex: 1 }}
          value={reason}
          onChange={(e) => setReason(e.currentTarget.value)}
        />
        <Button color="red" loading={isContaining} disabled={!id.trim() || !reason.trim()} onClick={submit}>
          Contain
        </Button>
      </Group>
      {error && (
        <Alert color="red" title="Not done">
          {error}
        </Alert>
      )}
      <Modal
        opened={lifting !== null}
        onClose={() => setLifting(null)}
        title={lifting?.subject ? `Lift the containment of ${entityLabel(lifting.subject)}?` : "Lift containment?"}
      >
        <Stack gap="sm">
          <TextInput
            label="Reason"
            placeholder="What the review found"
            value={liftReason}
            onChange={(e) => setLiftReason(e.currentTarget.value)}
          />
          <Group justify="flex-end">
            <Button variant="default" onClick={() => setLifting(null)}>
              Cancel
            </Button>
            <Button loading={isLifting} disabled={!liftReason.trim()} onClick={submitLift}>
              Lift
            </Button>
          </Group>
        </Stack>
      </Modal>
    </Stack>
  );
}
