/**
 * @generated SignedSource<<5895dd0d90e36df0b4ef575e38618ee7>>
 * @relayHash be3b75d08234e3142650b0419bbeab79
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 83e7fb2af6e3747a53ab2deb1c0e6860de883828c620eaa0a33722928d48e555

import { ConcreteRequest } from 'relay-runtime';
export type BeginChallengeVerificationInput = {
  challengeQid: string;
};
export type ChallengesPageBeginVerificationMutation$variables = {
  input: BeginChallengeVerificationInput;
};
export type ChallengesPageBeginVerificationMutation$data = {
  readonly beginChallengeVerification: {
    readonly ceremonyQid: string;
    readonly optionsJson: string;
  };
};
export type ChallengesPageBeginVerificationMutation = {
  response: ChallengesPageBeginVerificationMutation$data;
  variables: ChallengesPageBeginVerificationMutation$variables;
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
    "name": "beginChallengeVerification",
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
    "name": "ChallengesPageBeginVerificationMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "ChallengesPageBeginVerificationMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "id": "83e7fb2af6e3747a53ab2deb1c0e6860de883828c620eaa0a33722928d48e555",
    "metadata": {},
    "name": "ChallengesPageBeginVerificationMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "af122573564591a28a302fee4cb20557";

export default node;
