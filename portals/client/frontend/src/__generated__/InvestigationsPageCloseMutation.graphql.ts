/**
 * @generated SignedSource<<29f9538abd8860d542aa0ac9e4bceb8e>>
 * @relayHash 90b983b606363dd533c5dde9d8c07196
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 5ee2d2afc89e5ce99b0c267bb817333580968c9efd05b015f620ccc4547cb11b

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
    "id": "5ee2d2afc89e5ce99b0c267bb817333580968c9efd05b015f620ccc4547cb11b",
    "metadata": {},
    "name": "InvestigationsPageCloseMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "96aeeac19d6602fb91c60c9007178956";

export default node;
