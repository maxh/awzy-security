/**
 * @generated SignedSource<<5b7e398612be602be50862c30252bd2f>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type InvestigationStatus = "CLOSED" | "DONE" | "NEEDS_REVIEW" | "%future added value";
export type CloseInvestigationInput = {
  investigationQid: string;
  reason: string;
};
export type InvestigationsPageCloseMutation$variables = {
  input: CloseInvestigationInput;
};
export type InvestigationsPageCloseMutation$data = {
  readonly closeInvestigation: {
    readonly investigation: {
      readonly closeReason: string | null | undefined;
      readonly closedAt: string | null | undefined;
      readonly closedBy: {
        readonly label: string;
      } | null | undefined;
      readonly id: string;
      readonly status: InvestigationStatus;
    };
  };
};
export type InvestigationsPageCloseMutation = {
  response: InvestigationsPageCloseMutation$data;
  variables: InvestigationsPageCloseMutation$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "input"
  }
],
v1 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Variable",
        "name": "input",
        "variableName": "input"
      }
    ],
    "concreteType": "CloseInvestigationPayload",
    "kind": "LinkedField",
    "name": "closeInvestigation",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "Investigation",
        "kind": "LinkedField",
        "name": "investigation",
        "plural": false,
        "selections": [
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "id",
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
    ],
    "storageKey": null
  }
];
return {
  "fragment": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Fragment",
    "metadata": null,
    "name": "InvestigationsPageCloseMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "InvestigationsPageCloseMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "90b983b606363dd533c5dde9d8c07196",
    "id": null,
    "metadata": {},
    "name": "InvestigationsPageCloseMutation",
    "operationKind": "mutation",
    "text": "mutation InvestigationsPageCloseMutation(\n  $input: CloseInvestigationInput!\n) {\n  closeInvestigation(input: $input) {\n    investigation {\n      id\n      status\n      closedBy {\n        label\n      }\n      closedAt\n      closeReason\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "96aeeac19d6602fb91c60c9007178956";

export default node;
