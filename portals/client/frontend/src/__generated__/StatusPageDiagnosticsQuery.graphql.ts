/**
 * @generated SignedSource<<da879d7936380efcdaf616de46f6942b>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

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
    "cacheID": "40073846613a6a1d62e6320776eebd27",
    "id": null,
    "metadata": {},
    "name": "StatusPageDiagnosticsQuery",
    "operationKind": "query",
    "text": "query StatusPageDiagnosticsQuery {\n  diagnosticsBundle\n}\n"
  }
};
})();

(node as any).hash = "061ead73cfa0d15fb57544488f75889e";

export default node;
