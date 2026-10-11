/**
 * @generated SignedSource<<8595b758ad41ab670592b92db41df8dc>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ChallengeKind = "APPROVAL" | "BIOMETRIC" | "JUSTIFICATION" | "VERIFICATION" | "%future added value";
export type ChallengeStatus = "EXPIRED" | "PASSED" | "PENDING" | "REJECTED" | "%future added value";
export type ChallengesPageQuery$variables = Record<PropertyKey, never>;
export type ChallengesPageQuery$data = {
  readonly canAdminister: boolean;
  readonly mySecurityKeys: ReadonlyArray<{
    readonly createdAt: string | null | undefined;
    readonly id: string;
    readonly lastUsedAt: string | null | undefined;
    readonly name: string;
  }>;
  readonly pending: ReadonlyArray<{
    readonly action: string;
    readonly expiresAt: string | null | undefined;
    readonly id: string;
    readonly kinds: ReadonlyArray<ChallengeKind>;
    readonly onBehalfOf: {
      readonly id: string;
      readonly type: string;
    } | null | undefined;
    readonly passed: ReadonlyArray<ChallengeKind>;
    readonly principal: {
      readonly id: string;
      readonly type: string;
    } | null | undefined;
    readonly resource: {
      readonly id: string;
      readonly type: string;
    } | null | undefined;
  }>;
  readonly recent: ReadonlyArray<{
    readonly action: string;
    readonly createdAt: string | null | undefined;
    readonly decidedBy: {
      readonly label: string;
    } | null | undefined;
    readonly grantExpiresAt: string | null | undefined;
    readonly id: string;
    readonly onBehalfOf: {
      readonly id: string;
      readonly type: string;
    } | null | undefined;
    readonly principal: {
      readonly id: string;
      readonly type: string;
    } | null | undefined;
    readonly resource: {
      readonly id: string;
      readonly type: string;
    } | null | undefined;
    readonly status: ChallengeStatus;
  }>;
};
export type ChallengesPageQuery = {
  response: ChallengesPageQuery$data;
  variables: ChallengesPageQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
},
v1 = [
  {
    "alias": null,
    "args": null,
    "kind": "ScalarField",
    "name": "type",
    "storageKey": null
  },
  (v0/*:: as any*/)
],
v2 = {
  "alias": null,
  "args": null,
  "concreteType": "EntityRef",
  "kind": "LinkedField",
  "name": "principal",
  "plural": false,
  "selections": (v1/*:: as any*/),
  "storageKey": null
},
v3 = {
  "alias": null,
  "args": null,
  "concreteType": "EntityRef",
  "kind": "LinkedField",
  "name": "onBehalfOf",
  "plural": false,
  "selections": (v1/*:: as any*/),
  "storageKey": null
},
v4 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "action",
  "storageKey": null
},
v5 = {
  "alias": null,
  "args": null,
  "concreteType": "EntityRef",
  "kind": "LinkedField",
  "name": "resource",
  "plural": false,
  "selections": (v1/*:: as any*/),
  "storageKey": null
},
v6 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "createdAt",
  "storageKey": null
},
v7 = [
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
        "value": "CHALLENGES"
      }
    ],
    "kind": "ScalarField",
    "name": "viewerCan",
    "storageKey": "viewerCan(action:\"ADMINISTER\",area:\"CHALLENGES\")"
  },
  {
    "alias": "pending",
    "args": [
      {
        "kind": "Literal",
        "name": "status",
        "value": "PENDING"
      }
    ],
    "concreteType": "Challenge",
    "kind": "LinkedField",
    "name": "myChallenges",
    "plural": true,
    "selections": [
      (v0/*:: as any*/),
      (v2/*:: as any*/),
      (v3/*:: as any*/),
      (v4/*:: as any*/),
      (v5/*:: as any*/),
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "kinds",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "passed",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "expiresAt",
        "storageKey": null
      }
    ],
    "storageKey": "myChallenges(status:\"PENDING\")"
  },
  {
    "alias": "recent",
    "args": [
      {
        "kind": "Literal",
        "name": "first",
        "value": 20
      }
    ],
    "concreteType": "Challenge",
    "kind": "LinkedField",
    "name": "myChallenges",
    "plural": true,
    "selections": [
      (v0/*:: as any*/),
      (v2/*:: as any*/),
      (v3/*:: as any*/),
      (v4/*:: as any*/),
      (v5/*:: as any*/),
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "status",
        "storageKey": null
      },
      (v6/*:: as any*/),
      {
        "alias": null,
        "args": null,
        "concreteType": "Actor",
        "kind": "LinkedField",
        "name": "decidedBy",
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
        "name": "grantExpiresAt",
        "storageKey": null
      }
    ],
    "storageKey": "myChallenges(first:20)"
  },
  {
    "alias": null,
    "args": null,
    "concreteType": "SecurityKey",
    "kind": "LinkedField",
    "name": "mySecurityKeys",
    "plural": true,
    "selections": [
      (v0/*:: as any*/),
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "name",
        "storageKey": null
      },
      (v6/*:: as any*/),
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
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "ChallengesPageQuery",
    "selections": (v7/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "ChallengesPageQuery",
    "selections": (v7/*:: as any*/)
  },
  "params": {
    "cacheID": "ca0c5e361ea2e2d82326be8e9cb0a67f",
    "id": null,
    "metadata": {},
    "name": "ChallengesPageQuery",
    "operationKind": "query",
    "text": "query ChallengesPageQuery {\n  canAdminister: viewerCan(action: ADMINISTER, area: CHALLENGES)\n  pending: myChallenges(status: PENDING) {\n    id\n    principal {\n      type\n      id\n    }\n    onBehalfOf {\n      type\n      id\n    }\n    action\n    resource {\n      type\n      id\n    }\n    kinds\n    passed\n    expiresAt\n  }\n  recent: myChallenges(first: 20) {\n    id\n    principal {\n      type\n      id\n    }\n    onBehalfOf {\n      type\n      id\n    }\n    action\n    resource {\n      type\n      id\n    }\n    status\n    createdAt\n    decidedBy {\n      label\n    }\n    grantExpiresAt\n  }\n  mySecurityKeys {\n    id\n    name\n    createdAt\n    lastUsedAt\n  }\n}\n"
  }
};
})();

(node as any).hash = "ea38c78a01c160b05eb200fdd0a4e8c1";

export default node;
