/**
 * @generated SignedSource<<49c5fa940dcc59e12ca959ec1dae1c12>>
 * @relayHash 447cccd313f55c6abdce6448366fb0f9
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 8e57d6327b0c6e8bc8d390ea01e34900f86dfa540f467b5935cea2442614e7a0

import { ConcreteRequest } from 'relay-runtime';
export type PolicyBundleStatus = "ACTIVE" | "DRAFT" | "SUPERSEDED" | "%future added value";
export type PoliciesPageQuery$variables = Record<PropertyKey, never>;
export type PoliciesPageQuery$data = {
  readonly activePolicyBundle: {
    readonly id: string;
    readonly policies: string;
    readonly publishedAt: string | null | undefined;
    readonly publishedBy: {
      readonly label: string;
    } | null | undefined;
    readonly schema: string;
    readonly version: number;
  } | null | undefined;
  readonly canPublish: boolean;
  readonly policyBundles: ReadonlyArray<{
    readonly createdAt: string | null | undefined;
    readonly createdBy: {
      readonly label: string;
    } | null | undefined;
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
    "alias": null,
    "args": null,
    "kind": "ScalarField",
    "name": "label",
    "storageKey": null
  }
],
v4 = [
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
        "concreteType": "Actor",
        "kind": "LinkedField",
        "name": "publishedBy",
        "plural": false,
        "selections": (v3/*:: as any*/),
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
        "concreteType": "Actor",
        "kind": "LinkedField",
        "name": "createdBy",
        "plural": false,
        "selections": (v3/*:: as any*/),
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
    "selections": (v4/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "PoliciesPageQuery",
    "selections": (v4/*:: as any*/)
  },
  "params": {
    "id": "8e57d6327b0c6e8bc8d390ea01e34900f86dfa540f467b5935cea2442614e7a0",
    "metadata": {},
    "name": "PoliciesPageQuery",
    "operationKind": "query",
    "text": null
  }
};
})();

(node as any).hash = "3b082a9bc4bc808ff7dcb96e73d54f63";

export default node;
