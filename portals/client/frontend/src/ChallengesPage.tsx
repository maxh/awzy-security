import { PageHeader } from "@awzy/ui";
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
import { Suspense, useEffect, useMemo, useRef, useState } from "react";
import {
  commitMutation,
  fetchQuery,
  graphql,
  useLazyLoadQuery,
  useMutation,
  useRelayEnvironment,
  useSubscription,
} from "react-relay";
import type { Environment, GraphQLSubscriptionConfig, GraphQLTaggedNode, MutationParameters } from "relay-runtime";

import type { ChallengesPageAdminQuery } from "./__generated__/ChallengesPageAdminQuery.graphql";
import type { ChallengesPageContainMutation } from "./__generated__/ChallengesPageContainMutation.graphql";
import type { ChallengesPageDecideMutation } from "./__generated__/ChallengesPageDecideMutation.graphql";
import type { ChallengesPageJustifyMutation } from "./__generated__/ChallengesPageJustifyMutation.graphql";
import type { ChallengesPageLiftMutation } from "./__generated__/ChallengesPageLiftMutation.graphql";
import type { ChallengesPageQuery } from "./__generated__/ChallengesPageQuery.graphql";
import type { ChallengesPageBeginKeyChangeMutation } from "./__generated__/ChallengesPageBeginKeyChangeMutation.graphql";
import type { ChallengesPageBeginRegistrationMutation } from "./__generated__/ChallengesPageBeginRegistrationMutation.graphql";
import type { ChallengesPageChallengeUpdatedSubscription } from "./__generated__/ChallengesPageChallengeUpdatedSubscription.graphql";
import type { ChallengesPageContainmentsChangedSubscription } from "./__generated__/ChallengesPageContainmentsChangedSubscription.graphql";
import type { ChallengesPageBeginVerificationMutation } from "./__generated__/ChallengesPageBeginVerificationMutation.graphql";
import type { ChallengesPageFinishRegistrationMutation } from "./__generated__/ChallengesPageFinishRegistrationMutation.graphql";
import type { ChallengesPageFinishVerificationMutation } from "./__generated__/ChallengesPageFinishVerificationMutation.graphql";
import type { ChallengesPageKeysOfQuery } from "./__generated__/ChallengesPageKeysOfQuery.graphql";
import type { ChallengesPageRemoveKeyMutation } from "./__generated__/ChallengesPageRemoveKeyMutation.graphql";
import type { ChallengesPageResetKeyMutation } from "./__generated__/ChallengesPageResetKeyMutation.graphql";
import { entityLabel, formatTime } from "./format";
import { createCredential, getAssertion, webAuthnAvailable, webAuthnMessage } from "./webauthn";

/** The shortest justification awzy-challenge accepts. */
const MIN_JUSTIFICATION = 20;

const STATUS_COLOR = { PENDING: "yellow", PASSED: "teal", REJECTED: "red", EXPIRED: "gray" } as const;

type Ref = { readonly type: string; readonly id: string } | null | undefined;

/** "alice read Resource::board-deck", or "sales-genie for bob read …". */
function requestLabel(c: { principal: Ref; onBehalfOf: Ref; action: string; resource: Ref }): string {
  const who = c.principal ? c.principal.id : "someone";
  const forWhom = c.onBehalfOf ? ` for ${c.onBehalfOf.id}` : "";
  return `${who}${forWhom} ${c.action} ${c.resource ? entityLabel(c.resource) : ""}`;
}

function kindLabel(kind: string): string {
  return kind.toLowerCase();
}

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

/** A Relay mutation as a promise, for flows that chain several around a browser prompt. */
function mutate<T extends MutationParameters>(
  environment: Environment,
  mutation: GraphQLTaggedNode,
  variables: T["variables"],
): Promise<T["response"]> {
  return new Promise((resolve, reject) =>
    commitMutation<T>(environment, {
      mutation,
      variables,
      onCompleted: (response, errors) => {
        const first = errors?.[0];
        if (first) reject(new Error(first.message));
        else resolve(response);
      },
      onError: reject,
    }),
  );
}

const beginKeyChange = graphql`
  mutation ChallengesPageBeginKeyChangeMutation {
    beginSecurityKeyChange {
      ceremonyQid
      optionsJson
    }
  }
`;

