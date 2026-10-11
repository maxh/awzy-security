/**
 * @generated SignedSource<<f725ead8bfce75aea592a7bf1374e7dd>>
 * @relayHash 064990088b7f3f8169e8a19b7762f85f
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID a547b4badffb62945045fbc7886d889920a7ce4953ee2c2afaba450e7428a5db

import { ConcreteRequest } from 'relay-runtime';
export type ChallengesPageChallengeUpdatedSubscription$variables = Record<PropertyKey, never>;
export type ChallengesPageChallengeUpdatedSubscription$data = {
  readonly challengeUpdated: {
    readonly id: string;
  };
};
export type ChallengesPageChallengeUpdatedSubscription = {
  response: ChallengesPageChallengeUpdatedSubscription$data;
  variables: ChallengesPageChallengeUpdatedSubscription$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "alias": null,
    "args": null,
    "concreteType": "Challenge",
    "kind": "LinkedField",
    "name": "challengeUpdated",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "id",
        "storageKey": null
      }
    ],
    "storageKey": null
  }
];
return {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "ChallengesPageChallengeUpdatedSubscription",
    "selections": (v0/*:: as any*/),
    "type": "ClientPortalSubscription",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "ChallengesPageChallengeUpdatedSubscription",
    "selections": (v0/*:: as any*/)
  },
  "params": {
    "id": "a547b4badffb62945045fbc7886d889920a7ce4953ee2c2afaba450e7428a5db",
    "metadata": {},
    "name": "ChallengesPageChallengeUpdatedSubscription",
    "operationKind": "subscription",
    "text": null
  }
};
})();

(node as any).hash = "4e78d983a2a62c6d1709e8671fcdf7ec";

export default node;
