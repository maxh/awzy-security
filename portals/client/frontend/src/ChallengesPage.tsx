import { PageHeader } from "@awzy/ui";
import { Alert, Badge, Button, Card, Center, Group, Loader, Stack, Table, Text, Textarea, Title } from "@mantine/core";
import { getRouteApi, useRouter } from "@tanstack/react-router";
import { Suspense, useEffect, useMemo, useRef, useState } from "react";
import {
  graphql,
  type PreloadedQuery,
  useMutation,
  usePreloadedQuery,
  useRelayEnvironment,
  useSubscription,
} from "react-relay";
import type { GraphQLSubscriptionConfig } from "relay-runtime";

import type { ChallengesPageBeginVerificationMutation } from "./__generated__/ChallengesPageBeginVerificationMutation.graphql";
import type { ChallengesPageChallengeUpdatedSubscription } from "./__generated__/ChallengesPageChallengeUpdatedSubscription.graphql";
import type { ChallengesPageContainmentsChangedSubscription } from "./__generated__/ChallengesPageContainmentsChangedSubscription.graphql";
import type { ChallengesPageFinishVerificationMutation } from "./__generated__/ChallengesPageFinishVerificationMutation.graphql";
import type { ChallengesPageJustifyMutation } from "./__generated__/ChallengesPageJustifyMutation.graphql";
import type { ChallengesPageQuery } from "./__generated__/ChallengesPageQuery.graphql";
import { Admin, Approval } from "./ChallengesAdmin";
import { SecurityKeys } from "./SecurityKeys";
import { LINKED, linkedChallenge, kindLabel, requestLabel, type Ref } from "./challengeLabels";
import { formatTime } from "./format";
import { mutate } from "./securityKeyChallenge";
import { getAssertion, webAuthnAvailable, webAuthnMessage } from "./webauthn";

/** The shortest justification awzy-challenge accepts. */
const MIN_JUSTIFICATION = 20;

const STATUS_COLOR: Record<string, string> = { PENDING: "yellow", PASSED: "teal", REJECTED: "red", EXPIRED: "gray" };

/** The notice for a challenge after an answer, from the challenge as awzy now has it. */
function answeredFrom(
  what: Answered["what"],
  c: { principal: Ref; onBehalfOf: Ref; action: string; resource: Ref },
  done: {
    readonly status: string;
    readonly kinds: ReadonlyArray<string>;
    readonly passed: ReadonlyArray<string>;
    readonly grantExpiresAt?: string | null;
  },
): Answered {
  return {
    what,
    request: requestLabel(c),
    grantExpiresAt: done.status === "PASSED" ? (done.grantExpiresAt ?? null) : null,
    stillNeeded: done.kinds.filter((k) => !done.passed.includes(k)).map(kindLabel),
  };
}

function Loading() {
  return (
    <Center p="xl">
      <Loader />
    </Center>
  );
}

/** What happened to the answer just given (a justification or a security key): the request it
 * was for, and whether that passed the challenge or something else is still needed. */
type Answered = {
  what: "Justification" | "Security key";
  request: string;
  grantExpiresAt: string | null;
  stillNeeded: ReadonlyArray<string>;
};

const beginVerification = graphql`
  mutation ChallengesPageBeginVerificationMutation($input: BeginChallengeVerificationInput!) {
    beginChallengeVerification(input: $input) {
      ceremonyQid
      optionsJson
    }
  }
`;

const finishVerification = graphql`
  mutation ChallengesPageFinishVerificationMutation($input: FinishChallengeVerificationInput!) {
    finishChallengeVerification(input: $input) {
      challenge {
        id
        status
        kinds
        passed
        grantExpiresAt
      }
    }
  }
`;

const route = getRouteApi("/challenges");

/** Challenges (ADR 0054): answer your own; admins approve requests and manage containments. */
export function ChallengesPage() {
  const queryRef = route.useLoaderData();
  const router = useRouter();
  // Only for the admin section's query, which loads only for those who administer challenges.
  const [fetchKey, setFetchKey] = useState(0);
  // Kept outside Suspense so the refetch after submitting doesn't clear it.
  const [answered, setAnswered] = useState<Answered | null>(null);
  const refresh = () => {
    void router.invalidate();
    setFetchKey((k) => k + 1);
  };
  useLive(refresh);
  return (
    <Stack gap="md">
      <PageHeader
        title="Challenges"
        description="Requests waiting for a person to verify, justify, or approve them, and the people and agents contained."
      />
      {answered && <AnsweredNotice answered={answered} onClose={() => setAnswered(null)} />}
      <Suspense fallback={<Loading />}>
        <Challenges
          queryRef={queryRef}
          fetchKey={fetchKey}
          onChanged={refresh}
          onAnswered={(a) => {
            setAnswered(a);
            refresh();
          }}
        />
      </Suspense>
    </Stack>
  );
}

