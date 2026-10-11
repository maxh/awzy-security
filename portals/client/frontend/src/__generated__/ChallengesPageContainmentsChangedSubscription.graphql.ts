/**
 * @generated SignedSource<<b3155b1515b5df4d5942167d1b6602a9>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ChallengesPageContainmentsChangedSubscription$variables = Record<PropertyKey, never>;
export type ChallengesPageContainmentsChangedSubscription$data = {
  readonly containmentsChanged: ReadonlyArray<{
    readonly id: string;
  }>;
};
export type ChallengesPageContainmentsChangedSubscription = {
  response: ChallengesPageContainmentsChangedSubscription$data;
  variables: ChallengesPageContainmentsChangedSubscription$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "alias": null,
    "args": null,
    "concreteType": "Containment",
    "kind": "LinkedField",
    "name": "containmentsChanged",
    "plural": true,
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
    "name": "ChallengesPageContainmentsChangedSubscription",
    "selections": (v0/*:: as any*/),
    "type": "ClientPortalSubscription",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "ChallengesPageContainmentsChangedSubscription",
    "selections": (v0/*:: as any*/)
  },
  "params": {
    "cacheID": "8dcf991428a90977a42db316b98e0a1c",
    "id": null,
    "metadata": {},
    "name": "ChallengesPageContainmentsChangedSubscription",
    "operationKind": "subscription",
    "text": "subscription ChallengesPageContainmentsChangedSubscription {\n  containmentsChanged {\n    id\n  }\n}\n"
  }
};
})();

(node as any).hash = "1da03f6ae5d3bc54a408f5f933092743";

export default node;
