/**
 * @generated SignedSource<<c19bce4fff039e41aa5e774b4c3101fa>>
 * @relayHash 1456a2eeddfd8b3d08731e68a8bfbbc9
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 0b4ec95ee5ceaaaab5a17c58e9156603074b89d0fe393a295b648a34d82135ea

import { ConcreteRequest } from 'relay-runtime';
export type StatusPageQuery$variables = Record<PropertyKey, never>;
export type StatusPageQuery$data = {
  readonly canGenerateDiagnostics: boolean;
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
    "alias": "canGenerateDiagnostics",
    "args": [
      {
        "kind": "Literal",
        "name": "action",
        "value": "ADMINISTER"
      },
      {
        "kind": "Literal",
        "name": "area",
        "value": "ENFORCEMENT"
      }
    ],
    "kind": "ScalarField",
    "name": "viewerCan",
    "storageKey": "viewerCan(action:\"ADMINISTER\",area:\"ENFORCEMENT\")"
  },
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
    "id": "0b4ec95ee5ceaaaab5a17c58e9156603074b89d0fe393a295b648a34d82135ea",
    "metadata": {},
    "name": "StatusPageQuery",
    "operationKind": "query",
    "text": null
  }
};
})();

(node as any).hash = "7e0e34c09f829797fe2efd2b7203cb12";

export default node;
