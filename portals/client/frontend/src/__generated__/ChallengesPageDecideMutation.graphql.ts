/**
 * @generated SignedSource<<6d065f0bcbf2ae1e7e44bfc702df1773>>
 * @relayHash 30f3fd76945267dd121b105496d77398
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 7057bef6b72a54e22d4fc05ee0057152fc4ae14ab11bc66618d1653a5df2439d

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
    "id": "7057bef6b72a54e22d4fc05ee0057152fc4ae14ab11bc66618d1653a5df2439d",
    "metadata": {},
    "name": "ChallengesPageDecideMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "7d8dad84f7898b8e2b0d4943ca018191";

export default node;
