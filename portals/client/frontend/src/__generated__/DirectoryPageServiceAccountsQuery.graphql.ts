/**
 * @generated SignedSource<<afbfea7b559ff21b85e6cbf06cf706f7>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type DirectoryPageServiceAccountsQuery$variables = Record<PropertyKey, never>;
export type DirectoryPageServiceAccountsQuery$data = {
  readonly canAdminister: boolean;
  readonly serviceAccounts: ReadonlyArray<{
    readonly ai: boolean;
    readonly description: string;
    readonly groups: ReadonlyArray<string>;
    readonly id: string;
    readonly owner: string;
    readonly updatedAt: string;
    readonly updatedBy: string;
  }>;
};
export type DirectoryPageServiceAccountsQuery = {
  response: DirectoryPageServiceAccountsQuery$data;
  variables: DirectoryPageServiceAccountsQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "alias": "canAdminister",
    "args": [
      {
        "kind": "Literal",
        "name": "action",
        "value": "ADMINISTER"
      },
      {
        "kind": "Literal",
        "name": "area",
        "value": "DIRECTORY"
      }
    ],
    "kind": "ScalarField",
    "name": "viewerCan",
    "storageKey": "viewerCan(action:\"ADMINISTER\",area:\"DIRECTORY\")"
  },
  {
    "alias": null,
    "args": null,
    "concreteType": "ServiceAccount",
    "kind": "LinkedField",
    "name": "serviceAccounts",
    "plural": true,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "id",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "owner",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "ai",
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
        "name": "description",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "updatedBy",
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
    "name": "DirectoryPageServiceAccountsQuery",
    "selections": (v0/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "DirectoryPageServiceAccountsQuery",
    "selections": (v0/*:: as any*/)
  },
  "params": {
    "cacheID": "f882fcc1663adc3b0b853803685f75d9",
    "id": null,
    "metadata": {},
    "name": "DirectoryPageServiceAccountsQuery",
    "operationKind": "query",
    "text": "query DirectoryPageServiceAccountsQuery {\n  canAdminister: viewerCan(action: ADMINISTER, area: DIRECTORY)\n  serviceAccounts {\n    id\n    owner\n    ai\n    groups\n    description\n    updatedBy\n    updatedAt\n  }\n}\n"
  }
};
})();

(node as any).hash = "fffb4ec31f83f61575d2c6f279d439e6";

export default node;
