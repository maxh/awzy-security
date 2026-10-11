import { EmptyState, PageHeader, PageLoader, Section, StatusBadge, type Tone } from "@awzy/ui";
import { Alert, Button, Checkbox, Group, Modal, NumberInput, Stack, Table, Text } from "@mantine/core";
import { IconLifebuoy } from "@tabler/icons-react";
import { Suspense, useState } from "react";
import { graphql, useLazyLoadQuery, useRelayEnvironment } from "react-relay";

import type { SupportPageBannerQuery } from "./__generated__/SupportPageBannerQuery.graphql";
import type { SupportPageDecideMutation } from "./__generated__/SupportPageDecideMutation.graphql";
import type { SupportPageEndMutation } from "./__generated__/SupportPageEndMutation.graphql";
import type { SupportPageQuery, SupportSessionStatus } from "./__generated__/SupportPageQuery.graphql";
import { formatTime } from "./format";
import { mutate, passChallenge } from "./securityKeyChallenge";
import { webAuthnMessage } from "./webauthn";

const decide = graphql`
  mutation SupportPageDecideMutation($input: DecideSupportSessionInput!) {
    decideSupportSession(input: $input) {
      supportSession {
        id
        status
      }
      challengeQid
    }
  }
`;

const end = graphql`
  mutation SupportPageEndMutation($id: ID!) {
    endSupportSession(id: $id) {
      id
      status
    }
  }
`;

const STATUS: Record<string, { label: string; tone: Tone }> = {
  REQUESTED: { label: "Waiting for you", tone: "warning" },
  APPROVED: { label: "Active", tone: "danger" },
  REJECTED: { label: "Rejected", tone: "neutral" },
  ENDED: { label: "Ended", tone: "neutral" },
  EXPIRED: { label: "Expired", tone: "neutral" },
};
const statusOf = (s: SupportSessionStatus) => STATUS[s] ?? { label: s, tone: "neutral" as Tone };

/**
 * Support sessions (ADR 0095): awzy staff ask, in awzy's corp portal, to view named areas of this
 * portal; an admin approves or rejects each here, and can end an active one.
 */
export function SupportPage() {
  const [fetchKey, setFetchKey] = useState(0);
  return (
    <>
      <PageHeader
        title="Support"
        description="awzy support asks here before looking inside your portal. An approved session is read-only, limited to the areas you choose, ends on its own, and records everything it opens."
      />
      <Suspense fallback={<PageLoader />}>
        <Sessions fetchKey={fetchKey} onChanged={() => setFetchKey((k) => k + 1)} />
      </Suspense>
    </>
  );
}

type Session = SupportPageQuery["response"]["supportSessions"][number];

