import { createClient } from "graphql-ws";
import {
  Environment,
  Network,
  Observable,
  RecordSource,
  Store,
  type FetchFunction,
  type GraphQLSingularResponse,
  type RequestParameters,
  type SubscribeFunction,
} from "relay-runtime";

export type EnvironmentOptions = {
  /** The signed-in viewer's access token, sent as a bearer token. */
  token?: () => string | null;
  /** Called when the backend rejects the token, e.g. because it expired. */
  onUnauthorized?: () => void;
  /**
   * Send each operation's text instead of its persisted ID: for the Vite dev server, whose
   * queries a backend doesn't have (ADR 0090).
   */
  sendQueryText?: boolean;
};

/**
 * What's sent for an operation: its persisted ID, which the backend maps to the text the built
 * frontend it serves sends (ADR 0088, 0090), or its text.
 */
function operationBody(operation: RequestParameters, sendQueryText: boolean) {
  return operation.id && !sendQueryText
    ? { query: "", extensions: { persistedQuery: { version: 1, sha256Hash: operation.id } } }
    : { query: operation.text ?? "" };
}

/** Whether `v` is a GraphQL response: data, errors, or both, as the server sent it. */
function isGraphQLResponse(v: unknown): v is GraphQLSingularResponse {
  if (typeof v !== "object" || v === null || Array.isArray(v)) return false;
  const errors = "errors" in v ? v.errors : undefined;
  const errorsOk =
    errors === undefined ||
    (Array.isArray(errors) &&
      errors.every(
        (e: unknown) => typeof e === "object" && e !== null && "message" in e && typeof e.message === "string",
      ));
  return ("data" in v || errors !== undefined) && errorsOk;
}

/**
 * A failed operation, with what kind of failure it was: the `code` extension of its first error
 * that has one (FORBIDDEN, CHALLENGE_REQUIRED, ...; #224), and the challenge to answer for a
 * CHALLENGE_REQUIRED. Branch on `code`, never on the message.
 */
export class PortalError extends Error {
  readonly code: string | null;
  readonly challengeQid: string | null;

  constructor(errors: readonly unknown[]) {
    const messages = errors.map((e) => (hasField(e, "message") && typeof e.message === "string" ? e.message : ""));
    super([...new Set(messages.filter(Boolean))].join("\n"));
    this.name = "PortalError";
    const extensions = errors
      .map((e) => (hasField(e, "extensions") ? e.extensions : undefined))
      .find((x) => hasField(x, "code") && typeof x.code === "string");
    this.code = hasField(extensions, "code") && typeof extensions.code === "string" ? extensions.code : null;
    this.challengeQid =
      hasField(extensions, "challengeQid") && typeof extensions.challengeQid === "string"
        ? extensions.challengeQid
        : null;
  }
}

function hasField<K extends string>(v: unknown, key: K): v is Record<K, unknown> {
  return typeof v === "object" && v !== null && key in v;
}

/**
 * Relay environment for a portal backend: queries and mutations over HTTP POST,
 * subscriptions over graphql-ws on the same path, each sent by its persisted ID.
 */
export function createEnvironment(endpoint = "/graphql", options: EnvironmentOptions = {}): Environment {
  const sendQueryText = options.sendQueryText ?? false;
  const authorization = (): Record<string, string> => {
    const token = options.token?.();
    return token ? { authorization: `Bearer ${token}` } : {};
  };

  const fetchFn: FetchFunction = async (operation, variables) => {
    const res = await fetch(endpoint, {
      method: "POST",
      headers: { "content-type": "application/json", ...authorization() },
      body: JSON.stringify({ ...operationBody(operation, sendQueryText), variables }),
    });
    if (res.status === 401) {
      options.onUnauthorized?.();
      throw new Error("Your session has expired. Signing in again.");
    }
    const body: unknown = await res.json();
    if (!isGraphQLResponse(body)) throw new Error("The portal's answer isn't a GraphQL response.");
    // Pages don't handle partial data, so any error fails the whole query, with the server's
    // reasons and code. The page's error boundary shows them.
    const errors = "errors" in body ? (body.errors ?? []) : [];
    if (errors.length) throw new PortalError(errors);
    return body;
  };

  const wsUrl = new URL(endpoint, window.location.href);
  wsUrl.protocol = wsUrl.protocol === "https:" ? "wss:" : "ws:";
  const ws = createClient({ url: wsUrl.toString(), lazy: true, connectionParams: authorization });

  const subscribeFn: SubscribeFunction = (operation, variables) =>
    Observable.create((sink) =>
      ws.subscribe(
        { operationName: operation.name, ...operationBody(operation, sendQueryText), variables },
        {
          next: (value) =>
            isGraphQLResponse(value) ? sink.next(value) : sink.error(new Error("Not a GraphQL response.")),
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
