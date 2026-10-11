/**
 * @generated SignedSource<<137c831801933b97ccdf00d7960f6c6f>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
import { FragmentRefs } from "relay-runtime";
export type DecisionDetailQuery$variables = {
  id: string;
};
export type DecisionDetailQuery$data = {
  readonly node: {
    readonly DecisionDetail_decision: {
      readonly " $fragmentSpreads": FragmentRefs<"DecisionDetail_decision">;
    } | null | undefined;
  } | null | undefined;
};
export type DecisionDetailQuery = {
  response: DecisionDetailQuery$data;
  variables: DecisionDetailQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "id"
  }
],
v1 = [
  {
    "kind": "Variable",
    "name": "id",
    "variableName": "id"
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
],
v4 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "verdict",
  "storageKey": null
},
v5 = {
  "alias": null,
  "args": null,
  "concreteType": "PolicySource",
  "kind": "LinkedField",
  "name": "determiningPolicies",
  "plural": true,
  "selections": [
    (v2/*:: as any*/),
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "source",
      "storageKey": null
    }
  ],
  "storageKey": null
};
return {
  "fragment": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Fragment",
    "metadata": null,
    "name": "DecisionDetailQuery",
    "selections": [
      {
        "alias": null,
        "args": (v1/*:: as any*/),
        "concreteType": null,
        "kind": "LinkedField",
        "name": "node",
        "plural": false,
        "selections": [
          {
            "fragment": {
              "kind": "InlineFragment",
              "selections": [
                {
                  "args": null,
                  "kind": "FragmentSpread",
                  "name": "DecisionDetail_decision"
                }
              ],
              "type": "Decision",
              "abstractKey": null
            },
            "kind": "AliasedInlineFragmentSpread",
            "name": "DecisionDetail_decision"
          }
        ],
        "storageKey": null
      }
    ],
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "DecisionDetailQuery",
    "selections": [
      {
        "alias": null,
        "args": (v1/*:: as any*/),
        "concreteType": null,
        "kind": "LinkedField",
        "name": "node",
        "plural": false,
        "selections": [
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "__typename",
            "storageKey": null
          },
          (v2/*:: as any*/),
          {
            "kind": "InlineFragment",
            "selections": [
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
                  },
                  (v4/*:: as any*/),
                  (v5/*:: as any*/)
                ],
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "humanPresent",
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
              (v4/*:: as any*/),
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
                "name": "policyVersion",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "context",
                "storageKey": null
              },
              (v5/*:: as any*/)
            ],
            "type": "Decision",
            "abstractKey": null
          }
        ],
        "storageKey": null
      }
    ]
  },
  "params": {
    "cacheID": "34a0e32510b0bec291f00b43da39c286",
    "id": null,
    "metadata": {},
    "name": "DecisionDetailQuery",
    "operationKind": "query",
    "text": "query DecisionDetailQuery(\n  $id: ID!\n) {\n  node(id: $id) {\n    __typename\n    ...DecisionDetail_decision\n    id\n  }\n}\n\nfragment DecisionDetail_decision on Decision {\n  id\n  at\n  principal {\n    type\n    id\n  }\n  onBehalfOf {\n    human {\n      type\n      id\n    }\n    verdict\n    determiningPolicies {\n      id\n      source\n    }\n  }\n  humanPresent\n  action\n  resource {\n    type\n    id\n  }\n  verdict\n  mode\n  challenges\n  policyVersion\n  context\n  determiningPolicies {\n    id\n    source\n  }\n}\n"
  }
};
})();

(node as any).hash = "de36093c11edeb216ea29939e5d03aae";

export default node;
