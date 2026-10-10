/**
 * @generated SignedSource<<87687e4c322e8802869d05b01fd24da3>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type DirectoryPageSummaryQuery$variables = Record<PropertyKey, never>;
export type DirectoryPageSummaryQuery$data = {
  readonly directorySummary: {
    readonly activePeople: number;
    readonly groups: number;
    readonly lastProvisionedAt: string | null | undefined;
    readonly people: number;
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
        "name": "people",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "activePeople",
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
    "cacheID": "886e8b83bc39f6fca418437acccf4a69",
    "id": null,
    "metadata": {},
    "name": "DirectoryPageSummaryQuery",
    "operationKind": "query",
    "text": "query DirectoryPageSummaryQuery {\n  directorySummary {\n    people\n    activePeople\n    groups\n    serviceAccounts\n    lastProvisionedAt\n  }\n}\n"
  }
};
})();

(node as any).hash = "524d6bc1b31d67791ca4d872b0d74830";

export default node;
