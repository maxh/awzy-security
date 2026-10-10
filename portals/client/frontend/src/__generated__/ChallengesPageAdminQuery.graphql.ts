/**
 * @generated SignedSource<<618ff60c232a77b72a70634fcd7c89ae>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ChallengeKind = "APPROVAL" | "BIOMETRIC" | "JUSTIFICATION" | "VERIFICATION" | "%future added value";
export type ChallengesPageAdminQuery$variables = Record<PropertyKey, never>;
export type ChallengesPageAdminQuery$data = {
  readonly containments: ReadonlyArray<{
    readonly createdAt: string | null | undefined;
    readonly createdBy: {
      readonly label: string;
    } | null | undefined;
    readonly id: string;
    readonly monitor: boolean;
    readonly reason: string;
    readonly subject: {
      readonly id: string;
      readonly type: string;
    } | null | undefined;
  }>;
  readonly pending: ReadonlyArray<{
    readonly action: string;
    readonly answererId: string | null | undefined;
    readonly createdAt: string | null | undefined;
    readonly id: string;
    readonly justification: string | null | undefined;
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
};
export type ChallengesPageAdminQuery = {
  response: ChallengesPageAdminQuery$data;
  variables: ChallengesPageAdminQuery$variables;
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
  "kind": "ScalarField",
  "name": "createdAt",
  "storageKey": null
},
v4 = [
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
    "name": "challenges",
    "plural": true,
    "selections": [
      (v0/*:: as any*/),
      (v2/*:: as any*/),
      {
        "alias": null,
        "args": null,
        "concreteType": "EntityRef",
        "kind": "LinkedField",
        "name": "onBehalfOf",
        "plural": false,
        "selections": (v1/*:: as any*/),
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "action",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "concreteType": "EntityRef",
        "kind": "LinkedField",
        "name": "resource",
        "plural": false,
        "selections": (v1/*:: as any*/),
        "storageKey": null
      },
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
        "name": "answererId",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "justification",
        "storageKey": null
      },
      (v3/*:: as any*/)
    ],
    "storageKey": "challenges(status:\"PENDING\")"
  },
  {
    "alias": null,
    "args": null,
    "concreteType": "Containment",
    "kind": "LinkedField",
    "name": "containments",
    "plural": true,
    "selections": [
      (v0/*:: as any*/),
      (v2/*:: as any*/),
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "reason",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "concreteType": "Actor",
        "kind": "LinkedField",
        "name": "createdBy",
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
      (v3/*:: as any*/),
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "monitor",
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
    "name": "ChallengesPageAdminQuery",
    "selections": (v4/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "ChallengesPageAdminQuery",
    "selections": (v4/*:: as any*/)
  },
  "params": {
    "cacheID": "0b43f589af1569ee2d4343da902d4f29",
    "id": null,
    "metadata": {},
    "name": "ChallengesPageAdminQuery",
    "operationKind": "query",
    "text": "query ChallengesPageAdminQuery {\n  pending: challenges(status: PENDING) {\n    id\n    subject {\n      type\n      id\n    }\n    onBehalfOf {\n      type\n      id\n    }\n    action\n    resource {\n      type\n      id\n    }\n    kinds\n    passed\n    answererId\n    justification\n    createdAt\n  }\n  containments {\n    id\n    subject {\n      type\n      id\n    }\n    reason\n    createdBy {\n      label\n    }\n    createdAt\n    monitor\n  }\n}\n"
  }
};
})();

(node as any).hash = "235f976cfe53e67d365bae01cd20a841";

export default node;
