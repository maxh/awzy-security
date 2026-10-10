/**
 * @generated SignedSource<<0aedafa8ea689eda87aab32939d81b8d>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ChallengeKind = "APPROVAL" | "BIOMETRIC" | "JUSTIFICATION" | "VERIFICATION" | "%future added value";
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
export type ChallengesPageFinishVerificationMutation$variables = {
  input: FinishChallengeVerificationInput;
};
export type ChallengesPageFinishVerificationMutation$data = {
  readonly finishChallengeVerification: {
    readonly challenge: {
      readonly grantExpiresAt: string | null | undefined;
      readonly id: string;
      readonly kinds: ReadonlyArray<ChallengeKind>;
      readonly passed: ReadonlyArray<ChallengeKind>;
      readonly status: ChallengeStatus;
    };
  };
};
export type ChallengesPageFinishVerificationMutation = {
  response: ChallengesPageFinishVerificationMutation$data;
  variables: ChallengesPageFinishVerificationMutation$variables;
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
          },
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "kinds",
            "storageKey": null
          },
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "passed",
            "storageKey": null
          },
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "grantExpiresAt",
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
    "name": "ChallengesPageFinishVerificationMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "ChallengesPageFinishVerificationMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "d6a53fbf18cee798575701a9f42c0b1e",
    "id": null,
    "metadata": {},
    "name": "ChallengesPageFinishVerificationMutation",
    "operationKind": "mutation",
    "text": "mutation ChallengesPageFinishVerificationMutation(\n  $input: FinishChallengeVerificationInput!\n) {\n  finishChallengeVerification(input: $input) {\n    challenge {\n      id\n      status\n      kinds\n      passed\n      grantExpiresAt\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "ba4115bc589f4e010fa44646e680a3b6";

export default node;
