/**
 * @generated SignedSource<<4285a09938b8370fe1d3c1f4580e7851>>
 * @relayHash d6eb1b872c1fba9ca148ebe470b2f38d
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID da289075fcc5fda934cdda9a0ec0b7be4103e2ee4aed99d58a1405941839c5f6

import { ConcreteRequest } from 'relay-runtime';
export type PoliciesPageAwzyBotQuery$variables = Record<PropertyKey, never>;
export type PoliciesPageAwzyBotQuery$data = {
  readonly awzyBotEnabled: boolean;
};
export type PoliciesPageAwzyBotQuery = {
  response: PoliciesPageAwzyBotQuery$data;
  variables: PoliciesPageAwzyBotQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "alias": null,
    "args": null,
    "kind": "ScalarField",
    "name": "awzyBotEnabled",
    "storageKey": null
  }
];
return {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "PoliciesPageAwzyBotQuery",
    "selections": (v0/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "PoliciesPageAwzyBotQuery",
    "selections": (v0/*:: as any*/)
  },
  "params": {
    "id": "da289075fcc5fda934cdda9a0ec0b7be4103e2ee4aed99d58a1405941839c5f6",
    "metadata": {},
    "name": "PoliciesPageAwzyBotQuery",
    "operationKind": "query",
    "text": null
  }
};
})();

(node as any).hash = "e0affa8f8b566421e3966d7e0d2ca2b0";

export default node;
