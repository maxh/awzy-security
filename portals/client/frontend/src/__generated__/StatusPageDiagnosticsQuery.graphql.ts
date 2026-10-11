/**
 * @generated SignedSource<<ddd5632c3d7b4946f19bb707a18902f0>>
 * @relayHash 40073846613a6a1d62e6320776eebd27
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID de0ad711c5c39e99744046155fff8a5ed7f1d3a0af00477c59e06ed9a4d1aa61

import { ConcreteRequest } from 'relay-runtime';
export type StatusPageDiagnosticsQuery$variables = Record<PropertyKey, never>;
export type StatusPageDiagnosticsQuery$data = {
  readonly diagnosticsBundle: string;
};
export type StatusPageDiagnosticsQuery = {
  response: StatusPageDiagnosticsQuery$data;
  variables: StatusPageDiagnosticsQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "alias": null,
    "args": null,
    "kind": "ScalarField",
    "name": "diagnosticsBundle",
    "storageKey": null
  }
];
return {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "StatusPageDiagnosticsQuery",
    "selections": (v0/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "StatusPageDiagnosticsQuery",
    "selections": (v0/*:: as any*/)
  },
  "params": {
    "id": "de0ad711c5c39e99744046155fff8a5ed7f1d3a0af00477c59e06ed9a4d1aa61",
    "metadata": {},
    "name": "StatusPageDiagnosticsQuery",
    "operationKind": "query",
    "text": null
  }
};
})();

(node as any).hash = "061ead73cfa0d15fb57544488f75889e";

export default node;
