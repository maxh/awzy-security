/**
 * @generated SignedSource<<f9b8376844c817655b683e15f22a928a>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type ReleaseChannel = "FAST" | "PINNED" | "STABLE" | "%future added value";
export type RegisterInstallInput = {
  channel: ReleaseChannel;
  client: string;
  cloud: string;
  environment: string;
  pinnedVersion?: string | null | undefined;
  region: string;
};
export type FleetPageRegisterMutation$variables = {
  input: RegisterInstallInput;
};
export type FleetPageRegisterMutation$data = {
  readonly registerInstall: {
    readonly install: {
      readonly id: string;
    };
    readonly pullToken: string | null | undefined;
    readonly reportingToken: string;
  };
};
export type FleetPageRegisterMutation = {
  response: FleetPageRegisterMutation$data;
  variables: FleetPageRegisterMutation$variables;
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
    "concreteType": "RegisterInstallPayload",
    "kind": "LinkedField",
    "name": "registerInstall",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "Install",
        "kind": "LinkedField",
        "name": "install",
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
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "reportingToken",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "pullToken",
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
    "name": "FleetPageRegisterMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "FleetPageRegisterMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "ad86849b3c74bbaa75f41eba18428ec7",
    "id": null,
    "metadata": {},
    "name": "FleetPageRegisterMutation",
    "operationKind": "mutation",
    "text": "mutation FleetPageRegisterMutation(\n  $input: RegisterInstallInput!\n) {\n  registerInstall(input: $input) {\n    install {\n      id\n    }\n    reportingToken\n    pullToken\n  }\n}\n"
  }
};
})();

(node as any).hash = "dd61f5909af30690aa0946e2d63ea0ea";

export default node;
