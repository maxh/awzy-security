/**
 * @generated SignedSource<<03a60cda2472384637ca43485885b7a9>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

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
    "cacheID": "be3b75d08234e3142650b0419bbeab79",
    "id": null,
    "metadata": {},
    "name": "ChallengesPageBeginVerificationMutation",
    "operationKind": "mutation",
    "text": "mutation ChallengesPageBeginVerificationMutation(\n  $input: BeginChallengeVerificationInput!\n) {\n  beginChallengeVerification(input: $input) {\n    ceremonyQid\n    optionsJson\n  }\n}\n"
  }
};
})();

(node as any).hash = "af122573564591a28a302fee4cb20557";

export default node;
