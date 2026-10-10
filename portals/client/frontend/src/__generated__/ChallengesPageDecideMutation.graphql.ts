/**
 * @generated SignedSource<<1d631d1cd2768648545da06a183adf7f>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ChallengeStatus = "EXPIRED" | "PASSED" | "PENDING" | "REJECTED" | "%future added value";
export type DecideChallengeInput = {
  approve: boolean;
  challengeQid: string;
  note?: string | null | undefined;
};
export type ChallengesPageDecideMutation$variables = {
  input: DecideChallengeInput;
};
export type ChallengesPageDecideMutation$data = {
  readonly decideChallenge: {
    readonly challenge: {
      readonly id: string;
      readonly status: ChallengeStatus;
    };
  };
};
export type ChallengesPageDecideMutation = {
  response: ChallengesPageDecideMutation$data;
  variables: ChallengesPageDecideMutation$variables;
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
    "name": "decideChallenge",
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
    "name": "ChallengesPageDecideMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "ChallengesPageDecideMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "30f3fd76945267dd121b105496d77398",
    "id": null,
    "metadata": {},
    "name": "ChallengesPageDecideMutation",
    "operationKind": "mutation",
    "text": "mutation ChallengesPageDecideMutation(\n  $input: DecideChallengeInput!\n) {\n  decideChallenge(input: $input) {\n    challenge {\n      id\n      status\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "7d8dad84f7898b8e2b0d4943ca018191";

export default node;