/**
 * Refreshes the page when a challenge opens or changes, or a containment is added or lifted, so
 * no one has to reload to see a request approved. A burst of changes refreshes once. Containments
 * are only for those who administer challenges; for anyone else that subscription is refused,
 * which is fine.
 */
function useLive(refresh: () => void) {
  const latest = useRef(refresh);
  useEffect(() => {
    latest.current = refresh;
  });
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const changed = useMemo(
    () => () => {
      clearTimeout(timer.current);
      timer.current = setTimeout(() => latest.current(), 300);
    },
    [],
  );
  const challenges = useMemo<GraphQLSubscriptionConfig<ChallengesPageChallengeUpdatedSubscription>>(
    () => ({
      subscription: graphql`
        subscription ChallengesPageChallengeUpdatedSubscription {
          challengeUpdated {
            id
          }
        }
      `,
      variables: {},
      onNext: changed,
      onError: () => {},
    }),
    [changed],
  );
  const containments = useMemo<GraphQLSubscriptionConfig<ChallengesPageContainmentsChangedSubscription>>(
    () => ({
      subscription: graphql`
        subscription ChallengesPageContainmentsChangedSubscription {
          containmentsChanged {
            id
          }
        }
      `,
      variables: {},
      onNext: changed,
      onError: () => {},
    }),
    [changed],
  );
  useSubscription(challenges);
  useSubscription(containments);
}

