/**
 * @generated SignedSource<<91b85d6821f0e366d2713e4751c64ca6>>
 * @relayHash ad856d27a9f5f6ebefe573fe4011b36a
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 30d03cb15626a8f0bb2b0cbd9931abcbef34e0053df04f129c78f33bbcbb0686

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
    "id": "30d03cb15626a8f0bb2b0cbd9931abcbef34e0053df04f129c78f33bbcbb0686",
    "metadata": {},
    "name": "DirectoryPageSummaryQuery",
    "operationKind": "query",
    "text": null
  }
};
})();

(node as any).hash = "70248a0cdb0ab089ecc0bd9994943332";

export default node;
