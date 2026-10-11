/**
 * @generated SignedSource<<3ecd53577384d8dc5916fd4443f75fb8>>
 * @relayHash 14d359a1ba6287bb0627b01b70216fe4
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 8f048a9a8908d94a3d89918bc831be93784d2c8ee7daf45de0aca96507bdfe60

import { ConcreteRequest } from 'relay-runtime';
export type RemoveSecurityKeyInput = {
  authorization: AssertionInput;
  securityKeyQid: string;
};
export type AssertionInput = {
  authenticatorData: string;
  ceremonyQid: string;
  clientDataJson: string;
  credentialId: string;
  signature: string;
};
export type ChallengesPageRemoveKeyMutation$variables = {
  input: RemoveSecurityKeyInput;
};
export type ChallengesPageRemoveKeyMutation$data = {
  readonly removeSecurityKey: {
    readonly securityKey: {
      readonly id: string;
    };
  };
};
export type ChallengesPageRemoveKeyMutation = {
  response: ChallengesPageRemoveKeyMutation$data;
  variables: ChallengesPageRemoveKeyMutation$variables;
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
    "concreteType": "SecurityKeyPayload",
    "kind": "LinkedField",
    "name": "removeSecurityKey",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "SecurityKey",
        "kind": "LinkedField",
        "name": "securityKey",
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
    "name": "ChallengesPageRemoveKeyMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "ChallengesPageRemoveKeyMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "id": "8f048a9a8908d94a3d89918bc831be93784d2c8ee7daf45de0aca96507bdfe60",
    "metadata": {},
    "name": "ChallengesPageRemoveKeyMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "59a3cca9d0505c330d59801a2cd6bcaa";

export default node;
