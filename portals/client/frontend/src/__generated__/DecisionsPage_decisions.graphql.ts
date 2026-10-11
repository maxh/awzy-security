/**
 * @generated SignedSource<<dd2ad7f6d53113f652772448498dcdbf>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from 'relay-runtime';
export type EnforcementMode = "ENFORCE" | "MONITOR" | "%future added value";
export type Verdict = "ALLOW" | "CHALLENGE" | "DENY" | "%future added value";
import { FragmentRefs } from "relay-runtime";
export type DecisionsPage_decisions$data = {
  readonly decisions: {
    readonly edges: ReadonlyArray<{
      readonly node: {
        readonly action: string;
        readonly at: string;
        readonly challenges: ReadonlyArray<string>;
        readonly id: string;
        readonly mode: EnforcementMode;
        readonly onBehalfOf: {
          readonly human: {
            readonly id: string;
            readonly type: string;
          };
        } | null | undefined;
        readonly policies: ReadonlyArray<string>;
        readonly principal: {
          readonly id: string;
          readonly type: string;
        };
        readonly resource: {
          readonly id: string;
          readonly type: string;
        };
        readonly verdict: Verdict;
      };
    }>;
  };
  readonly " $fragmentType": "DecisionsPage_decisions";
};
export type DecisionsPage_decisions$key = {
  readonly " $data"?: DecisionsPage_decisions$data;
  readonly " $fragmentSpreads": FragmentRefs<"DecisionsPage_decisions">;
};

import DecisionsPagePaginationQuery_graphql from './DecisionsPagePaginationQuery.graphql';

const node: ReaderFragment = (function(){
var v0 = [
  "decisions"
],
v1 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
},
v2 = [
  {
    "alias": null,
    "args": null,
    "kind": "ScalarField",
    "name": "type",
    "storageKey": null
  },
  (v1/*:: as any*/)
];
return {
  "argumentDefinitions": [
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
  "kind": "Fragment",
  "metadata": {
    "connection": [
      {
        "count": "first",
        "cursor": "after",
        "direction": "forward",
        "path": (v0/*:: as any*/)
      }
    ],
    "refetch": {
      "connection": {
        "forward": {
          "count": "first",
          "cursor": "after"
        },
        "backward": null,
        "path": (v0/*:: as any*/)
      },
      "fragmentPathInResult": [],
      "operation": DecisionsPagePaginationQuery_graphql
    }
  },
  "name": "DecisionsPage_decisions",
  "selections": [
    {
      "alias": "decisions",
      "args": [
        {
          "kind": "Variable",
          "name": "filter",
          "variableName": "filter"
        }
      ],
      "concreteType": "DecisionConnection",
      "kind": "LinkedField",
      "name": "__DecisionsPage_decisions_connection",
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
                (v1/*:: as any*/),
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
                  "selections": (v2/*:: as any*/),
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
                      "selections": (v2/*:: as any*/),
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
                  "selections": (v2/*:: as any*/),
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
    }
  ],
  "type": "Query",
  "abstractKey": null
};
})();

(node as any).hash = "64de53bf86e1737c02ae16fcdf2843c9";

export default node;
