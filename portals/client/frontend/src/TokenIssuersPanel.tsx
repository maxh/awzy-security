import { PageLoader } from "@awzy/ui";
import { Alert, Button, Group, Modal, Stack, Table, Text, TextInput } from "@mantine/core";
import { type SyntheticEvent, Suspense, useState } from "react";
import { graphql, useLazyLoadQuery, useMutation } from "react-relay";

import type { TokenIssuersPanelQuery } from "./__generated__/TokenIssuersPanelQuery.graphql";
import type { TokenIssuersPanelRegisterMutation } from "./__generated__/TokenIssuersPanelRegisterMutation.graphql";
import type { TokenIssuersPanelRemoveMutation } from "./__generated__/TokenIssuersPanelRemoveMutation.graphql";
import { formatTime } from "./format";

/**
 * Apps trusted to sign tokens for their own agents (ADR 0110): the service accounts whose ID
 * starts with the app's name and a dot, which the app registers and retires itself.
 */
export function TokenIssuersPanel() {
  const [fetchKey, setFetchKey] = useState(0);
  return (
    <Suspense fallback={<PageLoader />}>
      <TokenIssuersTable fetchKey={fetchKey} onChanged={() => setFetchKey((k) => k + 1)} />
    </Suspense>
  );
}

function TokenIssuersTable({ fetchKey, onChanged }: { fetchKey: number; onChanged: () => void }) {
  const data = useLazyLoadQuery<TokenIssuersPanelQuery>(
    graphql`
      query TokenIssuersPanelQuery {
        canAdminister: viewerCan(action: ADMINISTER, area: DIRECTORY)
        tokenIssuers(first: 100) {
          nodes {
            name
            issuer
            jwksUrl
            audience
            agentPrefix
            updatedBy {
              label
            }
            updatedAt
          }
        }
      }
    `,
    {},
    { fetchKey, fetchPolicy: "store-and-network" },
  );
  const [registering, setRegistering] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [commitRemove, isRemoving] = useMutation<TokenIssuersPanelRemoveMutation>(graphql`
    mutation TokenIssuersPanelRemoveMutation($input: RemoveTokenIssuerInput!) {
      removeTokenIssuer(input: $input) {
        removedName
      }
    }
  `);
  const remove = (name: string) => {
    if (!window.confirm(`Stop trusting ${name}'s tokens? Its agents stay registered until you remove them.`)) return;
    commitRemove({ variables: { input: { name } }, onCompleted: onChanged, onError: (e) => setError(e.message) });
  };

  return (
    <Stack>
      <Group justify="space-between">
        <Text size="sm" c="dimmed">
          Apps that sign tokens for their own agents, besides your IdP. Each signs only for service accounts under its
          prefix, and registers and retires them itself.
        </Text>
        {data.canAdminister && <Button onClick={() => setRegistering(true)}>Trust an app</Button>}
      </Group>
      {error && <Alert color="red">{error}</Alert>}
      {data.tokenIssuers.nodes.length === 0 ? (
        <Text c="dimmed">Only your IdP&apos;s tokens are trusted.</Text>
      ) : (
        <Table>
          <Table.Thead>
            <Table.Tr>
              <Table.Th>App</Table.Th>
              <Table.Th>Agents</Table.Th>
              <Table.Th>Issuer and audience</Table.Th>
              <Table.Th>Changed</Table.Th>
              <Table.Th />
            </Table.Tr>
          </Table.Thead>
          <Table.Tbody>
            {data.tokenIssuers.nodes.map((app) => (
              <Table.Tr key={app.name}>
                <Table.Td ff="monospace">{app.name}</Table.Td>
                <Table.Td ff="monospace">{app.agentPrefix}*</Table.Td>
                <Table.Td>
                  <Text size="sm">{app.issuer}</Text>
                  <Text size="xs" c="dimmed">
                    aud {app.audience}, keys from {app.jwksUrl}
                  </Text>
                </Table.Td>
                <Table.Td>
                  {formatTime(app.updatedAt)} by {app.updatedBy?.label ?? "unknown"}
                </Table.Td>
                <Table.Td>
                  {data.canAdminister && (
                    <Button
                      size="xs"
                      variant="subtle"
                      color="red"
                      loading={isRemoving}
                      onClick={() => remove(app.name)}
                    >
                      Remove
                    </Button>
                  )}
                </Table.Td>
              </Table.Tr>
            ))}
          </Table.Tbody>
        </Table>
      )}
      <TrustApp opened={registering} onClose={() => setRegistering(false)} onTrusted={onChanged} />
    </Stack>
  );
}

function TrustApp({ opened, onClose, onTrusted }: { opened: boolean; onClose: () => void; onTrusted: () => void }) {
  const [name, setName] = useState("");
  const [issuer, setIssuer] = useState("");
  const [jwksUrl, setJwksUrl] = useState("");
  const [audience, setAudience] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [commit, isSaving] = useMutation<TokenIssuersPanelRegisterMutation>(graphql`
    mutation TokenIssuersPanelRegisterMutation($input: RegisterTokenIssuerInput!) {
      registerTokenIssuer(input: $input) {
        tokenIssuer {
          name
        }
      }
    }
  `);
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    commit({
      variables: {
        input: { name: name.trim(), issuer: issuer.trim(), jwksUrl: jwksUrl.trim(), audience: audience.trim() },
      },
      onCompleted: () => {
        setError(null);
        onTrusted();
        onClose();
      },
      onError: (err) => setError(err.message),
    });
  };
  const prefix = name.trim() ? `${name.trim()}.` : "<name>.";
  return (
    <Modal opened={opened} onClose={onClose} title="Trust an app to sign tokens for its agents" size="lg">
      <form onSubmit={submit}>
        <Stack>
          {error && <Alert color="red">{error}</Alert>}
          <TextInput
            label="Name"
            description={`Lowercase letters, digits, and dashes. Its agents' IDs start with ${prefix}`}
            placeholder="acme-crm"
            value={name}
            onChange={(e) => setName(e.currentTarget.value)}
            required
          />
          <TextInput
            label="Issuer"
            description="The iss its tokens carry"
            placeholder="https://crm.example.com"
            value={issuer}
            onChange={(e) => setIssuer(e.currentTarget.value)}
            required
          />
          <TextInput
            label="JWKS URL"
            description="Where its signing keys are published (RS256 or ES256), https://"
            value={jwksUrl}
            onChange={(e) => setJwksUrl(e.currentTarget.value)}
            required
          />
          <TextInput
            label="Audience"
            description="The aud its tokens must carry"
            value={audience}
            onChange={(e) => setAudience(e.currentTarget.value)}
            required
          />
          <Group justify="flex-end">
            <Button type="submit" loading={isSaving}>
              Trust
            </Button>
          </Group>
        </Stack>
      </form>
    </Modal>
  );
}
