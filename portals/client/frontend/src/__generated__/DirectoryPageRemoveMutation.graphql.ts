/**
 * @generated SignedSource<<f32d1eb185b2a8923584a2981c1545ce>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type RemoveServiceAccountInput = {
  id: string;
};
export type DirectoryPageRemoveMutation$variables = {
  input: RemoveServiceAccountInput;
};
export type DirectoryPageRemoveMutation$data = {
  readonly removeServiceAccount: {
    readonly removedId: string;
  };
};
export type DirectoryPageRemoveMutation = {
  response: DirectoryPageRemoveMutation$data;
  variables: DirectoryPageRemoveMutation$variables;
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
    "concreteType": "RemoveServiceAccountPayload",
    "kind": "LinkedField",
    "name": "removeServiceAccount",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "removedId",
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
    "name": "DirectoryPageRemoveMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "DirectoryPageRemoveMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "6a01792a739bc4cbc4a49aba31e0c463",
    "id": null,
    "metadata": {},
    "name": "DirectoryPageRemoveMutation",
    "operationKind": "mutation",
    "text": "mutation DirectoryPageRemoveMutation(\n  $input: RemoveServiceAccountInput!\n) {\n  removeServiceAccount(input: $input) {\n    removedId\n  }\n}\n"
  }
};
})();

(node as any).hash = "3c13d10e690553c4a6796709e3651de1";

export default node;
