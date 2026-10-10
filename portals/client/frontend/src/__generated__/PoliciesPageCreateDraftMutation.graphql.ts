/**
 * @generated SignedSource<<2950f397d64e53e959dc95591759a7de>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type CreatePolicyDraftInput = {
  policies: string;
  schema: string;
};
export type PoliciesPageCreateDraftMutation$variables = {
  input: CreatePolicyDraftInput;
};
export type PoliciesPageCreateDraftMutation$data = {
  readonly createPolicyDraft: {
    readonly bundle: {
      readonly id: string;
      readonly version: number;
    } | null | undefined;
    readonly validationError: string | null | undefined;
  };
};
export type PoliciesPageCreateDraftMutation = {
  response: PoliciesPageCreateDraftMutation$data;
  variables: PoliciesPageCreateDraftMutation$variables;
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
    "concreteType": "CreatePolicyDraftPayload",
    "kind": "LinkedField",
    "name": "createPolicyDraft",
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
            "name": "version",
            "storageKey": null
          }
        ],
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "validationError",
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
    "name": "PoliciesPageCreateDraftMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "PoliciesPageCreateDraftMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "5153f5bc3dd94f87d9536c7582a3221d",
    "id": null,
    "metadata": {},
    "name": "PoliciesPageCreateDraftMutation",
    "operationKind": "mutation",
    "text": "mutation PoliciesPageCreateDraftMutation(\n  $input: CreatePolicyDraftInput!\n) {\n  createPolicyDraft(input: $input) {\n    bundle {\n      id\n      version\n    }\n    validationError\n  }\n}\n"
  }
};
})();

(node as any).hash = "8e9fd211eca6029185c6ce17550f9fe8";

export default node;
