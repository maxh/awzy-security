/**
 * @generated SignedSource<<e7bb2ee03663fa268083806cbe988e29>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type LiftContainmentInput = {
  id: string;
  reason: string;
};
export type ChallengesPageLiftMutation$variables = {
  input: LiftContainmentInput;
};
export type ChallengesPageLiftMutation$data = {
  readonly liftContainment: {
    readonly containment: {
      readonly id: string;
      readonly liftedAt: string | null | undefined;
    };
  };
};
export type ChallengesPageLiftMutation = {
  response: ChallengesPageLiftMutation$data;
  variables: ChallengesPageLiftMutation$variables;
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
    "name": "liftContainment",
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
          },
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "liftedAt",
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
    "name": "ChallengesPageLiftMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "ChallengesPageLiftMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "5b3c54089a411fef1e0b598b8a6235d9",
    "id": null,
    "metadata": {},
    "name": "ChallengesPageLiftMutation",
    "operationKind": "mutation",
    "text": "mutation ChallengesPageLiftMutation(\n  $input: LiftContainmentInput!\n) {\n  liftContainment(input: $input) {\n    containment {\n      id\n      liftedAt\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "c2cb814f96946e6ba1a6fd2fedd785e5";

export default node;
