/**
 * @generated SignedSource<<8bf25dc711ecfe30d9f849bedb42064a>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
import { FragmentRefs } from "relay-runtime";
export type EnforcementMode = "ENFORCE" | "MONITOR" | "%future added value";
export type Verdict = "ALLOW" | "CHALLENGE" | "DENY" | "%future added value";
export type DecisionFilter = {
  mode?: EnforcementMode | null | undefined;
  resourceId?: string | null | undefined;
  since?: string | null | undefined;
  subjectId?: string | null | undefined;
  verdict?: Verdict | null | undefined;
};
export type DecisionsPagePaginationQuery$variables = {
  after?: string | null | undefined;
  filter?: DecisionFilter | null | undefined;
  first?: number | null | undefined;
};
export type DecisionsPagePaginationQuery$data = {
  readonly " $fragmentSpreads": FragmentRefs<"DecisionsPage_decisions">;
};
export type DecisionsPagePaginationQuery = {
  response: DecisionsPagePaginationQuery$data;
  variables: DecisionsPagePaginationQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "after"
  },
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "filter"
  },
  {
    "defaultValue": 50,
    "kind": "LocalArgument",
    "name": "first"
  }
],
v1 = [
  {
    "kind": "Variable",
    "name": "after",
    "variableName": "after"
  },
  {
    "kind": "Variable",
    "name": "filter",
    "variableName": "filter"
  },
  {
    "kind": "Variable",
    "name": "first",
    "variableName": "first"
  }
],
v2 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
},
v3 = [
  {
    "alias": null,
    "args": null,
    "kind": "ScalarField",
    "name": "type",
    "storageKey": null
  },
  (v2/*:: as any*/)
];
return {
  "fragment": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Fragment",
    "metadata": null,
    "name": "DecisionsPagePaginationQuery",
    "selections": [
      {
        "args": (v1/*:: as any*/),
        "kind": "FragmentSpread",
        "name": "DecisionsPage_decisions"
      }
    ],
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "DecisionsPagePaginationQuery",
    "selections": [
      {
        "alias": null,
        "args": (v1/*:: as any*/),
        "concreteType": "DecisionConnection",
        "kind": "LinkedField",
        "name": "decisions",
        "plural": false,
        "selections": [
          {
            "alias": null,
            "args": null,
            "concreteType": "DecisionEdge",
            "kind": "LinkedField",
            "name": "edges",
            "plural": true,
            "selections": [
              {
                "alias": null,
                "args": null,
                "concreteType": "Decision",
                "kind": "LinkedField",
                "name": "node",
                "plural": false,
                "selections": [
                  (v2/*:: as any*/),
                  {
                    "alias": null,
                    "args": null,
                    "kind": "ScalarField",
                    "name": "at",
                    "storageKey": null
                  },
                  {
                    "alias": null,
                    "args": null,
                    "concreteType": "EntityRef",
                    "kind": "LinkedField",
                    "name": "subject",
                    "plural": false,
                    "selections": (v3/*:: as any*/),
                    "storageKey": null
                  },
                  {
                    "alias": null,
                    "args": null,
                    "concreteType": "OnBehalfOf",
                    "kind": "LinkedField",
                    "name": "onBehalfOf",
                    "plural": false,
                    "selections": [
                      {
                        "alias": null,
                        "args": null,
                        "concreteType": "EntityRef",
                        "kind": "LinkedField",
                        "name": "human",
                        "plural": false,
                        "selections": (v3/*:: as any*/),
                        "storageKey": null
                      }
                    ],
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
                    "selections": (v3/*:: as any*/),
                    "storageKey": null
                  },
                  {
                    "alias": null,
                    "args": null,
                    "kind": "ScalarField",
                    "name": "verdict",
                    "storageKey": null
                  },
                  {
                    "alias": null,
                    "args": null,
                    "kind": "ScalarField",
                    "name": "mode",
                    "storageKey": null
                  },
                  {
                    "alias": null,
                    "args": null,
                    "kind": "ScalarField",
                    "name": "challenges",
                    "storageKey": null
                  },
                  {
                    "alias": null,
                    "args": null,
                    "kind": "ScalarField",
                    "name": "policies",
                    "storageKey": null
                  },
                  {
                    "alias": null,
                    "args": null,
                    "kind": "ScalarField",
                    "name": "__typename",
                    "storageKey": null
                  }
                ],
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "cursor",
                "storageKey": null
              }
            ],
            "storageKey": null
          },
          {
            "alias": null,
            "args": null,
            "concreteType": "PageInfo",
            "kind": "LinkedField",
            "name": "pageInfo",
            "plural": false,
            "selections": [
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "endCursor",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "hasNextPage",
                "storageKey": null
              }
            ],
            "storageKey": null
          }
        ],
        "storageKey": null
      },
      {
        "alias": null,
        "args": (v1/*:: as any*/),
        "filters": [
          "filter"
        ],
        "handle": "connection",
        "key": "DecisionsPage_decisions",
        "kind": "LinkedHandle",
        "name": "decisions"
      }
    ]
  },
  "params": {
    "cacheID": "b7b7060e8e2c6f451ed79c3b3419a5be",
    "id": null,
    "metadata": {},
    "name": "DecisionsPagePaginationQuery",
    "operationKind": "query",
    "text": "query DecisionsPagePaginationQuery(\n  $after: String\n  $filter: DecisionFilter\n  $first: Int = 50\n) {\n  ...DecisionsPage_decisions_G9cLv\n}\n\nfragment DecisionsPage_decisions_G9cLv on Query {\n  decisions(filter: $filter, first: $first, after: $after) {\n    edges {\n      node {\n        id\n        at\n        subject {\n          type\n          id\n        }\n        onBehalfOf {\n          human {\n            type\n            id\n          }\n        }\n        action\n        resource {\n          type\n          id\n        }\n        verdict\n        mode\n        challenges\n        policies\n        __typename\n      }\n      cursor\n    }\n    pageInfo {\n      endCursor\n      hasNextPage\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "982978d5aefb142e5b1ee8cd2ec7bac6";

export default node;
