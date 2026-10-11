/**
 * @generated SignedSource<<c8435c9d97be20695c194d8c4539e57b>>
 * @relayHash 3022fb2d2f96d3c2c3416d7f558d76c2
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 54e56aa745cadf5a16077797ef677ea75b0218bdd6964f37b2e053f354fb2372

import { ConcreteRequest } from 'relay-runtime';
export type DirectoryPageGroupsQuery$variables = Record<PropertyKey, never>;
export type DirectoryPageGroupsQuery$data = {
  readonly groups: ReadonlyArray<{
    readonly groups: number;
    readonly humans: number;
    readonly name: string;
    readonly parents: ReadonlyArray<string>;
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
        "name": "humans",
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
    "id": "54e56aa745cadf5a16077797ef677ea75b0218bdd6964f37b2e053f354fb2372",
    "metadata": {},
    "name": "DirectoryPageGroupsQuery",
    "operationKind": "query",
    "text": null
  }
};
})();

(node as any).hash = "a562796f54568b2f5c97ed9433854856";

export default node;
