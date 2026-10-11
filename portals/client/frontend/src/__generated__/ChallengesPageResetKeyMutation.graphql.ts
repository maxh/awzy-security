/**
 * @generated SignedSource<<494c686ab098df3bb1ae60ed4f62d9ce>>
 * @relayHash 50b279c9b124cf92d1858a7a29859865
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID a824dba4f516b9624189e3ca2ee2f669ff098f9747683496ebd9cb2005986da0

import { ConcreteRequest } from 'relay-runtime';
export type ResetSecurityKeyInput = {
  securityKeyQid: string;
};
export type ChallengesPageResetKeyMutation$variables = {
  input: ResetSecurityKeyInput;
};
export type ChallengesPageResetKeyMutation$data = {
  readonly resetSecurityKey: {
    readonly securityKey: {
      readonly id: string;
    };
  };
};
export type ChallengesPageResetKeyMutation = {
  response: ChallengesPageResetKeyMutation$data;
  variables: ChallengesPageResetKeyMutation$variables;
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
    "concreteType": "SecurityKeyPayload",
    "kind": "LinkedField",
    "name": "resetSecurityKey",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "SecurityKey",
        "kind": "LinkedField",
        "name": "securityKey",
        "plural": false,
        "selections": [
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "id",
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
    "name": "ChallengesPageResetKeyMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "ChallengesPageResetKeyMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "id": "a824dba4f516b9624189e3ca2ee2f669ff098f9747683496ebd9cb2005986da0",
    "metadata": {},
    "name": "ChallengesPageResetKeyMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "bb5ef0c9693d0e6f73610959c5dd75ed";

export default node;
