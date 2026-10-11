import { Alert, Button, Code, Modal, Stack, Text } from "@mantine/core";
import { IconRobot } from "@tabler/icons-react";
import { useState } from "react";
import { graphql, useMutation } from "react-relay";

import type { McpTokenButtonMutation } from "./__generated__/McpTokenButtonMutation.graphql";

/** Mints a token for an agent of yours on the corp portal's MCP endpoint: read-only, 30 days. */
export function McpTokenButton() {
  const [token, setToken] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mint, minting] = useMutation<McpTokenButtonMutation>(graphql`
    mutation McpTokenButtonMutation {
      mcpToken
    }
  `);
  const open = () =>
    mint({
      variables: {},
      onCompleted: (res, errors) => (errors?.[0] ? setError(errors[0].message) : setToken(res.mcpToken)),
      onError: (e) => setError(e.message),
    });
  return (
    <>
      <Button variant="default" leftSection={<IconRobot size={16} />} loading={minting} onClick={open}>
        Token for an agent
      </Button>
      <Modal
        opened={token !== null || error !== null}
        onClose={() => {
          setToken(null);
          setError(null);
        }}
        title={<Text fw={600}>MCP token</Text>}
        size="lg"
      >
        <Stack gap="sm">
          {error && <Alert color="red">{error}</Alert>}
          {token && (
            <>
              <Text size="sm">
                For your agent's MCP client, as a bearer token on {window.location.origin}/mcp. Read-only tools over the
                fleet, for 30 days, and nothing else in this portal. Shown once.
              </Text>
              <Code block style={{ wordBreak: "break-all" }}>
                {token}
              </Code>
            </>
          )}
        </Stack>
      </Modal>
    </>
  );
}
