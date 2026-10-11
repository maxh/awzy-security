/**
 * @generated SignedSource<<268209ead7b4a31a036879c74fb37aad>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ChallengeStatus = "EXPIRED" | "PASSED" | "PENDING" | "REJECTED" | "%future added value";
export type FinishChallengeVerificationInput = {
  assertion: AssertionInput;
};
export type AssertionInput = {
  authenticatorData: string;
  ceremonyQid: string;
  clientDataJson: string;
  credentialId: string;
  signature: string;
};
export type EnforcementPageFinishVerificationMutation$variables = {
  input: FinishChallengeVerificationInput;
};
export type EnforcementPageFinishVerificationMutation$data = {
  readonly finishChallengeVerification: {
    readonly challenge: {
      readonly id: string;
      readonly status: ChallengeStatus;
    };
  };
};
export type EnforcementPageFinishVerificationMutation = {
  response: EnforcementPageFinishVerificationMutation$data;
  variables: EnforcementPageFinishVerificationMutation$variables;
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
    "concreteType": "ChallengePayload",
    "kind": "LinkedField",
    "name": "finishChallengeVerification",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "Challenge",
        "kind": "LinkedField",
        "name": "challenge",
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
            "name": "status",
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
    "name": "EnforcementPageFinishVerificationMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "EnforcementPageFinishVerificationMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "f2df88983a4008959f410fbf030502bf",
    "id": null,
    "metadata": {},
    "name": "EnforcementPageFinishVerificationMutation",
    "operationKind": "mutation",
    "text": "mutation EnforcementPageFinishVerificationMutation(\n  $input: FinishChallengeVerificationInput!\n) {\n  finishChallengeVerification(input: $input) {\n    challenge {\n      id\n      status\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "020d0a67ac31be8bf243711c267c834e";

export default node;
