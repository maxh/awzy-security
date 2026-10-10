/**
 * @generated SignedSource<<5317c435692984892f88ac78d75d5fdf>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ChallengeKind = "APPROVAL" | "BIOMETRIC" | "JUSTIFICATION" | "VERIFICATION" | "%future added value";
export type ChallengeStatus = "EXPIRED" | "PASSED" | "PENDING" | "REJECTED" | "%future added value";
export type JustifyChallengeInput = {
  id: string;
  justification: string;
};
export type ChallengesPageJustifyMutation$variables = {
  input: JustifyChallengeInput;
};
export type ChallengesPageJustifyMutation$data = {
  readonly justifyChallenge: {
    readonly challenge: {
      readonly id: string;
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
            "name": "passed",
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
    "cacheID": "48760f5ec542492fa7fe74e4bb1836ed",
    "id": null,
    "metadata": {},
    "name": "ChallengesPageJustifyMutation",
    "operationKind": "mutation",
    "text": "mutation ChallengesPageJustifyMutation(\n  $input: JustifyChallengeInput!\n) {\n  justifyChallenge(input: $input) {\n    challenge {\n      id\n      status\n      passed\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "f63bde83c44089c80c4057e00936e6f6";

export default node;
