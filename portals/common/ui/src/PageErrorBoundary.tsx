import { Alert, Button, Stack, Text } from "@mantine/core";
import { Component, type ReactNode } from "react";

type Props = { children: ReactNode };
type State = { error: Error | null };

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
    return (
      <Alert color="red" title="This page couldn't load">
        <Stack gap="xs" align="flex-start">
          <Text size="sm" style={{ whiteSpace: "pre-wrap" }}>
            {error.message}
          </Text>
          <Button size="xs" variant="light" color="red" onClick={() => this.setState({ error: null })}>
            Try again
          </Button>
        </Stack>
      </Alert>
    );
  }
}
