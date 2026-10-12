import { Section, ShowMore, StatusBadge } from "@awzy/ui";
import {
  Alert,
  Button,
  Group,
  Modal,
  MultiSelect,
  NumberInput,
  Select,
  Stack,
  Table,
  Text,
  TextInput,
  Textarea,
} from "@mantine/core";
import { IconLifebuoy } from "@tabler/icons-react";
import { Suspense, useState } from "react";
import { graphql, useLazyLoadQuery, useMutation, usePaginationFragment } from "react-relay";
import { ConnectionHandler } from "relay-runtime";

import type { SupportSectionInstallsQuery } from "./__generated__/SupportSectionInstallsQuery.graphql";
import type { SupportSection_install$key } from "./__generated__/SupportSection_install.graphql";
import type { SupportSectionPaginationQuery } from "./__generated__/SupportSectionPaginationQuery.graphql";
import type { SupportSectionQuery } from "./__generated__/SupportSectionQuery.graphql";
import type { SupportSectionRequestMutation } from "./__generated__/SupportSectionRequestMutation.graphql";
import type { SupportSectionSignInMutation } from "./__generated__/SupportSectionSignInMutation.graphql";
import { formatTime, supportStatus } from "./format";

const AREAS = ["decisions", "policies", "enforcement", "directory", "challenges", "investigations"];

/**
 * Support sessions (ADR 0095): ask an install's admins to let you view parts of their client
 * portal, read-only, for a while. They decide in their portal; once approved, sign in from here.
 */
export function SupportSection({ installId }: { installId: string }) {
  return (
    <Suspense
      fallback={
        <Section title="Support sessions">
          <Text size="sm">Loading…</Text>
        </Section>
      }
    >
      <Sessions installId={installId} />
    </Suspense>
  );
}

function Sessions({ installId }: { installId: string }) {
  const data = useLazyLoadQuery<SupportSectionQuery>(
    graphql`
      query SupportSectionQuery($id: ID!) {
        node(id: $id) {
          ...SupportSection_install @alias(as: "install")
        }
      }
    `,
    { id: installId },
    { fetchPolicy: "store-and-network" },
  );
  return data.node?.install ? <SessionList installId={installId} install={data.node.install} /> : null;
}

/** The connection a new request is prepended to, so the list shows it without a refetch. */
export const SUPPORT_SESSIONS_CONNECTION = "SupportSection_supportSessions";

function SessionList({ installId, install }: { installId: string; install: SupportSection_install$key }) {
  const { data, loadNext, hasNext, isLoadingNext } = usePaginationFragment<
    SupportSectionPaginationQuery,
    SupportSection_install$key
  >(
    graphql`
      fragment SupportSection_install on Install
      @argumentDefinitions(first: { type: "Int", defaultValue: 5 }, after: { type: "String" })
      @refetchable(queryName: "SupportSectionPaginationQuery") {
        supportSessions(first: $first, after: $after) @connection(key: "SupportSection_supportSessions") {
          edges {
            node {
              id
              status
              requestedBy
              areas
              reason
              portalUrl
              requestedAt
              expiresAt
              delivered
            }
          }
        }
      }
    `,
    install,
  );
  const sessions = data.supportSessions.edges.map((e) => e.node);
  const [signIn, signingIn] = useMutation<SupportSectionSignInMutation>(graphql`
    mutation SupportSectionSignInMutation($supportSessionQid: Qid!) {
      signInToSupportSession(supportSessionQid: $supportSessionQid) {
        signInUrl
      }
    }
  `);
  const [error, setError] = useState<string | null>(null);
  const open = (id: string) =>
    signIn({
      variables: { supportSessionQid: id },
      onCompleted: (res, errors) => {
        if (errors?.[0]) setError(errors[0].message);
        else window.open(res.signInToSupportSession.signInUrl, "_blank", "noopener");
      },
      onError: (e) => setError(e.message),
    });
  return (
    <Section
      title="Support sessions"
      description="Ask the client's admins to let you view parts of their portal, read-only, for a while. They approve or reject it in their portal, and everything you open there is recorded for them."
      actions={<AskForSupportButton installId={installId} lastPortal={sessions[0]?.portalUrl} />}
    >
      <Stack gap="md">
        {sessions.length === 0 && (
          <Text size="sm" c="dimmed">
            No support sessions for this install yet.
          </Text>
        )}
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
                const status = supportStatus(s.status);
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
        <ShowMore hasMore={hasNext} loading={isLoadingNext} onMore={() => loadNext(20)} />
      </Stack>
    </Section>
  );
}

