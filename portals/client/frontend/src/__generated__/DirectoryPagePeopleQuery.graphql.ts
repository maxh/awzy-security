/**
 * @generated SignedSource<<9ac979d079b98c27c75bbe998fc75cac>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type DirectoryPagePeopleQuery$variables = {
  search?: string | null | undefined;
};
export type DirectoryPagePeopleQuery$data = {
  readonly people: ReadonlyArray<{
    readonly active: boolean;
    readonly displayName: string | null | undefined;
    readonly groups: ReadonlyArray<string>;
    readonly updatedAt: string;
    readonly userName: string;
  }>;
};
export type DirectoryPagePeopleQuery = {
  response: DirectoryPagePeopleQuery$data;
  variables: DirectoryPagePeopleQuery$variables;
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
    "concreteType": "Person",
    "kind": "LinkedField",
    "name": "people",
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
    "name": "DirectoryPagePeopleQuery",
    "selections": (v1/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "DirectoryPagePeopleQuery",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "13223b6e8e498ed6fa528101f3dbe9ec",
    "id": null,
    "metadata": {},
    "name": "DirectoryPagePeopleQuery",
    "operationKind": "query",
    "text": "query DirectoryPagePeopleQuery(\n  $search: String\n) {\n  people(search: $search, first: 200) {\n    userName\n    displayName\n    active\n    groups\n    updatedAt\n  }\n}\n"
  }
};
})();

(node as any).hash = "fb7068b6bedc38707a4ad53087a7bd32";

export default node;
