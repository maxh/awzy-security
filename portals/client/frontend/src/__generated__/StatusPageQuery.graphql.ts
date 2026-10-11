/**
 * @generated SignedSource<<706e138f10cd86870e4c8c7d32a058d6>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

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
    "cacheID": "1456a2eeddfd8b3d08731e68a8bfbbc9",
    "id": null,
    "metadata": {},
    "name": "StatusPageQuery",
    "operationKind": "query",
    "text": "query StatusPageQuery {\n  canGenerateDiagnostics: viewerCan(action: ADMINISTER, area: ENFORCEMENT)\n  installStatus {\n    name\n    ready\n    version\n    detail\n    runbookUrl\n  }\n}\n"
  }
};
})();

(node as any).hash = "7e0e34c09f829797fe2efd2b7203cb12";

export default node;
