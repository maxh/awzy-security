/**
 * @generated SignedSource<<e4f5c6e6c41ec59fd261f7b2c98ca8fc>>
 * @relayHash ca61b032e1c00e628816acf8f2c901e8
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID b6503b365c8c7bdfc04c05b805e844a68bbb6e4e9711f64ac763f6dd1c1e7764

import { ConcreteRequest } from 'relay-runtime';
import { FragmentRefs } from "relay-runtime";
export type EnforcementMode = "ENFORCE" | "MONITOR" | "%future added value";
export type Verdict = "ALLOW" | "CHALLENGE" | "DENY" | "%future added value";
export type DecisionFilter = {
  mode?: EnforcementMode | null | undefined;
  principalId?: string | null | undefined;
  resourceId?: string | null | undefined;
  since?: string | null | undefined;
  verdict?: Verdict | null | undefined;
};
export type DecisionsPageQuery$variables = {
  filter?: DecisionFilter | null | undefined;
};
export type DecisionsPageQuery$data = {
  readonly " $fragmentSpreads": FragmentRefs<"DecisionsPage_decisions">;
};
export type DecisionsPageQuery = {
  response: DecisionsPageQuery$data;
  variables: DecisionsPageQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "filter"
  }
],
v1 = {
  "kind": "Variable",
  "name": "filter",
  "variableName": "filter"
},
v2 = [
  (v1/*:: as any*/),
  {
    "kind": "Literal",
    "name": "first",
    "value": 50
  }
],
v3 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
},
v4 = [
  {
    "alias": null,
    "args": null,
    "kind": "ScalarField",
    "name": "type",
    "storageKey": null
  },
  (v3/*:: as any*/)
];
return {
  "fragment": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Fragment",
    "metadata": null,
    "name": "DecisionsPageQuery",
    "selections": [
      {
        "args": [
          (v1/*:: as any*/)
        ],
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
    "name": "DecisionsPageQuery",
    "selections": [
      {
        "alias": null,
        "args": (v2/*:: as any*/),
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
                  (v3/*:: as any*/),
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
                    "name": "principal",
                    "plural": false,
                    "selections": (v4/*:: as any*/),
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
                        "selections": (v4/*:: as any*/),
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
                    "selections": (v4/*:: as any*/),
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
        "args": (v2/*:: as any*/),
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
    "id": "b6503b365c8c7bdfc04c05b805e844a68bbb6e4e9711f64ac763f6dd1c1e7764",
    "metadata": {},
    "name": "DecisionsPageQuery",
    "operationKind": "query",
    "text": null
  }
};
})();

(node as any).hash = "6dad67a521e53e1966d09fa867697462";

export default node;
