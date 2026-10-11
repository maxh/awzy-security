/**
 * @generated SignedSource<<ce58c38bdf1ddf70420924f537814643>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type InvestigationContainment = "NONE" | "PROPOSE" | "RECOMMEND" | "%future added value";
export type InvestigationStatus = "CLOSED" | "DONE" | "NEEDS_REVIEW" | "%future added value";
export type InvestigationVerdict = "BENIGN" | "INCONCLUSIVE" | "MALICIOUS" | "SUSPICIOUS" | "%future added value";
export type InvestigationsPageQuery$variables = {
  status?: InvestigationStatus | null | undefined;
};
export type InvestigationsPageQuery$data = {
  readonly canClose: boolean;
  readonly investigations: ReadonlyArray<{
    readonly closeReason: string | null | undefined;
    readonly closedAt: string | null | undefined;
    readonly closedBy: {
      readonly label: string;
    } | null | undefined;
    readonly containment: InvestigationContainment;
    readonly evidence: ReadonlyArray<{
      readonly evidenceId: string;
      readonly why: string;
    }>;
    readonly finishedAt: string;
    readonly floor: number;
    readonly id: string;
    readonly inputTokens: number;
    readonly model: string;
    readonly notes: ReadonlyArray<string>;
    readonly outputTokens: number;
    readonly principal: {
      readonly id: string;
      readonly type: string;
    };
    readonly risk: number;
    readonly status: InvestigationStatus;
    readonly summary: string;
    readonly trigger: string;
    readonly verdict: InvestigationVerdict;
  }>;
};
export type InvestigationsPageQuery = {
  response: InvestigationsPageQuery$data;
  variables: InvestigationsPageQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "status"
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
    "alias": "canClose",
    "args": [
      {
        "kind": "Literal",
        "name": "action",
        "value": "ADMINISTER"
      },
      {
        "kind": "Literal",
        "name": "area",
        "value": "INVESTIGATIONS"
      }
    ],
    "kind": "ScalarField",
    "name": "viewerCan",
    "storageKey": "viewerCan(action:\"ADMINISTER\",area:\"INVESTIGATIONS\")"
  },
  {
    "alias": null,
    "args": [
      {
        "kind": "Literal",
        "name": "first",
        "value": 100
      },
      {
        "kind": "Variable",
        "name": "status",
        "variableName": "status"
      }
    ],
    "concreteType": "Investigation",
    "kind": "LinkedField",
    "name": "investigations",
    "plural": true,
    "selections": [
      (v1/*:: as any*/),
      {
        "alias": null,
        "args": null,
        "concreteType": "EntityRef",
        "kind": "LinkedField",
        "name": "principal",
        "plural": false,
        "selections": [
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "type",
            "storageKey": null
          },
          (v1/*:: as any*/)
        ],
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "trigger",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "floor",
        "storageKey": null
      },
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
        "name": "verdict",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "risk",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "summary",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "concreteType": "CitedEvidence",
        "kind": "LinkedField",
        "name": "evidence",
        "plural": true,
        "selections": [
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "evidenceId",
            "storageKey": null
          },
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "why",
            "storageKey": null
          }
        ],
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "containment",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "notes",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "model",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "inputTokens",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "outputTokens",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "finishedAt",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "concreteType": "Actor",
        "kind": "LinkedField",
        "name": "closedBy",
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
        "name": "closedAt",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "closeReason",
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
    "name": "InvestigationsPageQuery",
    "selections": (v2/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "InvestigationsPageQuery",
    "selections": (v2/*:: as any*/)
  },
  "params": {
    "cacheID": "7b2821b51818af82f530c0fb65d8e045",
    "id": null,
    "metadata": {},
    "name": "InvestigationsPageQuery",
    "operationKind": "query",
    "text": "query InvestigationsPageQuery(\n  $status: InvestigationStatus\n) {\n  canClose: viewerCan(action: ADMINISTER, area: INVESTIGATIONS)\n  investigations(status: $status, first: 100) {\n    id\n    principal {\n      type\n      id\n    }\n    trigger\n    floor\n    status\n    verdict\n    risk\n    summary\n    evidence {\n      evidenceId\n      why\n    }\n    containment\n    notes\n    model\n    inputTokens\n    outputTokens\n    finishedAt\n    closedBy {\n      label\n    }\n    closedAt\n    closeReason\n  }\n}\n"
  }
};
})();

(node as any).hash = "79d43411f4e7625621f00adfaf5275cb";

export default node;
