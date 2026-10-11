import { Alert, Badge, Button, Card, Group, Modal, Select, Stack, Table, Text, TextInput, Title } from "@mantine/core";
import { useState } from "react";
import { graphql, useLazyLoadQuery, useMutation } from "react-relay";

import type { ChallengesAdminContainMutation } from "./__generated__/ChallengesAdminContainMutation.graphql";
import type { ChallengesAdminDecideMutation } from "./__generated__/ChallengesAdminDecideMutation.graphql";
import type { ChallengesAdminLiftMutation } from "./__generated__/ChallengesAdminLiftMutation.graphql";
import type { ChallengesAdminQuery } from "./__generated__/ChallengesAdminQuery.graphql";
import type { ChallengesPageQuery } from "./__generated__/ChallengesPageQuery.graphql";
import { ResetKeys } from "./SecurityKeys";
import { LINKED, linkedChallenge, requestLabel } from "./challengeLabels";
import { entityLabel, formatTime } from "./format";

/** Approvals and containments, for those who administer challenges. */
export function Admin({ fetchKey, onChanged }: { fetchKey: number; onChanged: () => void }) {
  const data = useLazyLoadQuery<ChallengesAdminQuery>(
    graphql`
      query ChallengesAdminQuery {
        pending: challenges(status: PENDING) {
          id
          principal {
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
          principal {
            type
            id
          }
          reason
          createdBy {
            label
          }
          createdAt
          monitor
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
          <Stack gap="sm">
            {approvals.map((c) => (
              <Approval key={c.id} challenge={c} onChanged={onChanged} />
            ))}
          </Stack>
        )}
      </Stack>
      <Containments containments={data.containments} onChanged={onChanged} />
      <ResetKeys />
    </Stack>
  );
}

type AdminPending = ChallengesAdminQuery["response"]["pending"][number];
type ToApprove = ChallengesPageQuery["response"]["toApprove"][number];

export function Approval({ challenge: c, onChanged }: { challenge: AdminPending | ToApprove; onChanged: () => void }) {
  const [commit, isSaving] = useMutation<ChallengesAdminDecideMutation>(graphql`
    mutation ChallengesAdminDecideMutation($input: DecideChallengeInput!) {
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
      variables: { input: { challengeQid: c.id, approve, note } },
      onCompleted: () => {
        setError(null);
        onChanged();
      },
      onError: (e) => setError(e.message),
    });

  return (
    <Card withBorder id={c.id} style={c.id === linkedChallenge() ? LINKED : undefined}>
      <Stack gap="xs">
        <Group justify="space-between" gap="xs">
          <Text fw={600}>{requestLabel(c)}</Text>
          <Text size="sm" c="dimmed">
            {c.createdAt ? formatTime(c.createdAt) : ""}
          </Text>
        </Group>
        {c.justification && (
          <Text size="sm" c="dimmed">
            “{c.justification}”
          </Text>
        )}
        <Group gap="xs" align="flex-end">
          <TextInput
            size="xs"
            placeholder="Note (optional)"
            value={note}
            onChange={(e) => setNote(e.currentTarget.value)}
            style={{ flex: "1 1 200px" }}
          />
          <Group gap="xs" wrap="nowrap">
            <Button size="xs" color="teal" loading={isSaving} onClick={() => decide(true)}>
              Approve
            </Button>
            <Button size="xs" color="red" variant="light" loading={isSaving} onClick={() => decide(false)}>
              Reject
            </Button>
          </Group>
        </Group>
        {error && (
          <Text size="sm" c="red">
            {error}
          </Text>
        )}
      </Stack>
    </Card>
  );
}

type ContainmentRow = ChallengesAdminQuery["response"]["containments"][number];

function Containments({
  containments,
  onChanged,
}: {
  containments: ReadonlyArray<ContainmentRow>;
  onChanged: () => void;
}) {
  const [contain, isContaining] = useMutation<ChallengesAdminContainMutation>(graphql`
    mutation ChallengesAdminContainMutation($input: ContainPrincipalInput!) {
      containPrincipal(input: $input) {
        containment {
          id
        }
      }
    }
  `);
  const [lift, isLifting] = useMutation<ChallengesAdminLiftMutation>(graphql`
    mutation ChallengesAdminLiftMutation($input: LiftContainmentInput!) {
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
      variables: { input: { principal: { type: type ?? "ServiceAccount", id: id.trim() }, reason } },
      onCompleted: () => {
        setId("");
        setReason("");
        setError(null);
        onChanged();
      },
      onError: (e) => setError(e.message),
    });
  // A monitored containment (ADR 0066) that the security team agrees with: contain for real.
  const confirm = (c: ContainmentRow) =>
    c.principal &&
    contain({
      variables: { input: { principal: c.principal, reason: `Confirmed on review: ${c.reason}` } },
      onCompleted: () => {
        setError(null);
        onChanged();
      },
      onError: (e) => setError(e.message),
    });
  const submitLift = () =>
    lifting &&
    lift({
      variables: { input: { containmentQid: lifting.id, reason: liftReason } },
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
        A contained principal is denied everything until a person lifts it. Every PDP applies a change within seconds.
        In monitor mode it's recorded as a would-deny. While automated containments are in monitor, the ones
        awzy-reasoner would have made are listed as "Would contain": contain the person, or dismiss the record.
      </Text>
      {containments.length === 0 ? (
        <Text c="dimmed">No one is contained.</Text>
      ) : (
        <Table.ScrollContainer minWidth={560}>
          <Table>
            <Table.Tbody>
              {containments.map((c) => (
                <Table.Tr key={c.id}>
                  <Table.Td>
                    {c.principal ? entityLabel(c.principal) : ""}
                    {c.monitor && (
                      <Badge ml="xs" variant="light" color="gray">
                        Would contain
                      </Badge>
                    )}
                  </Table.Td>
                  <Table.Td>{c.reason}</Table.Td>
                  <Table.Td c="dimmed" style={{ whiteSpace: "nowrap" }}>
                    {c.createdBy?.label}
                    {c.createdAt ? `, ${formatTime(c.createdAt)}` : ""}
                  </Table.Td>
                  <Table.Td>
                    <Group gap="xs" wrap="nowrap">
                      {c.monitor && (
                        <Button size="xs" color="red" variant="light" loading={isContaining} onClick={() => confirm(c)}>
                          Contain
                        </Button>
                      )}
                      <Button size="xs" variant="light" onClick={() => setLifting(c)}>
                        {c.monitor ? "Dismiss" : "Lift"}
                      </Button>
                    </Group>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Table.ScrollContainer>
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
        title={lifting?.principal ? `Lift the containment of ${entityLabel(lifting.principal)}?` : "Lift containment?"}
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
