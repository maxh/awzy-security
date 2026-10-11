/**
 * @generated SignedSource<<02aa00951524189bce89d352a121a8a0>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type EnforcementMode = "ENFORCE" | "MONITOR" | "%future added value";
export type EnforcementScope = "AUTOMATED_CONTAINMENTS" | "INSTALL" | "%future added value";
export type SetEnforcementModeInput = {
  mode: EnforcementMode;
  scope?: EnforcementScope;
};
export type EnforcementPageSetModeMutation$variables = {
  input: SetEnforcementModeInput;
};
export type EnforcementPageSetModeMutation$data = {
  readonly setEnforcementMode: {
    readonly challengeQid: string | null | undefined;
    readonly enforcement: {
      readonly changedAt: string | null | undefined;
      readonly changedBy: {
        readonly label: string;
      } | null | undefined;
      readonly mode: EnforcementMode;
    } | null | undefined;
  };
};
export type EnforcementPageSetModeMutation = {
  response: EnforcementPageSetModeMutation$data;
  variables: EnforcementPageSetModeMutation$variables;
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
    "concreteType": "SetEnforcementModePayload",
    "kind": "LinkedField",
    "name": "setEnforcementMode",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "EnforcementSetting",
        "kind": "LinkedField",
        "name": "enforcement",
        "plural": false,
        "selections": [
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
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "challengeQid",
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
    "name": "EnforcementPageSetModeMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "EnforcementPageSetModeMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "10c289993e13974506f6fde744eb10c6",
    "id": null,
    "metadata": {},
    "name": "EnforcementPageSetModeMutation",
    "operationKind": "mutation",
    "text": "mutation EnforcementPageSetModeMutation(\n  $input: SetEnforcementModeInput!\n) {\n  setEnforcementMode(input: $input) {\n    enforcement {\n      mode\n      changedBy {\n        label\n      }\n      changedAt\n    }\n    challengeQid\n  }\n}\n"
  }
};
})();

(node as any).hash = "426a5b6bc923fbaa72e9775a52f0008e";

export default node;