const beginRegistration = graphql`
  mutation ChallengesPageBeginRegistrationMutation($input: BeginSecurityKeyRegistrationInput!) {
    beginSecurityKeyRegistration(input: $input) {
      ceremonyQid
      optionsJson
    }
  }
`;

const finishRegistration = graphql`
  mutation ChallengesPageFinishRegistrationMutation($input: FinishSecurityKeyRegistrationInput!) {
    finishSecurityKeyRegistration(input: $input) {
      securityKey {
        id
        name
      }
    }
  }
`;

const removeKey = graphql`
  mutation ChallengesPageRemoveKeyMutation($input: RemoveSecurityKeyInput!) {
    removeSecurityKey(input: $input) {
      securityKey {
        id
      }
    }
  }
`;

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

/** An assertion from one of the signed-in person's keys, authorizing a change to their keys. */
async function authorizeKeyChange(environment: Environment) {
  const { beginSecurityKeyChange: c } = await mutate<ChallengesPageBeginKeyChangeMutation>(
    environment,
    beginKeyChange,
    {},
  );
  return getAssertion(c.ceremonyQid, c.optionsJson);
}

/** Challenges (ADR 0054): answer your own; admins approve requests and manage containments. */
export function ChallengesPage() {
  const [fetchKey, setFetchKey] = useState(0);
  // Kept outside Suspense so the refetch after submitting doesn't clear it.
  const [answered, setAnswered] = useState<Answered | null>(null);
  const refresh = () => setFetchKey((k) => k + 1);
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

/** The next step after an answer: retry the request, or wait for what's left. */
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
  fetchKey,
  onChanged,
  onAnswered,
}: {
  fetchKey: number;
  onChanged: () => void;
  onAnswered: (a: Answered) => void;
}) {
  const data = useLazyLoadQuery<ChallengesPageQuery>(
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
      }
    `,
    {},
    { fetchKey, fetchPolicy: "store-and-network" },
  );
  const recent = data.recent.filter((c) => c.status !== "PENDING");
  const hasKeys = data.mySecurityKeys.length > 0;
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
          data.pending.map((c) => (
            <PendingChallenge key={c.id} challenge={c} hasKeys={hasKeys} onAnswered={onAnswered} />
          ))
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
                    {c.status === "REJECTED" && c.decidedBy ? `rejected by ${c.decidedBy.label}` : ""}
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
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
  const needs = (kind: string) => c.kinds.includes(kind as never) && !c.passed.includes(kind as never);

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

type Key = ChallengesPageQuery["response"]["mySecurityKeys"][number];

/** The signed-in person's security keys. The first is added on their sign-in alone; adding or
 * removing any other takes a touch of a key they already have (ADR 0059). */
function SecurityKeys({ keys, onChanged }: { keys: ReadonlyArray<Key>; onChanged: () => void }) {
  const environment = useRelayEnvironment();
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (step: () => Promise<void>) => {
    setBusy(true);
    try {
      await step();
      setError(null);
      onChanged();
    } catch (e) {
      setError(webAuthnMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const add = () =>
    run(async () => {
      const authorization = keys.length > 0 ? await authorizeKeyChange(environment) : null;
      const { beginSecurityKeyRegistration: r } = await mutate<ChallengesPageBeginRegistrationMutation>(
        environment,
        beginRegistration,
        { input: { authorization } },
      );
      const created = await createCredential(r.optionsJson);
      await mutate<ChallengesPageFinishRegistrationMutation>(environment, finishRegistration, {
        input: { ceremonyQid: r.ceremonyQid, name: name.trim(), ...created },
      });
      setName("");
    });

  const remove = (key: Key) =>
    run(async () => {
      const authorization = await authorizeKeyChange(environment);
      await mutate<ChallengesPageRemoveKeyMutation>(environment, removeKey, {
        input: { securityKeyQid: key.id, authorization },
      });
    });

  return (
    <Stack gap="xs">
      <Title order={4}>Your security keys</Title>
      <Text size="sm" c="dimmed">
        Some challenges ask you to touch a security key.{" "}
        {keys.length > 0
          ? "Adding or removing a key takes a touch of one you already have."
          : "Add your first one here; after that, adding or removing a key takes a touch of one you have."}
      </Text>
      {keys.length > 0 && (
        <Table>
          <Table.Tbody>
            {keys.map((k) => (
              <Table.Tr key={k.id}>
                <Table.Td>{k.name}</Table.Td>
                <Table.Td c="dimmed">{k.createdAt ? `added ${formatTime(k.createdAt)}` : ""}</Table.Td>
                <Table.Td c="dimmed">{k.lastUsedAt ? `last used ${formatTime(k.lastUsedAt)}` : "never used"}</Table.Td>
                <Table.Td>
                  <Button size="xs" variant="light" color="red" disabled={busy} onClick={() => remove(k)}>
                    Remove
                  </Button>
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      )}
      {webAuthnAvailable() ? (
        <Group align="flex-end">
          <TextInput
            label="Add a security key"
            placeholder="YubiKey on my keychain"
            value={name}
            onChange={(e) => setName(e.currentTarget.value)}
            w={320}
          />
          <Button loading={busy} disabled={!name.trim()} onClick={add}>
            Add key
          </Button>
        </Group>
      ) : (
        <Text size="sm" c="dimmed">
          This browser can't use security keys.
        </Text>
      )}
      {error && (
        <Alert color="red" title="Not done">
          {error}
        </Alert>
      )}
    </Stack>
  );
}

/** For an admin: someone's keys, to remove one they lost so they can enroll again. */
function ResetKeys() {
  const environment = useRelayEnvironment();
  const [human, setHuman] = useState("");
  const [found, setFound] = useState<ChallengesPageKeysOfQuery["response"]["securityKeys"] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const keysOf = graphql`
    query ChallengesPageKeysOfQuery($humanId: String!) {
      securityKeys(humanId: $humanId) {
        id
        name
        createdAt
        lastUsedAt
      }
    }
  `;
  const resetKey = graphql`
    mutation ChallengesPageResetKeyMutation($input: ResetSecurityKeyInput!) {
      resetSecurityKey(input: $input) {
        securityKey {
          id
        }
      }
    }
  `;

  const look = async () => {
    try {
      const res = await fetchQuery<ChallengesPageKeysOfQuery>(environment, keysOf, {
        humanId: human.trim(),
      }).toPromise();
      setFound(res?.securityKeys ?? []);
      setError(null);
    } catch (e) {
      setError(webAuthnMessage(e));
    }
  };
  const reset = async (id: string) => {
    try {
      await mutate<ChallengesPageResetKeyMutation>(environment, resetKey, { input: { securityKeyQid: id } });
      await look();
    } catch (e) {
      setError(webAuthnMessage(e));
    }
  };

  return (
    <Stack gap="xs">
      <Title order={4}>Reset a lost security key</Title>
      <Text size="sm" c="dimmed">
        Removing someone's key lets them add a first key again on their sign-in alone, so check who's asking.
      </Text>
      <Group align="flex-end">
        <TextInput label="Person" placeholder="bob" value={human} onChange={(e) => setHuman(e.currentTarget.value)} />
        <Button variant="default" disabled={!human.trim()} onClick={look}>
          Look up keys
        </Button>
      </Group>
      {found &&
        (found.length === 0 ? (
          <Text c="dimmed">No security keys.</Text>
        ) : (
          <Table>
            <Table.Tbody>
              {found.map((k) => (
                <Table.Tr key={k.id}>
                  <Table.Td>{k.name}</Table.Td>
                  <Table.Td c="dimmed">{k.createdAt ? `added ${formatTime(k.createdAt)}` : ""}</Table.Td>
                  <Table.Td>
                    <Button size="xs" variant="light" color="red" onClick={() => reset(k.id)}>
                      Remove
                    </Button>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        ))}
      {error && (
        <Alert color="red" title="Not done">
          {error}
        </Alert>
      )}
    </Stack>
  );
}

/** Approvals and containments, for those who administer challenges. */
function Admin({ fetchKey, onChanged }: { fetchKey: number; onChanged: () => void }) {
  const data = useLazyLoadQuery<ChallengesPageAdminQuery>(
    graphql`
      query ChallengesPageAdminQuery {
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
      <ResetKeys />
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
      variables: { input: { challengeQid: c.id, approve, note } },
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
    mutation ChallengesPageContainMutation($input: ContainPrincipalInput!) {
      containPrincipal(input: $input) {
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
