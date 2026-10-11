/**
 * @generated SignedSource<<20c9bde5e5d9e94e7d5c8e63973bdb1e>>
 * @relayHash a0374ce306476892f8bd5da902318141
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 5b5e8f13053ef2308fad3b65c7adbeb77ab02eb4f45e8536a21a69f2b58e0280

import { ConcreteRequest } from 'relay-runtime';
export type BeginChallengeVerificationInput = {
  challengeQid: string;
};
export type EnforcementPageBeginVerificationMutation$variables = {
  input: BeginChallengeVerificationInput;
};
export type EnforcementPageBeginVerificationMutation$data = {
  readonly beginChallengeVerification: {
    readonly ceremonyQid: string;
    readonly optionsJson: string;
  };
};
export type EnforcementPageBeginVerificationMutation = {
  response: EnforcementPageBeginVerificationMutation$data;
  variables: EnforcementPageBeginVerificationMutation$variables;
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
    "name": "EnforcementPageBeginVerificationMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "EnforcementPageBeginVerificationMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "id": "5b5e8f13053ef2308fad3b65c7adbeb77ab02eb4f45e8536a21a69f2b58e0280",
    "metadata": {},
    "name": "EnforcementPageBeginVerificationMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "eb5e67bb03dd8ad33e72c11d8c782cb1";

export default node;