function AnsweredNotice({ answered: j, onClose }: { answered: Answered; onClose: () => void }) {
  if (j.stillNeeded.length > 0) {
    return (
      <Alert color="blue" title={`${j.what} recorded`} withCloseButton onClose={onClose}>
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
  queryRef,
  fetchKey,
  onChanged,
  onAnswered,
}: {
  queryRef: PreloadedQuery<ChallengesPageQuery>;
  fetchKey: number;
  onChanged: () => void;
  onAnswered: (a: Answered) => void;
}) {
  const data = usePreloadedQuery<ChallengesPageQuery>(
    graphql`
      query ChallengesPageQuery {
        canAdminister: viewerCan(action: ADMINISTER, area: CHALLENGES)
        pending: myChallenges(status: PENDING) {
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
          expiresAt
        }
        recent: myChallenges(first: 20) {
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
          status
          createdAt
          decidedBy {
            label
          }
          grantExpiresAt
        }
        mySecurityKeys {
          id
          name
          createdAt
          lastUsedAt
        }
        toApprove: challengesToApprove {
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
      }
    `,
    queryRef,
  );
  // A notification's link names one challenge: bring it into view once it's on the page.
  const linked = linkedChallenge();
  const linkedShown =
    linked !== null && (data.pending.some((c) => c.id === linked) || data.toApprove.some((c) => c.id === linked));
  useEffect(() => {
    if (linked && linkedShown) document.getElementById(linked)?.scrollIntoView({ block: "center" });
  }, [linked, linkedShown]);
  const recent = data.recent.filter((c) => c.status !== "PENDING");
  const linkedRecent = linked && !linkedShown ? data.recent.find((c) => c.id === linked) : undefined;
  const hasKeys = data.mySecurityKeys.length > 0;
  // When the page loaded: a pass that has since run out no longer invites a retry.
  const [loadedAt] = useState(() => Date.now());

  return (
    <Stack gap="lg">
      {linked && !linkedShown && (
        <Alert color="gray" title="That challenge isn't waiting for you">
          {linkedRecent
            ? `It's ${linkedRecent.status.toLowerCase()}${linkedRecent.decidedBy ? `, by ${linkedRecent.decidedBy.label}` : ""}.`
            : "It was answered or ran out, or it's for someone else."}
        </Alert>
      )}
      <Stack gap="xs">
        <Title order={4}>Waiting for you</Title>
        {data.pending.length === 0 ? (
          <Text c="dimmed">
            Nothing to answer. When awzy challenges a request you made, or one an agent made for you, it shows up here;
            answer it, then retry the request.
          </Text>
        ) : (
          data.pending.map((c) => (
            <div key={c.id} id={c.id} style={c.id === linkedChallenge() ? LINKED : undefined}>
              <PendingChallenge challenge={c} hasKeys={hasKeys} onAnswered={onAnswered} />
            </div>
          ))
        )}
      </Stack>

      {/* Those who administer challenges see every approval below; others, the ones policy lets them give. */}
      {!data.canAdminister && data.toApprove.length > 0 && (
        <Stack gap="xs">
          <Title order={4}>Waiting for your approval</Title>
          <Stack gap="sm">
            {data.toApprove.map((c) => (
              <Approval key={c.id} challenge={c} onChanged={onChanged} />
            ))}
          </Stack>
        </Stack>
      )}

      {recent.length > 0 && (
        <Stack gap="xs">
          <Title order={4}>Recent</Title>
          <Table.ScrollContainer minWidth={560}>
            <Table>
              <Table.Tbody>
                {recent.map((c) => (
                  <Table.Tr key={c.id}>
                    <Table.Td style={{ whiteSpace: "nowrap" }}>{c.createdAt ? formatTime(c.createdAt) : ""}</Table.Td>
                    <Table.Td>{requestLabel(c)}</Table.Td>
                    <Table.Td>
                      <Badge color={STATUS_COLOR[c.status] ?? "gray"} variant="light">
                        {c.status.toLowerCase()}
                      </Badge>
                    </Table.Td>
                    <Table.Td c="dimmed">
                      {c.status === "PASSED" && c.grantExpiresAt
                        ? Date.parse(c.grantExpiresAt) > loadedAt
                          ? `retry the request before ${formatTime(c.grantExpiresAt)}`
                          : `was allowed until ${formatTime(c.grantExpiresAt)}`
                        : ""}
                      {c.status === "REJECTED" && c.decidedBy ? `rejected by ${c.decidedBy.label}` : ""}
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </Table.ScrollContainer>
        </Stack>
      )}

      <SecurityKeys keys={data.mySecurityKeys} onChanged={onChanged} />

      {data.canAdminister && (
        <Suspense fallback={<Loading />}>
          <Admin fetchKey={fetchKey} onChanged={onChanged} />
        </Suspense>
      )}
    </Stack>
  );
}

type Pending = ChallengesPageQuery["response"]["pending"][number];

function PendingChallenge({
  challenge: c,
  hasKeys,
  onAnswered,
}: {
  challenge: Pending;
  hasKeys: boolean;
  onAnswered: (a: Answered) => void;
}) {
  const environment = useRelayEnvironment();
  const [verifying, setVerifying] = useState(false);
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
  const needs = (kind: Pending["kinds"][number]) => c.kinds.includes(kind) && !c.passed.includes(kind);

  const justify = () =>
    commit({
      variables: { input: { challengeQid: c.id, justification: text } },
      onCompleted: ({ justifyChallenge: { challenge: done } }) => {
        setError(null);
        onAnswered(answeredFrom("Justification", c, done));
      },
      onError: (e) => setError(e.message),
    });

  const verify = async () => {
    setVerifying(true);
    try {
      const { beginChallengeVerification: v } = await mutate<ChallengesPageBeginVerificationMutation>(
        environment,
        beginVerification,
        { input: { challengeQid: c.id } },
      );
      const assertion = await getAssertion(v.ceremonyQid, v.optionsJson);
      const { finishChallengeVerification: done } = await mutate<ChallengesPageFinishVerificationMutation>(
        environment,
        finishVerification,
        { input: { assertion } },
      );
      setError(null);
      onAnswered(answeredFrom("Security key", c, done.challenge));
    } catch (e) {
      setError(webAuthnMessage(e));
    } finally {
      setVerifying(false);
    }
  };

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
        {needs("VERIFICATION") &&
          (!webAuthnAvailable() ? (
            <Text size="sm" c="dimmed">
              This browser can't use security keys. Open the portal in one that can to answer this.
            </Text>
          ) : hasKeys ? (
            <Group>
              <Button loading={verifying} onClick={verify}>
                Use security key
              </Button>
              <Text size="sm" c="dimmed">
                Touch your key when it blinks.
              </Text>
            </Group>
          ) : (
            <Text size="sm">Add a security key below, then come back to answer this with it.</Text>
          ))}
        {needs("BIOMETRIC") && (
          <Text size="sm" c="dimmed">
            Biometric checks aren't available yet, so this challenge can't be passed here.
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
