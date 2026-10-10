/**
 * @generated SignedSource<<8e50cf840cb1c7b39297291e1b127b01>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type DryRunReportQuery$variables = {
  draftId: string;
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
        readonly resourceId: string;
        readonly subjectId: string;
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
    "name": "draftId"
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
        "name": "draftId",
        "variableName": "draftId"
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
                "name": "subjectId",
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
    "cacheID": "4760dfc91a7d947c1cab6ccfe5ea4ad1",
    "id": null,
    "metadata": {},
    "name": "DryRunReportQuery",
    "operationKind": "query",
    "text": "query DryRunReportQuery(\n  $draftId: ID!\n) {\n  policyDryRun(draftId: $draftId) {\n    days\n    comparedWithVersion\n    decisions\n    distinctRequests\n    replayedRequests\n    truncated\n    unchangedDecisions\n    skippedRequests\n    skippedReason\n    changes {\n      from\n      to\n      decisions\n      requests\n      examples {\n        subjectId\n        onBehalfOfId\n        action\n        resourceId\n        decisions\n        policiesBefore\n        policiesAfter\n      }\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "abba76e319d607cc9a8414abd41f1d1f";

export default node;
