/**
 * @generated SignedSource<<7cbe83d9bb886af6f4f9b4ce344438fc>>
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
    readonly updatedBy: {
      readonly label: string;
    } | null | undefined;
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
        "concreteType": "Actor",
        "kind": "LinkedField",
        "name": "updatedBy",
        "plural": false,
        "selections": [
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "label",
            "storageKey": null
          }
        ],
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
    "cacheID": "05fac166ef452c6b91c734e6665a80a8",
    "id": null,
    "metadata": {},
    "name": "DirectoryPageServiceAccountsQuery",
    "operationKind": "query",
    "text": "query DirectoryPageServiceAccountsQuery {\n  canAdminister: viewerCan(action: ADMINISTER, area: DIRECTORY)\n  serviceAccounts {\n    id\n    owner\n    ai\n    groups\n    description\n    updatedBy {\n      label\n    }\n    updatedAt\n  }\n}\n"
  }
};
})();

(node as any).hash = "6a4dc4237ea29ea91f67ea877356c801";

export default node;
