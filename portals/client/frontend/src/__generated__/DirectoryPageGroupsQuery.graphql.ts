/**
 * @generated SignedSource<<1c3b137ceddf5c84e63f50d3b37598f3>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type DirectoryPageGroupsQuery$variables = Record<PropertyKey, never>;
export type DirectoryPageGroupsQuery$data = {
  readonly groups: ReadonlyArray<{
    readonly groups: number;
    readonly name: string;
    readonly parents: ReadonlyArray<string>;
    readonly people: number;
    readonly updatedAt: string;
  }>;
};
export type DirectoryPageGroupsQuery = {
  response: DirectoryPageGroupsQuery$data;
  variables: DirectoryPageGroupsQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "alias": null,
    "args": null,
    "concreteType": "DirectoryGroup",
    "kind": "LinkedField",
    "name": "groups",
    "plural": true,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "name",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "people",
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
        "name": "parents",
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
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "DirectoryPageGroupsQuery",
    "selections": (v0/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "DirectoryPageGroupsQuery",
    "selections": (v0/*:: as any*/)
  },
  "params": {
    "cacheID": "b53dc799eb96006042c564fa99f4f8c3",
    "id": null,
    "metadata": {},
    "name": "DirectoryPageGroupsQuery",
    "operationKind": "query",
    "text": "query DirectoryPageGroupsQuery {\n  groups {\n    name\n    people\n    groups\n    parents\n    updatedAt\n  }\n}\n"
  }
};
})();

(node as any).hash = "6133a9410af749a67921344f3c81ce11";

export default node;
