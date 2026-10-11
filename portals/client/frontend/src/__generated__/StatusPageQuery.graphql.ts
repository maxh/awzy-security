/**
 * @generated SignedSource<<e651bc1f6506aef7ed45d10a5f0791e4>>
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
    "cacheID": "549ee381ca2041784d7a9be3b861864b",
    "id": null,
    "metadata": {},
    "name": "StatusPageQuery",
    "operationKind": "query",
    "text": "query StatusPageQuery {\n  installStatus {\n    name\n    ready\n    version\n    detail\n  }\n}\n"
  }
};
})();

(node as any).hash = "36e8817901f8962dd8aeb7eee126c0fb";

export default node;
