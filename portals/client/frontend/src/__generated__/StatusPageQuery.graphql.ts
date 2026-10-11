/**
 * @generated SignedSource<<7a1fbc5bf0589eb04a8522a1df86a363>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type StatusPageQuery$variables = Record<PropertyKey, never>;
export type StatusPageQuery$data = {
  readonly installStatus: ReadonlyArray<{
    readonly detail: string | null | undefined;
    readonly name: string;
    readonly ready: boolean;
    readonly runbookUrl: string;
    readonly version: string | null | undefined;
  }>;
};
export type StatusPageQuery = {
  response: StatusPageQuery$data;
  variables: StatusPageQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "alias": null,
    "args": null,
    "concreteType": "ComponentStatus",
    "kind": "LinkedField",
    "name": "installStatus",
    "plural": true,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "name",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "ready",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "version",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "detail",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "runbookUrl",
        "storageKey": null
      }
    ],
    "storageKey": null
  }
];
return {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "StatusPageQuery",
    "selections": (v0/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "StatusPageQuery",
    "selections": (v0/*:: as any*/)
  },
  "params": {
    "cacheID": "5a2e58da8a06efc74ab91fea9a244c95",
    "id": null,
    "metadata": {},
    "name": "StatusPageQuery",
    "operationKind": "query",
    "text": "query StatusPageQuery {\n  installStatus {\n    name\n    ready\n    version\n    detail\n    runbookUrl\n  }\n}\n"
  }
};
})();

(node as any).hash = "af205d084df79e3563f37c1f62b311a6";

export default node;
