/**
 * @generated SignedSource<<b75afb52348f3ceccf36118d27e21d89>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type AskAwzyBotInput = {
  message: string;
  transcript?: string | null | undefined;
};
export type AwzyBotPanelAskMutation$variables = {
  input: AskAwzyBotInput;
};
export type AwzyBotPanelAskMutation$data = {
  readonly askAwzyBot: {
    readonly drafts: ReadonlyArray<{
      readonly id: string;
      readonly version: number;
    }>;
    readonly refused: boolean;
    readonly reply: string;
    readonly toolsUsed: ReadonlyArray<string>;
    readonly transcript: string;
  };
};
export type AwzyBotPanelAskMutation = {
  response: AwzyBotPanelAskMutation$data;
  variables: AwzyBotPanelAskMutation$variables;
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
    "concreteType": "AskAwzyBotPayload",
    "kind": "LinkedField",
    "name": "askAwzyBot",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "reply",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "transcript",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "toolsUsed",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "refused",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "concreteType": "PolicyBundle",
        "kind": "LinkedField",
        "name": "drafts",
        "plural": true,
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
            "name": "version",
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
    "name": "AwzyBotPanelAskMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "AwzyBotPanelAskMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "ea6fad51560636d3a7375baa4cbc9b63",
    "id": null,
    "metadata": {},
    "name": "AwzyBotPanelAskMutation",
    "operationKind": "mutation",
    "text": "mutation AwzyBotPanelAskMutation(\n  $input: AskAwzyBotInput!\n) {\n  askAwzyBot(input: $input) {\n    reply\n    transcript\n    toolsUsed\n    refused\n    drafts {\n      id\n      version\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "b456f6a6b72f698fc2f96689116e41d6";

export default node;
