/**
 * @generated SignedSource<<3fdba272d92e15caca059d2b71d9e7f1>>
 * @relayHash ea6fad51560636d3a7375baa4cbc9b63
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 5820aca120b7aa0228a0efa911d505b50bbce114ec09d17f6cc04a564f97c58a

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
    "id": "5820aca120b7aa0228a0efa911d505b50bbce114ec09d17f6cc04a564f97c58a",
    "metadata": {},
    "name": "AwzyBotPanelAskMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "b456f6a6b72f698fc2f96689116e41d6";

export default node;
