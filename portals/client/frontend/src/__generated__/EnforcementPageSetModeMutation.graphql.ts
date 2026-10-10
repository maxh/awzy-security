/**
 * @generated SignedSource<<099e4f669291ff38f174fcd1987b8112>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type EnforcementMode = "ENFORCE" | "MONITOR" | "%future added value";
export type SetEnforcementModeInput = {
  mode: EnforcementMode;
};
export type EnforcementPageSetModeMutation$variables = {
  input: SetEnforcementModeInput;
};
export type EnforcementPageSetModeMutation$data = {
  readonly setEnforcementMode: {
    readonly enforcement: {
      readonly changedAt: string | null | undefined;
      readonly changedBy: string | null | undefined;
      readonly mode: EnforcementMode;
    };
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
            "kind": "ScalarField",
            "name": "changedBy",
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
    "cacheID": "7d5e866464a92e78972dca05c5443140",
    "id": null,
    "metadata": {},
    "name": "EnforcementPageSetModeMutation",
    "operationKind": "mutation",
    "text": "mutation EnforcementPageSetModeMutation(\n  $input: SetEnforcementModeInput!\n) {\n  setEnforcementMode(input: $input) {\n    enforcement {\n      mode\n      changedBy\n      changedAt\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "d86e1b7932c01bc0315a4f153ec01619";

export default node;
