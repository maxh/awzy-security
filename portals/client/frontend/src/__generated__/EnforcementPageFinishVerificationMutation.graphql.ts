/**
 * @generated SignedSource<<08bed5b6e555826a240a8d828be7f89d>>
 * @relayHash f2df88983a4008959f410fbf030502bf
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 4bd37329f33c75d0581788d1d5569797bf7421ce0c4d19c54f330c544fead1f1

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
    "id": "4bd37329f33c75d0581788d1d5569797bf7421ce0c4d19c54f330c544fead1f1",
    "metadata": {},
    "name": "EnforcementPageFinishVerificationMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "020d0a67ac31be8bf243711c267c834e";

export default node;