/**
 * Opens the form to ask for a support session: for `installId`, or for an install picked in the
 * form when there's none.
 */
export function AskForSupportButton({
  installId,
  lastPortal,
  onRequested,
}: {
  installId?: string;
  lastPortal?: string;
  onRequested?: () => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button variant="default" leftSection={<IconLifebuoy size={16} />} onClick={() => setOpen(true)}>
        Ask for support
      </Button>
      <Modal
        opened={open}
        onClose={() => setOpen(false)}
        size="lg"
        title={<Text fw={600}>Ask for a support session</Text>}
      >
        <Text size="sm" c="dimmed" mb="sm">
          The client's admins approve or reject it in their portal. Once approved, you view the areas you asked for,
          read-only, and everything you open is recorded for them.
        </Text>
        <Suspense fallback={<Text size="sm">Loading…</Text>}>
          <RequestForm
            installId={installId}
            lastPortal={lastPortal}
            onCancel={() => setOpen(false)}
            onRequested={() => {
              setOpen(false);
              onRequested?.();
            }}
          />
        </Suspense>
      </Modal>
    </>
  );
}

function InstallSelect({ value, onChange }: { value: string | null; onChange: (id: string | null) => void }) {
  const data = useLazyLoadQuery<SupportSectionInstallsQuery>(
    graphql`
      query SupportSectionInstallsQuery {
        installs(first: 200) {
          edges {
            node {
              id
              client
              environment
            }
          }
        }
      }
    `,
    {},
  );
  const options = data.installs.edges.map(({ node }) => ({
    value: node.id,
    label: `${node.client} / ${node.environment}`,
  }));
  return (
    <Select label="Install" data={options} value={value} onChange={onChange} searchable placeholder="Which install" />
  );
}

function RequestForm({
  installId: given,
  lastPortal,
  onCancel,
  onRequested,
}: {
  installId: string | undefined;
  lastPortal: string | undefined;
  onCancel: () => void;
  onRequested: () => void;
}) {
  const [installId, setInstallId] = useState<string | null>(given ?? null);
  const [areas, setAreas] = useState<string[]>([]);
  const [reason, setReason] = useState("");
  const [minutes, setMinutes] = useState<number>(60);
  const [portalUrl, setPortalUrl] = useState(lastPortal ?? "https://");
  const [error, setError] = useState<string | null>(null);
  const [request, requesting] = useMutation<SupportSectionRequestMutation>(graphql`
    mutation SupportSectionRequestMutation($input: RequestSupportSessionInput!, $connections: [ID!]!) {
      requestSupportSession(input: $input) @prependNode(connections: $connections, edgeTypeName: "SupportSessionEdge") {
        id
        status
        requestedBy
        areas
        reason
        portalUrl
        requestedAt
        expiresAt
        delivered
      }
    }
  `);
  const submit = () => {
    if (!installId) return;
    request({
      variables: {
        input: { installQid: installId, areas, reason, durationMinutes: minutes, portalUrl },
        connections: [ConnectionHandler.getConnectionID(installId, SUPPORT_SESSIONS_CONNECTION)],
      },
      onCompleted: (_, errors) => {
        if (errors?.[0]) {
          setError(errors[0].message);
          return;
        }
        onRequested();
      },
      onError: (e) => setError(e.message),
    });
  };
  return (
    <Stack gap="xs">
      {!given && <InstallSelect value={installId} onChange={setInstallId} />}
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
        <Button variant="default" onClick={onCancel}>
          Cancel
        </Button>
        <Button loading={requesting} disabled={!installId || areas.length === 0 || !reason.trim()} onClick={submit}>
          Ask for a session
        </Button>
      </Group>
    </Stack>
  );
}
