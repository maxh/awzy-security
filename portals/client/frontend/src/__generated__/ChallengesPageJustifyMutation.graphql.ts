/**
 * @generated SignedSource<<9be4438e404666b7112a4ace86fa76a0>>
 * @relayHash 5517194588ccf16e2604ede52b525eb9
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 34c09b098b561b3db1e499b09c2b494fd13474ef2e2e801a1e7b5d768c863de1

import { ConcreteRequest } from 'relay-runtime';
export type ChallengeKind = "APPROVAL" | "BIOMETRIC" | "JUSTIFICATION" | "VERIFICATION" | "%future added value";
export type ChallengeStatus = "EXPIRED" | "PASSED" | "PENDING" | "REJECTED" | "%future added value";
export type JustifyChallengeInput = {
  challengeQid: string;
  justification: string;
};
export type ChallengesPageJustifyMutation$variables = {
  input: JustifyChallengeInput;
};
export type ChallengesPageJustifyMutation$data = {
  readonly justifyChallenge: {
    readonly challenge: {
      readonly grantExpiresAt: string | null | undefined;
      readonly id: string;
      readonly kinds: ReadonlyArray<ChallengeKind>;
      readonly passed: ReadonlyArray<ChallengeKind>;
      readonly status: ChallengeStatus;
    };
  };
};
export type ChallengesPageJustifyMutation = {
  response: ChallengesPageJustifyMutation$data;
  variables: ChallengesPageJustifyMutation$variables;
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
    "name": "justifyChallenge",
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
    "name": "ChallengesPageJustifyMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "ChallengesPageJustifyMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "id": "34c09b098b561b3db1e499b09c2b494fd13474ef2e2e801a1e7b5d768c863de1",
    "metadata": {},
    "name": "ChallengesPageJustifyMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "07b7c82b20ce42e052dbae23e426eb2b";

export default node;
