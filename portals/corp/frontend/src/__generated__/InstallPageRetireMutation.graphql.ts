/**
 * @generated SignedSource<<bc24670a700489c05f2f23ccbb319923>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type RetireInstallInput = {
  installQid: string;
};
export type InstallPageRetireMutation$variables = {
  input: RetireInstallInput;
};
export type InstallPageRetireMutation$data = {
  readonly retireInstall: {
    readonly install: {
      readonly id: string;
      readonly retiredAt: string | null | undefined;
    };
  };
};
export type InstallPageRetireMutation = {
  response: InstallPageRetireMutation$data;
  variables: InstallPageRetireMutation$variables;
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
    "concreteType": "RetireInstallPayload",
    "kind": "LinkedField",
    "name": "retireInstall",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "Install",
        "kind": "LinkedField",
        "name": "install",
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
            "name": "retiredAt",
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
    "name": "InstallPageRetireMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "InstallPageRetireMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "77193d1036930d84b27f9ef9f893411f",
    "id": null,
    "metadata": {},
    "name": "InstallPageRetireMutation",
    "operationKind": "mutation",
    "text": "mutation InstallPageRetireMutation(\n  $input: RetireInstallInput!\n) {\n  retireInstall(input: $input) {\n    install {\n      id\n      retiredAt\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "1fdd46408fc7c77d21382e989a30e360";

export default node;
