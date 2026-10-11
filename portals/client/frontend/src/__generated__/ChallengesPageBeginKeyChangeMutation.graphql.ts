/**
 * @generated SignedSource<<7078d0458ddef4526efa138312112843>>
 * @relayHash 77c28ba0e1478b7c3cf723bc4fca1f0e
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 629eee1e69d9906ef3459a0fc5d76bf06568fbf795a6abe82ce05073c420ca84

import { ConcreteRequest } from 'relay-runtime';
export type ChallengesPageBeginKeyChangeMutation$variables = Record<PropertyKey, never>;
export type ChallengesPageBeginKeyChangeMutation$data = {
  readonly beginSecurityKeyChange: {
    readonly ceremonyQid: string;
    readonly optionsJson: string;
  };
};
export type ChallengesPageBeginKeyChangeMutation = {
  response: ChallengesPageBeginKeyChangeMutation$data;
  variables: ChallengesPageBeginKeyChangeMutation$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "alias": null,
    "args": null,
    "concreteType": "WebAuthnCeremony",
    "kind": "LinkedField",
    "name": "beginSecurityKeyChange",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "ceremonyQid",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "optionsJson",
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
    "name": "ChallengesPageBeginKeyChangeMutation",
    "selections": (v0/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "ChallengesPageBeginKeyChangeMutation",
    "selections": (v0/*:: as any*/)
  },
  "params": {
    "id": "629eee1e69d9906ef3459a0fc5d76bf06568fbf795a6abe82ce05073c420ca84",
    "metadata": {},
    "name": "ChallengesPageBeginKeyChangeMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "88d4955543aa67b88fdf8920f6f2f971";

export default node;
