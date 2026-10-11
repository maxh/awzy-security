/**
 * @generated SignedSource<<ec99b7cd324b6d1ca636c24f4cafdb07>>
 * @relayHash 0c340448a2c83438c7f5e3330be54198
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID fcec1f400ff7e4f6598509fc9dc00843a065345f4a5733ff97e332e70f390fb5

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
export type securityKeyChallengeFinishMutation$variables = {
  input: FinishChallengeVerificationInput;
};
export type securityKeyChallengeFinishMutation$data = {
  readonly finishChallengeVerification: {
    readonly challenge: {
      readonly id: string;
      readonly status: ChallengeStatus;
    };
  };
};
export type securityKeyChallengeFinishMutation = {
  response: securityKeyChallengeFinishMutation$data;
  variables: securityKeyChallengeFinishMutation$variables;
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
    "name": "securityKeyChallengeFinishMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "securityKeyChallengeFinishMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "id": "fcec1f400ff7e4f6598509fc9dc00843a065345f4a5733ff97e332e70f390fb5",
    "metadata": {},
    "name": "securityKeyChallengeFinishMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "b925ccd01010098c45e43ee00a5ce285";

export default node;
