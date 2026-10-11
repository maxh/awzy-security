/**
 * @generated SignedSource<<036d5e8eca145f4cd4e78838ec598363>>
 * @relayHash 77193d1036930d84b27f9ef9f893411f
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 2c4d5141282284cb7703d1051df1d05f1b07ecd4aa6960c802f093b644f08aae

import { ConcreteRequest } from 'relay-runtime';
export type RetireInstallInput = {
  installQid: string;
};
export type InstallPageRetireMutation$variables = {
  input: RetireInstallInput;
};
export type InstallPageRetireMutation$data = {
  readonly retireInstall: {
    readonly install: {
      readonly id: string;
      readonly retiredAt: string | null | undefined;
    };
  };
};
export type InstallPageRetireMutation = {
  response: InstallPageRetireMutation$data;
  variables: InstallPageRetireMutation$variables;
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
    "concreteType": "RetireInstallPayload",
    "kind": "LinkedField",
    "name": "retireInstall",
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
          },
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "retiredAt",
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
    "name": "InstallPageRetireMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "InstallPageRetireMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "id": "2c4d5141282284cb7703d1051df1d05f1b07ecd4aa6960c802f093b644f08aae",
    "metadata": {},
    "name": "InstallPageRetireMutation",
    "operationKind": "mutation",
    "text": null
  }
};
})();

(node as any).hash = "1fdd46408fc7c77d21382e989a30e360";

export default node;
