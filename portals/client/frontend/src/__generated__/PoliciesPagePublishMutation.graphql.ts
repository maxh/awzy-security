/**
 * @generated SignedSource<<6749616d18dc81321badcea6968dd716>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type PolicyBundleStatus = "ACTIVE" | "DRAFT" | "SUPERSEDED" | "%future added value";
export type PublishPolicyBundleInput = {
  acceptLockout?: boolean;
  policyBundleQid: string;
};
export type PoliciesPagePublishMutation$variables = {
  input: PublishPolicyBundleInput;
};
export type PoliciesPagePublishMutation$data = {
  readonly publishPolicyBundle: {
    readonly bundle: {
      readonly id: string;
      readonly publishedAt: string | null | undefined;
      readonly status: PolicyBundleStatus;
    } | null | undefined;
    readonly wouldLockYouOut: string | null | undefined;
  };
};
export type PoliciesPagePublishMutation = {
  response: PoliciesPagePublishMutation$data;
  variables: PoliciesPagePublishMutation$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "input"
  }
],
v1 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Variable",
        "name": "input",
        "variableName": "input"
      }
    ],
    "concreteType": "PublishPolicyBundlePayload",
    "kind": "LinkedField",
    "name": "publishPolicyBundle",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "PolicyBundle",
        "kind": "LinkedField",
        "name": "bundle",
        "plural": false,
        "selections": [
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "id",
            "storageKey": null
          },
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
            "name": "publishedAt",
            "storageKey": null
          }
        ],
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "wouldLockYouOut",
        "storageKey": null
      }
    ],
    "storageKey": null
  }
];
return {
  "fragment": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Fragment",
    "metadata": null,
    "name": "PoliciesPagePublishMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "PoliciesPagePublishMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "24d3283630f83f10782cdde3b420478c",
    "id": null,
    "metadata": {},
    "name": "PoliciesPagePublishMutation",
    "operationKind": "mutation",
    "text": "mutation PoliciesPagePublishMutation(\n  $input: PublishPolicyBundleInput!\n) {\n  publishPolicyBundle(input: $input) {\n    bundle {\n      id\n      status\n      publishedAt\n    }\n    wouldLockYouOut\n  }\n}\n"
  }
};
})();

(node as any).hash = "576c158e1854d37fb6fbef467a9b2028";

export default node;
