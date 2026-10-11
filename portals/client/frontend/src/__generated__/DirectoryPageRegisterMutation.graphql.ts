/**
 * @generated SignedSource<<0ef90585b1db0c2f39c336fc81f674d7>>
 * @relayHash 180122e31113716b55fda2d4f4f5710a
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 8cc56bbb9ac454042ab80d1afe8efb4f3e814a9e59fa6dcd2bba59e25222b7d4

import { ConcreteRequest } from 'relay-runtime';
export type RegisterServiceAccountInput = {
  ai: boolean;
  description?: string;
  groups?: ReadonlyArray<string>;
  id: string;
  owner: string;
};
export type DirectoryPageRegisterMutation$variables = {
  input: RegisterServiceAccountInput;
};
export type DirectoryPageRegisterMutation$data = {
  readonly registerServiceAccount: {
    readonly serviceAccount: {
      readonly id: string;
    };
  };
};
export type DirectoryPageRegisterMutation = {
  response: DirectoryPageRegisterMutation$data;
  variables: DirectoryPageRegisterMutation$variables;
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
    "concreteType": "RegisterServiceAccountPayload",
    "kind": "LinkedField",
    "name": "registerServiceAccount",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "ServiceAccount",
        "kind": "LinkedField",
        "name": "serviceAccount",
        "plural": false,
        "selections": [
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "id",
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
    "name": "DirectoryPageRegisterMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "DirectoryPageRegisterMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "id": "8cc56bbb9ac454042ab80d1afe8efb4f3e814a9e59fa6dcd2bba59e25222b7d4",
    "metadata": {},
    "name": "DirectoryPageRegisterMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "3e523028376afa9303889700c5da8104";

export default node;
