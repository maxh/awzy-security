/**
 * @generated SignedSource<<0584bccfd937dc777bd201b09fb9743f>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ChallengesPageKeysOfQuery$variables = {
  personId: string;
};
export type ChallengesPageKeysOfQuery$data = {
  readonly securityKeys: ReadonlyArray<{
    readonly createdAt: string | null | undefined;
    readonly id: string;
    readonly lastUsedAt: string | null | undefined;
    readonly name: string;
  }>;
};
export type ChallengesPageKeysOfQuery = {
  response: ChallengesPageKeysOfQuery$data;
  variables: ChallengesPageKeysOfQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "personId"
  }
],
v1 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Variable",
        "name": "personId",
        "variableName": "personId"
      }
    ],
    "concreteType": "SecurityKey",
    "kind": "LinkedField",
    "name": "securityKeys",
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
        "name": "name",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "createdAt",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "lastUsedAt",
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
    "name": "ChallengesPageKeysOfQuery",
    "selections": (v1/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "ChallengesPageKeysOfQuery",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "cae60034c72dba432ac3ca1a3d288472",
    "id": null,
    "metadata": {},
    "name": "ChallengesPageKeysOfQuery",
    "operationKind": "query",
    "text": "query ChallengesPageKeysOfQuery(\n  $personId: String!\n) {\n  securityKeys(personId: $personId) {\n    id\n    name\n    createdAt\n    lastUsedAt\n  }\n}\n"
  }
};
})();

(node as any).hash = "923694357810340a341f06e7f67a79ef";

export default node;
