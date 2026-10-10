/**
 * @generated SignedSource<<cb2153c5a800eb6a0cd2553ccb7cc089>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type DirectoryPageSummaryQuery$variables = Record<PropertyKey, never>;
export type DirectoryPageSummaryQuery$data = {
  readonly directorySummary: {
    readonly activeHumans: number;
    readonly groups: number;
    readonly humans: number;
    readonly lastProvisionedAt: string | null | undefined;
    readonly serviceAccounts: number;
  };
};
export type DirectoryPageSummaryQuery = {
  response: DirectoryPageSummaryQuery$data;
  variables: DirectoryPageSummaryQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "alias": null,
    "args": null,
    "concreteType": "DirectorySummary",
    "kind": "LinkedField",
    "name": "directorySummary",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "humans",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "activeHumans",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "groups",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "serviceAccounts",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "lastProvisionedAt",
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
    "name": "DirectoryPageSummaryQuery",
    "selections": (v0/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "DirectoryPageSummaryQuery",
    "selections": (v0/*:: as any*/)
  },
  "params": {
    "cacheID": "ad856d27a9f5f6ebefe573fe4011b36a",
    "id": null,
    "metadata": {},
    "name": "DirectoryPageSummaryQuery",
    "operationKind": "query",
    "text": "query DirectoryPageSummaryQuery {\n  directorySummary {\n    humans\n    activeHumans\n    groups\n    serviceAccounts\n    lastProvisionedAt\n  }\n}\n"
  }
};
})();

(node as any).hash = "70248a0cdb0ab089ecc0bd9994943332";

export default node;
