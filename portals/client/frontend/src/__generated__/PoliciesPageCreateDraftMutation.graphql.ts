/**
 * @generated SignedSource<<d31fab1df8d06497085ea0834225db47>>
 * @relayHash 5153f5bc3dd94f87d9536c7582a3221d
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID ddffdb32ed14ab288a4661f7a976b438828841140d53e22ac76b47709d32b1fc

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
    "id": "ddffdb32ed14ab288a4661f7a976b438828841140d53e22ac76b47709d32b1fc",
    "metadata": {},
    "name": "PoliciesPageCreateDraftMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "8e9fd211eca6029185c6ce17550f9fe8";

export default node;
