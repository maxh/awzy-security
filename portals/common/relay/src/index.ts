import { createClient } from "graphql-ws";
import {
  Environment,
  Network,
  Observable,
  RecordSource,
  Store,
  type FetchFunction,
  type SubscribeFunction,
} from "relay-runtime";

export type EnvironmentOptions = {
  /** The signed-in viewer's access token, sent as a bearer token. */
  token?: () => string | null;
  /** Called when the backend rejects the token, e.g. because it expired. */
  onUnauthorized?: () => void;
};

/**
 * Relay environment for a portal backend: queries and mutations over HTTP POST,
 * subscriptions over graphql-ws on the same path.
 */
export function createEnvironment(endpoint = "/graphql", options: EnvironmentOptions = {}): Environment {
  const authorization = (): Record<string, string> => {
    const token = options.token?.();
    return token ? { authorization: `Bearer ${token}` } : {};
  };

  const fetchFn: FetchFunction = async (operation, variables) => {
    const res = await fetch(endpoint, {
      method: "POST",
      headers: { "content-type": "application/json", ...authorization() },
      body: JSON.stringify({ query: operation.text, variables }),
    });
    if (res.status === 401) {
      options.onUnauthorized?.();
      throw new Error("Your session has expired. Signing in again.");
    }
    const body = (await res.json()) as { data?: unknown; errors?: { message: string }[] };
    // Pages don't handle partial data, so any error fails the whole query, with the server's
    // reasons. The page's error boundary shows them.
    if (body.errors?.length) {
      throw new Error([...new Set(body.errors.map((e) => e.message))].join("\n"));
    }
    return body as never;
  };

  const wsUrl = new URL(endpoint, window.location.href);
  wsUrl.protocol = wsUrl.protocol === "https:" ? "wss:" : "ws:";
  const ws = createClient({ url: wsUrl.toString(), lazy: true, connectionParams: authorization });

  const subscribeFn: SubscribeFunction = (operation, variables) =>
    Observable.create((sink) =>
      ws.subscribe(
        { operationName: operation.name, query: operation.text ?? "", variables },
        {
          next: (value) => sink.next(value as never),
          error: (err) => sink.error(err instanceof Error ? err : new Error(String(err))),
          complete: () => sink.complete(),
        },
      ),
    );

  return new Environment({
    network: Network.create(fetchFn, subscribeFn),
    store: new Store(new RecordSource()),
  });
}
