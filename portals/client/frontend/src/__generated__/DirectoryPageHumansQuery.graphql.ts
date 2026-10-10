/**
 * @generated SignedSource<<659efb54069fd4753dd22fea77c87b99>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type DirectoryPageHumansQuery$variables = {
  search?: string | null | undefined;
};
export type DirectoryPageHumansQuery$data = {
  readonly humans: ReadonlyArray<{
    readonly active: boolean;
    readonly displayName: string | null | undefined;
    readonly groups: ReadonlyArray<string>;
    readonly updatedAt: string;
    readonly userName: string;
  }>;
};
export type DirectoryPageHumansQuery = {
  response: DirectoryPageHumansQuery$data;
  variables: DirectoryPageHumansQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "search"
  }
],
v1 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Literal",
        "name": "first",
        "value": 200
      },
      {
        "kind": "Variable",
        "name": "search",
        "variableName": "search"
      }
    ],
    "concreteType": "Human",
    "kind": "LinkedField",
    "name": "humans",
    "plural": true,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "userName",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "displayName",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "active",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "groups",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "updatedAt",
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
    "name": "DirectoryPageHumansQuery",
    "selections": (v1/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "DirectoryPageHumansQuery",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "45f13ffb51b9a085c32a9ec2a277513e",
    "id": null,
    "metadata": {},
    "name": "DirectoryPageHumansQuery",
    "operationKind": "query",
    "text": "query DirectoryPageHumansQuery(\n  $search: String\n) {\n  humans(search: $search, first: 200) {\n    userName\n    displayName\n    active\n    groups\n    updatedAt\n  }\n}\n"
  }
};
})();

(node as any).hash = "0353b3abbdf5ca75898043e564f1ed84";

export default node;
