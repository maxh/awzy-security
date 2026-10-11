/**
 * @generated SignedSource<<79798d6273a90ea0594aca32b6f7b56b>>
 * @relayHash 73adbd631dc96e7273015acda3c5287f
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID eaa3a9a04f5287ac6817615e5d6bf77c3c66e6dda69e9a3b3ea642ef27f27e08

import { ConcreteRequest } from 'relay-runtime';
export type BeginSecurityKeyRegistrationInput = {
  authorization?: AssertionInput | null | undefined;
};
export type AssertionInput = {
  authenticatorData: string;
  ceremonyQid: string;
  clientDataJson: string;
  credentialId: string;
  signature: string;
};
export type ChallengesPageBeginRegistrationMutation$variables = {
  input: BeginSecurityKeyRegistrationInput;
};
export type ChallengesPageBeginRegistrationMutation$data = {
  readonly beginSecurityKeyRegistration: {
    readonly ceremonyQid: string;
    readonly optionsJson: string;
  };
};
export type ChallengesPageBeginRegistrationMutation = {
  response: ChallengesPageBeginRegistrationMutation$data;
  variables: ChallengesPageBeginRegistrationMutation$variables;
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
    "concreteType": "WebAuthnCeremony",
    "kind": "LinkedField",
    "name": "beginSecurityKeyRegistration",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "ceremonyQid",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "optionsJson",
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
    "name": "ChallengesPageBeginRegistrationMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "ChallengesPageBeginRegistrationMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "id": "eaa3a9a04f5287ac6817615e5d6bf77c3c66e6dda69e9a3b3ea642ef27f27e08",
    "metadata": {},
    "name": "ChallengesPageBeginRegistrationMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "b052f798eb425e3be24eaed951f0af81";

export default node;
