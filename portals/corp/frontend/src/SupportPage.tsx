import { EmptyState, PageHeader, PageLoader, ShowMore, StatusBadge } from "@awzy/ui";
import { Alert, Anchor, Button, Group, Table, Text } from "@mantine/core";
import { IconLifebuoy } from "@tabler/icons-react";
import { getRouteApi, Link, useRouter } from "@tanstack/react-router";
import { Suspense, useState } from "react";
import { graphql, type PreloadedQuery, useMutation, usePaginationFragment, usePreloadedQuery } from "react-relay";

import type { SupportPage_sessions$key } from "./__generated__/SupportPage_sessions.graphql";
import type { SupportPagePaginationQuery } from "./__generated__/SupportPagePaginationQuery.graphql";
import type { SupportPageQuery } from "./__generated__/SupportPageQuery.graphql";
import type { SupportPageSignInMutation } from "./__generated__/SupportPageSignInMutation.graphql";
import { formatTime, supportStatus } from "./format";
import { AskForSupportButton } from "./SupportSection";

const route = getRouteApi("/support");

/**
 * Every install's support sessions (ADR 0095), newest first: what's waiting on a client, what's
 * open to sign in to now, and what ended. A session can be asked for here or on the install's page.
 */
export function SupportPage() {
  const queryRef = route.useLoaderData();
  const router = useRouter();
  return (
    <>
      <PageHeader
        title="Support"
        description="Support sessions across installs: waiting on the client's admins, approved and open to sign in to, and past ones."
        actions={<AskForSupportButton onRequested={() => void router.invalidate()} />}
      />
      <Suspense fallback={<PageLoader />}>
        <Sessions queryRef={queryRef} />
      </Suspense>
    </>
  );
}

function Sessions({ queryRef }: { queryRef: PreloadedQuery<SupportPageQuery> }) {
  const query = usePreloadedQuery<SupportPageQuery>(
    graphql`
      query SupportPageQuery {
        ...SupportPage_sessions
      }
    `,
    queryRef,
  );
  const { data, loadNext, hasNext, isLoadingNext } = usePaginationFragment<
    SupportPagePaginationQuery,
    SupportPage_sessions$key
  >(
    graphql`
      fragment SupportPage_sessions on Query
      @argumentDefinitions(first: { type: "Int", defaultValue: 20 }, after: { type: "String" })
      @refetchable(queryName: "SupportPagePaginationQuery") {
        supportSessions(first: $first, after: $after) @connection(key: "SupportPage_supportSessions") {
          edges {
            node {
              id
              status
              requestedBy
              areas
              reason
              requestedAt
              expiresAt
              delivered
              install {
                id
                client
                environment
              }
            }
          }
        }
      }
    `,
    query,
  );
  const sessions = data.supportSessions.edges.map((e) => e.node);
  const [signIn, signingIn] = useMutation<SupportPageSignInMutation>(graphql`
    mutation SupportPageSignInMutation($supportSessionQid: Qid!) {
      signInToSupportSession(supportSessionQid: $supportSessionQid) {
        signInUrl
      }
    }
  `);
  const [error, setError] = useState<string | null>(null);
  const open = (id: string) =>
    signIn({
      variables: { supportSessionQid: id },
      onCompleted: (res) => {
        window.open(res.signInToSupportSession.signInUrl, "_blank", "noopener");
      },
      onError: (e) => setError(e.message),
    });

  if (sessions.length === 0) {
    return (
      <EmptyState
        icon={<IconLifebuoy size={28} stroke={1.5} />}
        title="No support sessions yet"
        description="Ask an install's admins for one with Ask for support."
      />
    );
  }
  return (
    <>
      {error && (
        <Alert color="red" variant="light" withCloseButton onClose={() => setError(null)} mb="md">
          {error}
        </Alert>
      )}
      <Table>
        <Table.Thead>
          <Table.Tr>
            <Table.Th>Install</Table.Th>
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
                  {s.install ? (
                    <Anchor component={Link} to="/installs/$installId" params={{ installId: s.install.id }} size="sm">
                      {s.install.client}/{s.install.environment}
                    </Anchor>
                  ) : (
                    <Text size="sm" c="dimmed">
                      retired install
                    </Text>
                  )}
                </Table.Td>
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
      <ShowMore hasMore={hasNext} loading={isLoadingNext} onMore={() => loadNext(50)} />
    </>
  );
}