function Sessions({ fetchKey, onChanged }: { fetchKey: number; onChanged: () => void }) {
  const data = useLazyLoadQuery<SupportPageQuery>(
    graphql`
      query SupportPageQuery {
        canDecide: viewerCan(action: ADMINISTER, area: ENFORCEMENT)
        supportSessions {
          id
          status
          requestedBy
          areas
          reason
          durationMinutes
          requestedAt
          approvedAreas
          expiresAt
          decidedBy {
            label
          }
          endedBy {
            label
          }
          actionCount
        }
      }
    `,
    {},
    { fetchKey, fetchPolicy: "network-only" },
  );
  const environment = useRelayEnvironment();
  const [approving, setApproving] = useState<Session | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reject = async (s: Session) => {
    try {
      await mutate<SupportPageDecideMutation>(environment, decide, { input: { id: s.id, approve: false } });
      onChanged();
    } catch (e) {
      setError((e as Error).message);
    }
  };
  const stop = async (s: Session) => {
    try {
      await mutate<SupportPageEndMutation>(environment, end, { id: s.id });
      onChanged();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  if (data.supportSessions.length === 0) {
    return (
      <EmptyState
        icon={<IconLifebuoy size={22} />}
        title="No support sessions"
        description="When awzy support asks to look at something in your portal, the request shows up here for you to decide."
      />
    );
  }
  return (
    <Stack gap="md">
      {error && (
        <Alert color="red" variant="light" withCloseButton onClose={() => setError(null)}>
          {error}
        </Alert>
      )}
      <Section flush>
        <Table>
          <Table.Thead>
            <Table.Tr>
              <Table.Th>Asked by</Table.Th>
              <Table.Th>Why</Table.Th>
              <Table.Th>Areas</Table.Th>
              <Table.Th>Status</Table.Th>
              <Table.Th />
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {data.supportSessions.map((s) => (
              <Table.Tr key={s.id}>
                <Table.Td>
                  <Text size="sm">{s.requestedBy}</Text>
                  <Text size="xs" c="dimmed">
                    {s.requestedAt ? formatTime(s.requestedAt) : ""}, for {s.durationMinutes} min
                  </Text>
                </Table.Td>
                <Table.Td maw={320}>
                  <Text size="sm">{s.reason}</Text>
                </Table.Td>
                <Table.Td>
                  <Text size="sm">{(s.status === "REQUESTED" ? s.areas : s.approvedAreas).join(", ") || "none"}</Text>
                </Table.Td>
                <Table.Td>
                  <StatusBadge tone={statusOf(s.status).tone}>{statusOf(s.status).label}</StatusBadge>
                  <Text size="xs" c="dimmed">
                    {s.status === "APPROVED" && s.expiresAt ? `until ${formatTime(s.expiresAt)}, ` : ""}
                    {s.decidedBy ? `by ${s.decidedBy.label}` : ""}
                    {s.endedBy ? `, ended by ${s.endedBy.label}` : ""}
                    {s.actionCount > 0 ? `, ${s.actionCount} opened` : ""}
                  </Text>
                </Table.Td>
                <Table.Td>
                  {data.canDecide && s.status === "REQUESTED" && (
                    <Group gap="xs" justify="flex-end" wrap="nowrap">
                      <Button size="xs" variant="default" onClick={() => void reject(s)}>
                        Reject
                      </Button>
                      <Button size="xs" onClick={() => setApproving(s)}>
                        Approve…
                      </Button>
                    </Group>
                  )}
                  {data.canDecide && s.status === "APPROVED" && (
                    <Group justify="flex-end">
                      <Button size="xs" color="red" variant="light" onClick={() => void stop(s)}>
                        End now
                      </Button>
                    </Group>
                  )}
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      </Section>
      {approving && (
        <Approve
          session={approving}
          onClose={() => setApproving(null)}
          onApproved={() => {
            setApproving(null);
            onChanged();
          }}
        />
      )}
    </Stack>
  );
}

/** Approving: which of the areas asked for, and for how long. Takes the admin's security key. */
function Approve({ session, onClose, onApproved }: { session: Session; onClose: () => void; onApproved: () => void }) {
  const environment = useRelayEnvironment();
  const [areas, setAreas] = useState<string[]>([...session.areas]);
  const [minutes, setMinutes] = useState<number>(session.durationMinutes);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const approve = async () => {
    setSaving(true);
    try {
      const input = { input: { id: session.id, approve: true, areas, minutes } };
      let { decideSupportSession: res } = await mutate<SupportPageDecideMutation>(environment, decide, input);
      if (res.challengeQid) {
        await passChallenge(environment, res.challengeQid);
        ({ decideSupportSession: res } = await mutate<SupportPageDecideMutation>(environment, decide, input));
      }
      if (!res.supportSession) throw new Error("awzy still asks for a challenge before approving; try again.");
      onApproved();
    } catch (e) {
      setError(webAuthnMessage(e));
    } finally {
      setSaving(false);
    }
  };
  return (
    <Modal opened onClose={onClose} title={<Text fw={600}>Approve {session.requestedBy}'s support session</Text>}>
      <Stack gap="sm">
        <Text size="sm">
          They'll be able to view what you tick, read-only, until the time runs out or you end it. Everything they open
          is recorded here.
        </Text>
        <Checkbox.Group label="Areas" value={areas} onChange={setAreas}>
          <Stack gap={6} mt="xs">
            {session.areas.map((a) => (
              <Checkbox key={a} value={a} label={a} />
            ))}
          </Stack>
        </Checkbox.Group>
        <NumberInput
          label="Minutes"
          min={1}
          max={session.durationMinutes}
          value={minutes}
          onChange={(v) => setMinutes(typeof v === "number" ? v : session.durationMinutes)}
          description={`At most the ${session.durationMinutes} asked for.`}
        />
        {error && (
          <Alert color="red" variant="light">
            {error}
          </Alert>
        )}
        <Group justify="flex-end">
          <Button variant="default" onClick={onClose}>
            Cancel
          </Button>
          <Button loading={saving} disabled={areas.length === 0} onClick={() => void approve()}>
            Approve with your security key
          </Button>
        </Group>
      </Stack>
    </Modal>
  );
}

/** For awzy staff in a support session: what it allows, and until when. Nothing for anyone else. */
export function SupportBanner() {
  return (
    <Suspense fallback={null}>
      <Banner />
    </Suspense>
  );
}

function Banner() {
  const { viewerSupport: s } = useLazyLoadQuery<SupportPageBannerQuery>(
    graphql`
      query SupportPageBannerQuery {
        viewerSupport {
          staff
          areas
          expiresAt
        }
      }
    `,
    {},
  );
  if (!s) return null;
  return (
    <Alert color="orange" variant="light" mb="md" icon={<IconLifebuoy size={18} />}>
      awzy support session for {s.staff}: read-only, {s.areas.join(", ")}, until {formatTime(s.expiresAt)}. Everything
      you open is recorded for the client.
    </Alert>
  );
}
