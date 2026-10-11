import { Section, StatusBadge, type Tone } from "@awzy/ui";
import { Alert, Button, Group, MultiSelect, NumberInput, Stack, Table, Text, TextInput, Textarea } from "@mantine/core";
import { Suspense, useState } from "react";
import { graphql, useLazyLoadQuery, useMutation } from "react-relay";

import type { SupportSectionQuery } from "./__generated__/SupportSectionQuery.graphql";
import type { SupportSectionRequestMutation } from "./__generated__/SupportSectionRequestMutation.graphql";
import type { SupportSectionSignInMutation } from "./__generated__/SupportSectionSignInMutation.graphql";
import { formatTime } from "./format";

const AREAS = ["decisions", "policies", "enforcement", "directory", "challenges", "investigations"];

const STATUS: Record<string, { label: string; tone: Tone }> = {
  REQUESTED: { label: "Waiting for the client", tone: "warning" },
  APPROVED: { label: "Approved", tone: "success" },
  REJECTED: { label: "Rejected", tone: "neutral" },
  ENDED: { label: "Ended by the client", tone: "neutral" },
  EXPIRED: { label: "Expired", tone: "neutral" },
};

/**
 * Support sessions (ADR 0095): ask an install's admins to let you view parts of their client
 * portal, read-only, for a while. They decide in their portal; once approved, sign in from here.
 */
export function SupportSection({ installId }: { installId: string }) {
  const [fetchKey, setFetchKey] = useState(0);
  return (
    <Section
      title="Support sessions"
      description="Ask the client's admins to let you view parts of their portal, read-only, for a while. They approve or reject it in their portal, and everything you open there is recorded for them."
    >
      <Suspense fallback={<Text size="sm">Loading…</Text>}>
        <Sessions installId={installId} fetchKey={fetchKey} onChanged={() => setFetchKey((k) => k + 1)} />
      </Suspense>
    </Section>
  );
}

function Sessions({ installId, fetchKey, onChanged }: { installId: string; fetchKey: number; onChanged: () => void }) {
  const data = useLazyLoadQuery<SupportSectionQuery>(
    graphql`
      query SupportSectionQuery($id: ID!) {
        node(id: $id) {
          ... on Install {
            supportSessions {
              id
              status
              requestedBy
              areas
              reason
              durationMinutes
              portalUrl
              requestedAt
              expiresAt
              delivered
            }
          }
        }
      }
    `,
    { id: installId },
    { fetchKey, fetchPolicy: "network-only" },
  );
  const sessions = data.node?.supportSessions ?? [];
  const [signIn, signingIn] = useMutation<SupportSectionSignInMutation>(graphql`
    mutation SupportSectionSignInMutation($id: ID!) {
      signInToSupportSession(id: $id) {
        signInUrl
      }
    }
  `);
  const [error, setError] = useState<string | null>(null);
  const open = (id: string) =>
    signIn({
      variables: { id },
      onCompleted: (res, errors) => {
        if (errors?.[0]) setError(errors[0].message);
        else window.open(res.signInToSupportSession.signInUrl, "_blank", "noopener");
      },
      onError: (e) => setError(e.message),
    });
  return (
    <Stack gap="md">
      <RequestForm installId={installId} lastPortal={sessions[0]?.portalUrl} onRequested={onChanged} />
      {error && (
        <Alert color="red" variant="light" withCloseButton onClose={() => setError(null)}>
          {error}
        </Alert>
      )}
      {sessions.length > 0 && (
        <Table>
          <Table.Thead>
            <Table.Tr>
              <Table.Th>Asked</Table.Th>
              <Table.Th>Areas</Table.Th>
              <Table.Th>Status</Table.Th>
              <Table.Th />
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {sessions.map((s) => {
              const status = STATUS[s.status] ?? { label: s.status, tone: "neutral" as Tone };
              return (
                <Table.Tr key={s.id}>
                  <Table.Td>
                    <Text size="sm">
                      {s.requestedBy}, {formatTime(s.requestedAt)}
                    </Text>
                    <Text size="xs" c="dimmed">
                      {s.reason}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    <Text size="sm">{s.areas.join(", ")}</Text>
                  </Table.Td>
                  <Table.Td>
                    <StatusBadge tone={status.tone}>{status.label}</StatusBadge>
                    <Text size="xs" c="dimmed">
                      {s.status === "REQUESTED" && !s.delivered ? "not delivered yet" : ""}
                      {s.status === "APPROVED" && s.expiresAt ? `until ${formatTime(s.expiresAt)}` : ""}
                    </Text>
                  </Table.Td>
                  <Table.Td>
                    {s.status === "APPROVED" && (
                      <Group justify="flex-end">
                        <Button size="xs" loading={signingIn} onClick={() => open(s.id)}>
                          Sign in
                        </Button>
                      </Group>
                    )}
                  </Table.Td>
                </Table.Tr>
              );
            })}
          </Table.Tbody>
        </Table>
      )}
    </Stack>
  );
}

function RequestForm({
  installId,
  lastPortal,
  onRequested,
}: {
  installId: string;
  lastPortal: string | undefined;
  onRequested: () => void;
}) {
  const [areas, setAreas] = useState<string[]>([]);
  const [reason, setReason] = useState("");
  const [minutes, setMinutes] = useState<number>(60);
  const [portalUrl, setPortalUrl] = useState(lastPortal ?? "https://");
  const [error, setError] = useState<string | null>(null);
  const [request, requesting] = useMutation<SupportSectionRequestMutation>(graphql`
    mutation SupportSectionRequestMutation($input: RequestSupportSessionInput!) {
      requestSupportSession(input: $input) {
        id
      }
    }
  `);
  const submit = () =>
    request({
      variables: { input: { installId, areas, reason, durationMinutes: minutes, portalUrl } },
      onCompleted: (_, errors) => {
        if (errors?.[0]) {
          setError(errors[0].message);
          return;
        }
        setError(null);
        setAreas([]);
        setReason("");
        onRequested();
      },
      onError: (e) => setError(e.message),
    });
  return (
    <Stack gap="xs">
      <Group grow align="flex-start">
        <MultiSelect label="Areas" data={AREAS} value={areas} onChange={setAreas} placeholder="What you need to see" />
        <NumberInput
          label="Minutes"
          min={1}
          max={480}
          value={minutes}
          onChange={(v) => setMinutes(typeof v === "number" ? v : 60)}
        />
        <TextInput label="Client portal" value={portalUrl} onChange={(e) => setPortalUrl(e.currentTarget.value)} />
      </Group>
      <Textarea
        label="Why"
        description="The client's admins read this before deciding."
        value={reason}
        onChange={(e) => setReason(e.currentTarget.value)}
        autosize
        minRows={2}
      />
      {error && (
        <Alert color="red" variant="light">
          {error}
        </Alert>
      )}
      <Group justify="flex-end">
        <Button loading={requesting} disabled={areas.length === 0 || !reason.trim()} onClick={submit}>
          Ask for a session
        </Button>
      </Group>
    </Stack>
  );
}
