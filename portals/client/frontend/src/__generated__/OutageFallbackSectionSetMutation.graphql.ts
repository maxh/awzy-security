/**
 * @generated SignedSource<<4b6d7907facf467bb0be1642e6d42451>>
 * @relayHash 5d245695cc94e7d8e4bed69f67240211
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 74902ee9a6ce170bec4ce11938bef53bf14b65c30de1bc60bca724ca830b3f88

import { ConcreteRequest } from 'relay-runtime';
export type SensitivityTier = "CONFIDENTIAL" | "CROWN_JEWELS" | "INTERNAL" | "PUBLIC" | "%future added value";
export type SetOutageFallbackInput = {
  failOpenTiers: ReadonlyArray<SensitivityTier>;
  servers: ReadonlyArray<ServerTierInput>;
  tools: ReadonlyArray<ToolTierInput>;
};
export type ToolTierInput = {
  tier: SensitivityTier;
  tool: string;
};
export type ServerTierInput = {
  server: string;
  tier: SensitivityTier;
};
export type OutageFallbackSectionSetMutation$variables = {
  input: SetOutageFallbackInput;
};
export type OutageFallbackSectionSetMutation$data = {
  readonly setOutageFallback: {
    readonly challengeQid: string | null | undefined;
    readonly outageFallback: {
      readonly version: number;
    } | null | undefined;
  };
};
export type OutageFallbackSectionSetMutation = {
  response: OutageFallbackSectionSetMutation$data;
  variables: OutageFallbackSectionSetMutation$variables;
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
    "concreteType": "SetOutageFallbackPayload",
    "kind": "LinkedField",
    "name": "setOutageFallback",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "OutageFallback",
        "kind": "LinkedField",
        "name": "outageFallback",
        "plural": false,
        "selections": [
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "version",
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
    "name": "OutageFallbackSectionSetMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "OutageFallbackSectionSetMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "id": "74902ee9a6ce170bec4ce11938bef53bf14b65c30de1bc60bca724ca830b3f88",
    "metadata": {},
    "name": "OutageFallbackSectionSetMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "392925c00533d175dfe4944f050d8c47";

export default node;
