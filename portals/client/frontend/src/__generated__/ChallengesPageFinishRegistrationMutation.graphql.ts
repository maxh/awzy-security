/**
 * @generated SignedSource<<45f22a9b2dbe130c1e2dde9813de9145>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

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
    "cacheID": "a0e1e8584d2209ab90c0420626b90645",
    "id": null,
    "metadata": {},
    "name": "ChallengesPageFinishRegistrationMutation",
    "operationKind": "mutation",
    "text": "mutation ChallengesPageFinishRegistrationMutation(\n  $input: FinishSecurityKeyRegistrationInput!\n) {\n  finishSecurityKeyRegistration(input: $input) {\n    securityKey {\n      id\n      name\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "cb72ad64f7833b14198f91644b16c519";

export default node;
