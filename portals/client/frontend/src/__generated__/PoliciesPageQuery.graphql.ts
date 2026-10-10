/**
 * @generated SignedSource<<d91c34d64f58a5785ee944e460ef5894>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type PolicyBundleStatus = "ACTIVE" | "DRAFT" | "SUPERSEDED" | "%future added value";
export type PoliciesPageQuery$variables = Record<PropertyKey, never>;
export type PoliciesPageQuery$data = {
  readonly activePolicyBundle: {
    readonly id: string;
    readonly policies: string;
    readonly publishedAt: string | null | undefined;
    readonly publishedBy: string | null | undefined;
    readonly schema: string;
    readonly version: number;
  } | null | undefined;
  readonly canPublish: boolean;
  readonly policyBundles: ReadonlyArray<{
    readonly createdAt: string | null | undefined;
    readonly createdBy: string;
    readonly id: string;
    readonly publishedAt: string | null | undefined;
    readonly status: PolicyBundleStatus;
    readonly version: number;
  }>;
};
export type PoliciesPageQuery = {
  response: PoliciesPageQuery$data;
  variables: PoliciesPageQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
},
v1 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "version",
  "storageKey": null
},
v2 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "publishedAt",
  "storageKey": null
},
v3 = [
  {
    "alias": "canPublish",
    "args": [
      {
        "kind": "Literal",
        "name": "action",
        "value": "ADMINISTER"
      },
      {
        "kind": "Literal",
        "name": "area",
        "value": "POLICIES"
      }
    ],
    "kind": "ScalarField",
    "name": "viewerCan",
    "storageKey": "viewerCan(action:\"ADMINISTER\",area:\"POLICIES\")"
  },
  {
    "alias": null,
    "args": null,
    "concreteType": "PolicyBundle",
    "kind": "LinkedField",
    "name": "activePolicyBundle",
    "plural": false,
    "selections": [
      (v0/*:: as any*/),
      (v1/*:: as any*/),
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "schema",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "policies",
        "storageKey": null
      },
      (v2/*:: as any*/),
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "publishedBy",
        "storageKey": null
      }
    ],
    "storageKey": null
  },
  {
    "alias": null,
    "args": [
      {
        "kind": "Literal",
        "name": "first",
        "value": 20
      }
    ],
    "concreteType": "PolicyBundle",
    "kind": "LinkedField",
    "name": "policyBundles",
    "plural": true,
    "selections": [
      (v0/*:: as any*/),
      (v1/*:: as any*/),
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "status",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "createdAt",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "createdBy",
        "storageKey": null
      },
      (v2/*:: as any*/)
    ],
    "storageKey": "policyBundles(first:20)"
  }
];
return {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "PoliciesPageQuery",
    "selections": (v3/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "PoliciesPageQuery",
    "selections": (v3/*:: as any*/)
  },
  "params": {
    "cacheID": "c7d61245d95e12d6676ddbab5e6fa12e",
    "id": null,
    "metadata": {},
    "name": "PoliciesPageQuery",
    "operationKind": "query",
    "text": "query PoliciesPageQuery {\n  canPublish: viewerCan(action: ADMINISTER, area: POLICIES)\n  activePolicyBundle {\n    id\n    version\n    schema\n    policies\n    publishedAt\n    publishedBy\n  }\n  policyBundles(first: 20) {\n    id\n    version\n    status\n    createdAt\n    createdBy\n    publishedAt\n  }\n}\n"
  }
};
})();

(node as any).hash = "c4211f1de8aeee23ea0fb5321dc29231";

export default node;
