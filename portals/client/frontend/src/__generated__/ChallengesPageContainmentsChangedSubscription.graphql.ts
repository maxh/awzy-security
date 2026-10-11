/**
 * @generated SignedSource<<051be6051cbb3debf48b74327f61537b>>
 * @relayHash 8dcf991428a90977a42db316b98e0a1c
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 6e2a6d3088377a43e2b1c37303cf00f4d8cb4a2c3bc20c1d6628abaa547022b0

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
    "id": "6e2a6d3088377a43e2b1c37303cf00f4d8cb4a2c3bc20c1d6628abaa547022b0",
    "metadata": {},
    "name": "ChallengesPageContainmentsChangedSubscription",
    "operationKind": "subscription",
    "text": null
  }
};
})();

(node as any).hash = "1da03f6ae5d3bc54a408f5f933092743";

export default node;
