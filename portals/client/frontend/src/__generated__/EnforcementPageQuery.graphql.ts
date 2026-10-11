/**
 * @generated SignedSource<<b4b51df9e59d1d2b13746a563df22e95>>
 * @relayHash 072b28e4f4ff16bea2a9105c78497c6a
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 05cf2158ae47947bc8c18e3ebfb6b34348acd0619de508d30cae509c1c3b2667

import { ConcreteRequest } from 'relay-runtime';
export type EnforcementMode = "ENFORCE" | "MONITOR" | "%future added value";
export type EnforcementPageQuery$variables = Record<PropertyKey, never>;
export type EnforcementPageQuery$data = {
  readonly canSwitch: boolean;
  readonly containmentEnforcement: {
    readonly changedAt: string | null | undefined;
    readonly changedBy: {
      readonly label: string;
    } | null | undefined;
    readonly mode: EnforcementMode;
  };
  readonly enforcement: {
    readonly changedAt: string | null | undefined;
    readonly changedBy: {
      readonly label: string;
    } | null | undefined;
    readonly mode: EnforcementMode;
  };
  readonly enforcementHistory: ReadonlyArray<{
    readonly changedAt: string | null | undefined;
    readonly changedBy: {
      readonly label: string;
    } | null | undefined;
    readonly mode: EnforcementMode;
  }>;
  readonly enforcementSummary: {
    readonly enforcedChallenge: number;
    readonly enforcedDeny: number;
    readonly monitored: number;
    readonly since: string;
    readonly total: number;
    readonly wouldChallenge: number;
    readonly wouldDeny: number;
  };
  readonly wouldDeny: {
    readonly edges: ReadonlyArray<{
      readonly node: {
        readonly action: string;
        readonly at: string;
        readonly id: string;
        readonly policies: ReadonlyArray<string>;
        readonly principal: {
          readonly id: string;
          readonly type: string;
        };
        readonly resource: {
          readonly id: string;
          readonly type: string;
        };
      };
    }>;
  };
};
export type EnforcementPageQuery = {
  response: EnforcementPageQuery$data;
  variables: EnforcementPageQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
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
    "concreteType": "Actor",
    "kind": "LinkedField",
    "name": "changedBy",
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
    "name": "changedAt",
    "storageKey": null
  }
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
],
v3 = [
  {
    "alias": "canSwitch",
    "args": [
      {
        "kind": "Literal",
        "name": "action",
        "value": "ADMINISTER"
      },
      {
        "kind": "Literal",
        "name": "area",
        "value": "ENFORCEMENT"
      }
    ],
    "kind": "ScalarField",
    "name": "viewerCan",
    "storageKey": "viewerCan(action:\"ADMINISTER\",area:\"ENFORCEMENT\")"
  },
  {
    "alias": null,
    "args": null,
    "concreteType": "EnforcementSetting",
    "kind": "LinkedField",
    "name": "enforcement",
    "plural": false,
    "selections": (v0/*:: as any*/),
    "storageKey": null
  },
  {
    "alias": "containmentEnforcement",
    "args": [
      {
        "kind": "Literal",
        "name": "scope",
        "value": "AUTOMATED_CONTAINMENTS"
      }
    ],
    "concreteType": "EnforcementSetting",
    "kind": "LinkedField",
    "name": "enforcement",
    "plural": false,
    "selections": (v0/*:: as any*/),
    "storageKey": "enforcement(scope:\"AUTOMATED_CONTAINMENTS\")"
  },
  {
    "alias": null,
    "args": null,
    "concreteType": "EnforcementSummary",
    "kind": "LinkedField",
    "name": "enforcementSummary",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "since",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "total",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "monitored",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "wouldDeny",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "wouldChallenge",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "enforcedDeny",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "enforcedChallenge",
        "storageKey": null
      }
    ],
    "storageKey": null
  },
  {
    "alias": "wouldDeny",
    "args": [
      {
        "kind": "Literal",
        "name": "filter",
        "value": {
          "mode": "MONITOR",
          "verdict": "DENY"
        }
      },
      {
        "kind": "Literal",
        "name": "first",
        "value": 5
      }
    ],
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
                "name": "policies",
                "storageKey": null
              }
            ],
            "storageKey": null
          }
        ],
        "storageKey": null
      }
    ],
    "storageKey": "decisions(filter:{\"mode\":\"MONITOR\",\"verdict\":\"DENY\"},first:5)"
  },
  {
    "alias": null,
    "args": [
      {
        "kind": "Literal",
        "name": "first",
        "value": 10
      }
    ],
    "concreteType": "EnforcementSetting",
    "kind": "LinkedField",
    "name": "enforcementHistory",
    "plural": true,
    "selections": (v0/*:: as any*/),
    "storageKey": "enforcementHistory(first:10)"
  }
];
return {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "EnforcementPageQuery",
    "selections": (v3/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "EnforcementPageQuery",
    "selections": (v3/*:: as any*/)
  },
  "params": {
    "id": "05cf2158ae47947bc8c18e3ebfb6b34348acd0619de508d30cae509c1c3b2667",
    "metadata": {},
    "name": "EnforcementPageQuery",
    "operationKind": "query",
    "text": null
  }
};
})();

(node as any).hash = "d24d123db66a49c6b7cbb28b1405b6f4";

export default node;
