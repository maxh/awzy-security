/**
 * @generated SignedSource<<7146470a1ebd703a8dc1b6c17148d46a>>
 * @relayHash c773779d32faa9485879a18fee0c12d0
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 8e4af6d5e6524318b4d0d7979c8aa43004279ae533196a84dfe6eea95a67f147

import { ConcreteRequest } from 'relay-runtime';
export type ChallengesPageKeysOfQuery$variables = {
  humanId: string;
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
    "name": "humanId"
  }
],
v1 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Variable",
        "name": "humanId",
        "variableName": "humanId"
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
    "id": "8e4af6d5e6524318b4d0d7979c8aa43004279ae533196a84dfe6eea95a67f147",
    "metadata": {},
    "name": "ChallengesPageKeysOfQuery",
    "operationKind": "query",
    "text": null
  }
};
})();

(node as any).hash = "165ea5f5e052de4a74d75ed63deb8e77";

export default node;
