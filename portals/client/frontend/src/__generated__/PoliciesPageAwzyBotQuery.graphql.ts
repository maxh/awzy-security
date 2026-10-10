/**
 * @generated SignedSource<<e59bdf5932871a86f2434b796e9d2219>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

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
    "cacheID": "d6eb1b872c1fba9ca148ebe470b2f38d",
    "id": null,
    "metadata": {},
    "name": "PoliciesPageAwzyBotQuery",
    "operationKind": "query",
    "text": "query PoliciesPageAwzyBotQuery {\n  awzyBotEnabled\n}\n"
  }
};
})();

(node as any).hash = "e0affa8f8b566421e3966d7e0d2ca2b0";

export default node;
