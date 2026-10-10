/**
 * @generated SignedSource<<8feba62de5f77b30cbe866a836cd25b1>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type PolicyBundleStatus = "ACTIVE" | "DRAFT" | "SUPERSEDED" | "%future added value";
export type PublishPolicyBundleInput = {
  id: string;
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
    };
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
    "cacheID": "d4c5eb6d5ad1b4eabec2a5741237b0a7",
    "id": null,
    "metadata": {},
    "name": "PoliciesPagePublishMutation",
    "operationKind": "mutation",
    "text": "mutation PoliciesPagePublishMutation(\n  $input: PublishPolicyBundleInput!\n) {\n  publishPolicyBundle(input: $input) {\n    bundle {\n      id\n      status\n      publishedAt\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "d72419a7fb337fd6c5ecb13de0696d26";

export default node;
