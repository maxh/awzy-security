/**
 * @generated SignedSource<<4b15ac520882a730611eb25b67f72b1e>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

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
    "cacheID": "73adbd631dc96e7273015acda3c5287f",
    "id": null,
    "metadata": {},
    "name": "ChallengesPageBeginRegistrationMutation",
    "operationKind": "mutation",
    "text": "mutation ChallengesPageBeginRegistrationMutation(\n  $input: BeginSecurityKeyRegistrationInput!\n) {\n  beginSecurityKeyRegistration(input: $input) {\n    ceremonyQid\n    optionsJson\n  }\n}\n"
  }
};
})();

(node as any).hash = "b052f798eb425e3be24eaed951f0af81";

export default node;
