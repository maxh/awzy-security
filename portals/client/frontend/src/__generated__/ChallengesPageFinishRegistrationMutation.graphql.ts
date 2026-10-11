/**
 * @generated SignedSource<<8e4da4b8a26e65999b1a5dc92a9fc7f5>>
 * @relayHash a0e1e8584d2209ab90c0420626b90645
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 6fd6660a60188fb6976dd075abdb83a99511d79dd6495e6b58c602196c0cd43b

import { ConcreteRequest } from 'relay-runtime';
export type FinishSecurityKeyRegistrationInput = {
  attestationObject: string;
  ceremonyQid: string;
  clientDataJson: string;
  name: string;
};
export type ChallengesPageFinishRegistrationMutation$variables = {
  input: FinishSecurityKeyRegistrationInput;
};
export type ChallengesPageFinishRegistrationMutation$data = {
  readonly finishSecurityKeyRegistration: {
    readonly securityKey: {
      readonly id: string;
      readonly name: string;
    };
  };
};
export type ChallengesPageFinishRegistrationMutation = {
  response: ChallengesPageFinishRegistrationMutation$data;
  variables: ChallengesPageFinishRegistrationMutation$variables;
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
    "name": "finishSecurityKeyRegistration",
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
          },
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "name",
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
    "name": "ChallengesPageFinishRegistrationMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "ChallengesPageFinishRegistrationMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "id": "6fd6660a60188fb6976dd075abdb83a99511d79dd6495e6b58c602196c0cd43b",
    "metadata": {},
    "name": "ChallengesPageFinishRegistrationMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "cb72ad64f7833b14198f91644b16c519";

export default node;
