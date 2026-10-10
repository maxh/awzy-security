/**
 * @generated SignedSource<<ebe18640926b3a96416fa54b50108053>>
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
    readonly resource: {
      readonly id: string;
      readonly type: string;
    } | null | undefined;
    readonly subject: {
      readonly id: string;
      readonly type: string;
    } | null | undefined;
  }>;
  readonly recent: ReadonlyArray<{
    readonly action: string;
    readonly createdAt: string | null | undefined;
    readonly decidedBy: string | null | undefined;
    readonly grantExpiresAt: string | null | undefined;
    readonly id: string;
    readonly onBehalfOf: {
      readonly id: string;
      readonly type: string;
    } | null | undefined;
    readonly resource: {
      readonly id: string;
      readonly type: string;
    } | null | undefined;
    readonly status: ChallengeStatus;
    readonly subject: {
      readonly id: string;
      readonly type: string;
    } | null | undefined;
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
  "name": "subject",
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
v6 = [
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
        "name": "decidedBy",
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
  }
];
return {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "ChallengesPageQuery",
    "selections": (v6/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "ChallengesPageQuery",
    "selections": (v6/*:: as any*/)
  },
  "params": {
    "cacheID": "1857bbe6088edae8389652e9cfd753d9",
    "id": null,
    "metadata": {},
    "name": "ChallengesPageQuery",
    "operationKind": "query",
    "text": "query ChallengesPageQuery {\n  canAdminister: viewerCan(action: ADMINISTER, area: CHALLENGES)\n  pending: myChallenges(status: PENDING) {\n    id\n    subject {\n      type\n      id\n    }\n    onBehalfOf {\n      type\n      id\n    }\n    action\n    resource {\n      type\n      id\n    }\n    kinds\n    passed\n    expiresAt\n  }\n  recent: myChallenges(first: 20) {\n    id\n    subject {\n      type\n      id\n    }\n    onBehalfOf {\n      type\n      id\n    }\n    action\n    resource {\n      type\n      id\n    }\n    status\n    createdAt\n    decidedBy\n    grantExpiresAt\n  }\n}\n"
  }
};
})();

(node as any).hash = "dbd2e7d15054cfcee8c362afe741c656";

export default node;
