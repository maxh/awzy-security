/**
 * @generated SignedSource<<821e7ee71a617e19387962c1de0d4b4c>>
 * @relayHash cef546f11bde82bb73cd4dc41345ddb2
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID df19714981120e9beec1c0e2b56378b7f4e45dff97d02f4af8fda2037f7bf0b9

import { ConcreteRequest } from 'relay-runtime';
export type SensitivityTier = "CONFIDENTIAL" | "CROWN_JEWELS" | "INTERNAL" | "PUBLIC" | "%future added value";
export type OutageFallbackSectionQuery$variables = Record<PropertyKey, never>;
export type OutageFallbackSectionQuery$data = {
  readonly outageFallback: {
    readonly changedAt: string | null | undefined;
    readonly changedBy: {
      readonly label: string;
    } | null | undefined;
    readonly failOpenTiers: ReadonlyArray<SensitivityTier>;
    readonly servers: ReadonlyArray<{
      readonly server: string;
      readonly tier: SensitivityTier;
    }>;
    readonly tools: ReadonlyArray<{
      readonly tier: SensitivityTier;
      readonly tool: string;
    }>;
    readonly version: number;
  };
};
export type OutageFallbackSectionQuery = {
  response: OutageFallbackSectionQuery$data;
  variables: OutageFallbackSectionQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "tier",
  "storageKey": null
},
v1 = [
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
      },
      {
        "alias": null,
        "args": null,
        "concreteType": "ToolTier",
        "kind": "LinkedField",
        "name": "tools",
        "plural": true,
        "selections": [
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "tool",
            "storageKey": null
          },
          (v0/*:: as any*/)
        ],
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "concreteType": "ServerTier",
        "kind": "LinkedField",
        "name": "servers",
        "plural": true,
        "selections": [
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "server",
            "storageKey": null
          },
          (v0/*:: as any*/)
        ],
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "failOpenTiers",
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
  }
];
return {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "OutageFallbackSectionQuery",
    "selections": (v1/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "OutageFallbackSectionQuery",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "id": "df19714981120e9beec1c0e2b56378b7f4e45dff97d02f4af8fda2037f7bf0b9",
    "metadata": {},
    "name": "OutageFallbackSectionQuery",
    "operationKind": "query",
    "text": null
  }
};
})();

(node as any).hash = "d5d74791b52224af319551095e2dcd29";

export default node;
