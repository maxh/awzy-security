import { Alert, Button, Stack, Text } from "@mantine/core";
import { Component, type ReactNode } from "react";

import { errorCode, type ErrorCode } from "./errors";

type Props = { children: ReactNode };
type State = { error: Error | null };

/** How each kind of failure reads; a code the page doesn't know reads as a failed load. */
const KINDS: Partial<Record<ErrorCode, { color: string; title: string; retry: boolean }>> = {
  FORBIDDEN: { color: "yellow", title: "You don't have access to this", retry: false },
  NOT_FOUND: { color: "yellow", title: "This doesn't exist, or you can't see it", retry: false },
  UNAVAILABLE: { color: "orange", title: "Part of awzy isn't answering", retry: true },
};
const FAILED = { color: "red", title: "This page couldn't load", retry: true };

/**
 * Catches an error while rendering a page, e.g. a query that failed because a backend is
 * down, and shows it inside the page frame. The header, navigation, and sign-out stay usable.
 * Give it a `key` that changes with the route, so moving to another page starts fresh.
 */
export class PageErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    const code = errorCode(error);
    const kind = (code && KINDS[code]) ?? FAILED;
    return (
      <Alert color={kind.color} title={kind.title}>
        <Stack gap="xs" align="flex-start">
          <Text size="sm" style={{ whiteSpace: "pre-wrap" }}>
            {error.message}
          </Text>
          {kind.retry && (
            <Button size="xs" variant="light" color={kind.color} onClick={() => this.setState({ error: null })}>
              Try again
            </Button>
          )}
        </Stack>
      </Alert>
    );
  }
}
