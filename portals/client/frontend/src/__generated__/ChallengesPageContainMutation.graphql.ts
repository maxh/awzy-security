/**
 * @generated SignedSource<<563e616a6a3749b83ea6f176fbbb3f0e>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ContainAccessorInput = {
  reason: string;
  subject: EntityRefInput;
};
export type EntityRefInput = {
  id: string;
  type: string;
};
export type ChallengesPageContainMutation$variables = {
  input: ContainAccessorInput;
};
export type ChallengesPageContainMutation$data = {
  readonly containAccessor: {
    readonly containment: {
      readonly id: string;
    };
  };
};
export type ChallengesPageContainMutation = {
  response: ChallengesPageContainMutation$data;
  variables: ChallengesPageContainMutation$variables;
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
    "concreteType": "ContainmentPayload",
    "kind": "LinkedField",
    "name": "containAccessor",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "Containment",
        "kind": "LinkedField",
        "name": "containment",
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
    "name": "ChallengesPageContainMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "ChallengesPageContainMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "645215a24e5d2f3d248e273866b627c8",
    "id": null,
    "metadata": {},
    "name": "ChallengesPageContainMutation",
    "operationKind": "mutation",
    "text": "mutation ChallengesPageContainMutation(\n  $input: ContainAccessorInput!\n) {\n  containAccessor(input: $input) {\n    containment {\n      id\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "bd86084bdfdf8f4f93c195ca7b2f8b48";

export default node;
