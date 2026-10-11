/**
 * @generated SignedSource<<dde65ea0b20ffca5df876759c5047ab0>>
 * @relayHash 15a0876436173315aa0ac792156bae01
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 1922b51be10435bd8d4d430224602e24f6aeb0a4c6a97a779932fdc1bfeaa687

import { ConcreteRequest } from 'relay-runtime';
export type DryRunReportQuery$variables = {
  draftQid: string;
};
export type DryRunReportQuery$data = {
  readonly policyDryRun: {
    readonly changes: ReadonlyArray<{
      readonly decisions: number;
      readonly examples: ReadonlyArray<{
        readonly action: string;
        readonly decisions: number;
        readonly onBehalfOfId: string | null | undefined;
        readonly policiesAfter: ReadonlyArray<string>;
        readonly policiesBefore: ReadonlyArray<string>;
        readonly principalId: string;
        readonly resourceId: string;
      }>;
      readonly from: string;
      readonly requests: number;
      readonly to: string;
    }>;
    readonly comparedWithVersion: number;
    readonly days: number;
    readonly decisions: number;
    readonly distinctRequests: number;
    readonly replayedRequests: number;
    readonly skippedReason: string | null | undefined;
    readonly skippedRequests: number;
    readonly truncated: boolean;
    readonly unchangedDecisions: number;
  };
};
export type DryRunReportQuery = {
  response: DryRunReportQuery$data;
  variables: DryRunReportQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "draftQid"
  }
],
v1 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "decisions",
  "storageKey": null
},
v2 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Variable",
        "name": "draftQid",
        "variableName": "draftQid"
      }
    ],
    "concreteType": "PolicyDryRun",
    "kind": "LinkedField",
    "name": "policyDryRun",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "days",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "comparedWithVersion",
        "storageKey": null
      },
      (v1/*:: as any*/),
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "distinctRequests",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "replayedRequests",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "truncated",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "unchangedDecisions",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "skippedRequests",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "skippedReason",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "concreteType": "PolicyDryRunChange",
        "kind": "LinkedField",
        "name": "changes",
        "plural": true,
        "selections": [
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "from",
            "storageKey": null
          },
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "to",
            "storageKey": null
          },
          (v1/*:: as any*/),
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "requests",
            "storageKey": null
          },
          {
            "alias": null,
            "args": null,
            "concreteType": "PolicyDryRunExample",
            "kind": "LinkedField",
            "name": "examples",
            "plural": true,
            "selections": [
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "principalId",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "onBehalfOfId",
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
                "kind": "ScalarField",
                "name": "resourceId",
                "storageKey": null
              },
              (v1/*:: as any*/),
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "policiesBefore",
                "storageKey": null
              },
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "policiesAfter",
                "storageKey": null
              }
            ],
            "storageKey": null
          }
        ],
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
    "name": "DryRunReportQuery",
    "selections": (v2/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "DryRunReportQuery",
    "selections": (v2/*:: as any*/)
  },
  "params": {
    "id": "1922b51be10435bd8d4d430224602e24f6aeb0a4c6a97a779932fdc1bfeaa687",
    "metadata": {},
    "name": "DryRunReportQuery",
    "operationKind": "query",
    "text": null
  }
};
})();

(node as any).hash = "8e69e30cc8330efe7b70c3db5cda2be2";

export default node;
