/**
 * @generated SignedSource<<82b093bbb3b7a7dcb15240dde41da412>>
 * @relayHash 80bf96951f0778c0d6ff30fc96cd6ff7
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID d87fe2f202853a4499e3ee7880800b3c3b6a44131ad08204554b5c1b4ea33bd9

import { ConcreteRequest } from 'relay-runtime';
export type BeginChallengeVerificationInput = {
  challengeQid: string;
};
export type securityKeyChallengeBeginMutation$variables = {
  input: BeginChallengeVerificationInput;
};
export type securityKeyChallengeBeginMutation$data = {
  readonly beginChallengeVerification: {
    readonly ceremonyQid: string;
    readonly optionsJson: string;
  };
};
export type securityKeyChallengeBeginMutation = {
  response: securityKeyChallengeBeginMutation$data;
  variables: securityKeyChallengeBeginMutation$variables;
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
    "name": "securityKeyChallengeBeginMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "securityKeyChallengeBeginMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "id": "d87fe2f202853a4499e3ee7880800b3c3b6a44131ad08204554b5c1b4ea33bd9",
    "metadata": {},
    "name": "securityKeyChallengeBeginMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "081f92356e2c681879225390e95d9cfa";

export default node;
