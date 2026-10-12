import { Alert, Button, Group, Stack, Table, Text, TextInput, Title } from "@mantine/core";
import { useState } from "react";
import { fetchQuery, graphql, useRelayEnvironment } from "react-relay";
import type { Environment } from "relay-runtime";

import type { ChallengesPageQuery } from "./__generated__/ChallengesPageQuery.graphql";
import type { SecurityKeysBeginKeyChangeMutation } from "./__generated__/SecurityKeysBeginKeyChangeMutation.graphql";
import type { SecurityKeysBeginRegistrationMutation } from "./__generated__/SecurityKeysBeginRegistrationMutation.graphql";
import type { SecurityKeysFinishRegistrationMutation } from "./__generated__/SecurityKeysFinishRegistrationMutation.graphql";
import type { SecurityKeysKeysOfQuery } from "./__generated__/SecurityKeysKeysOfQuery.graphql";
import type { SecurityKeysRemoveKeyMutation } from "./__generated__/SecurityKeysRemoveKeyMutation.graphql";
import type { SecurityKeysResetKeyMutation } from "./__generated__/SecurityKeysResetKeyMutation.graphql";
import { formatTime } from "./format";
import { mutate } from "./securityKeyChallenge";
import { createCredential, getAssertion, webAuthnAvailable, webAuthnMessage } from "./webauthn";

const beginKeyChange = graphql`
  mutation SecurityKeysBeginKeyChangeMutation {
    beginSecurityKeyChange {
      ceremonyQid
      optionsJson
    }
  }
`;

const beginRegistration = graphql`
  mutation SecurityKeysBeginRegistrationMutation($input: BeginSecurityKeyRegistrationInput!) {
    beginSecurityKeyRegistration(input: $input) {
      ceremonyQid
      optionsJson
    }
  }
`;

const finishRegistration = graphql`
  mutation SecurityKeysFinishRegistrationMutation($input: FinishSecurityKeyRegistrationInput!) {
    finishSecurityKeyRegistration(input: $input) {
      securityKey {
        id
        name
      }
    }
  }
`;

const removeKey = graphql`
  mutation SecurityKeysRemoveKeyMutation($input: RemoveSecurityKeyInput!) {
    removeSecurityKey(input: $input) {
      securityKey {
        id
      }
    }
  }
`;

/** An assertion from one of the signed-in person's keys, authorizing a change to their keys. */
async function authorizeKeyChange(environment: Environment) {
  const { beginSecurityKeyChange: c } = await mutate<SecurityKeysBeginKeyChangeMutation>(
    environment,
    beginKeyChange,
    {},
  );
  return getAssertion(c.ceremonyQid, c.optionsJson);
}

type Key = ChallengesPageQuery["response"]["mySecurityKeys"][number];

/** The signed-in person's security keys. The first is added on their sign-in alone; adding or
 * removing any other takes a touch of a key they already have (ADR 0059). */
export function SecurityKeys({ keys, onChanged }: { keys: ReadonlyArray<Key>; onChanged: () => void }) {
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
      const { beginSecurityKeyRegistration: r } = await mutate<SecurityKeysBeginRegistrationMutation>(
        environment,
        beginRegistration,
        { input: { authorization } },
      );
      const created = await createCredential(r.optionsJson);
      await mutate<SecurityKeysFinishRegistrationMutation>(environment, finishRegistration, {
        input: { ceremonyQid: r.ceremonyQid, name: name.trim(), ...created },
      });
      setName("");
    });

  const remove = (key: Key) =>
    run(async () => {
      const authorization = await authorizeKeyChange(environment);
      await mutate<SecurityKeysRemoveKeyMutation>(environment, removeKey, {
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
        <Table.ScrollContainer minWidth={560}>
          <Table>
            <Table.Tbody>
              {keys.map((k) => (
                <Table.Tr key={k.id}>
                  <Table.Td>{k.name}</Table.Td>
                  <Table.Td c="dimmed">{k.createdAt ? `added ${formatTime(k.createdAt)}` : ""}</Table.Td>
                  <Table.Td c="dimmed">
                    {k.lastUsedAt ? `last used ${formatTime(k.lastUsedAt)}` : "never used"}
                  </Table.Td>
                  <Table.Td>
                    <Button size="xs" variant="light" color="red" disabled={busy} onClick={() => void remove(k)}>
                      Remove
                    </Button>
                  </Table.Td>
                </Table.Tr>
              ))}
            </Table.Tbody>
          </Table>
        </Table.ScrollContainer>
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
          <Button loading={busy} disabled={!name.trim()} onClick={() => void add()}>
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
export function ResetKeys() {
  const environment = useRelayEnvironment();
  const [human, setHuman] = useState("");
  const [found, setFound] = useState<SecurityKeysKeysOfQuery["response"]["securityKeys"] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const keysOf = graphql`
    query SecurityKeysKeysOfQuery($humanId: String!) {
      securityKeys(humanId: $humanId) {
        id
        name
        createdAt
        lastUsedAt
      }
    }
  `;
  const resetKey = graphql`
    mutation SecurityKeysResetKeyMutation($input: ResetSecurityKeyInput!) {
      resetSecurityKey(input: $input) {
        securityKey {
          id
        }
      }
    }
  `;

  const look = async () => {
    try {
      const res = await fetchQuery<SecurityKeysKeysOfQuery>(environment, keysOf, {
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
      await mutate<SecurityKeysResetKeyMutation>(environment, resetKey, { input: { securityKeyQid: id } });
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
        <Button variant="default" disabled={!human.trim()} onClick={() => void look()}>
          Look up keys
        </Button>
      </Group>
      {found &&
        (found.length === 0 ? (
          <Text c="dimmed">No security keys.</Text>
        ) : (
          <Table.ScrollContainer minWidth={560}>
            <Table>
              <Table.Tbody>
                {found.map((k) => (
                  <Table.Tr key={k.id}>
                    <Table.Td>{k.name}</Table.Td>
                    <Table.Td c="dimmed">{k.createdAt ? `added ${formatTime(k.createdAt)}` : ""}</Table.Td>
                    <Table.Td>
                      <Button size="xs" variant="light" color="red" onClick={() => void reset(k.id)}>
                        Remove
                      </Button>
                    </Table.Td>
                  </Table.Tr>
                ))}
              </Table.Tbody>
            </Table>
          </Table.ScrollContainer>
        ))}
      {error && (
        <Alert color="red" title="Not done">
          {error}
        </Alert>
      )}
    </Stack>
  );
}
