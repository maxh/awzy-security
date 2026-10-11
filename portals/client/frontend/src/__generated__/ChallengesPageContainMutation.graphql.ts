/**
 * @generated SignedSource<<19f88dd64157b157c93112745a3190ad>>
 * @relayHash 6a7e349057284fced35207ad4bcee167
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 22b26285ee1b9e4337c952e97e71cb09fe7b98551af4750b835a5cfba129db79

import { ConcreteRequest } from 'relay-runtime';
export type ContainPrincipalInput = {
  principal: EntityRefInput;
  reason: string;
};
export type EntityRefInput = {
  id: string;
  type: string;
};
export type ChallengesPageContainMutation$variables = {
  input: ContainPrincipalInput;
};
export type ChallengesPageContainMutation$data = {
  readonly containPrincipal: {
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
    "name": "containPrincipal",
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
    "id": "22b26285ee1b9e4337c952e97e71cb09fe7b98551af4750b835a5cfba129db79",
    "metadata": {},
    "name": "ChallengesPageContainMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "ea3644a9c475df369e2531b6c6467af2";

export default node;
