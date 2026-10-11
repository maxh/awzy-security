/**
 * @generated SignedSource<<c028ee41abc64dbfdb3da9019851387c>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type BeginChallengeVerificationInput = {
  challengeQid: string;
};
export type EnforcementPageBeginVerificationMutation$variables = {
  input: BeginChallengeVerificationInput;
};
export type EnforcementPageBeginVerificationMutation$data = {
  readonly beginChallengeVerification: {
    readonly ceremonyQid: string;
    readonly optionsJson: string;
  };
};
export type EnforcementPageBeginVerificationMutation = {
  response: EnforcementPageBeginVerificationMutation$data;
  variables: EnforcementPageBeginVerificationMutation$variables;
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
    "concreteType": "WebAuthnCeremony",
    "kind": "LinkedField",
    "name": "beginChallengeVerification",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "ceremonyQid",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "optionsJson",
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
    "name": "EnforcementPageBeginVerificationMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "EnforcementPageBeginVerificationMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "a0374ce306476892f8bd5da902318141",
    "id": null,
    "metadata": {},
    "name": "EnforcementPageBeginVerificationMutation",
    "operationKind": "mutation",
    "text": "mutation EnforcementPageBeginVerificationMutation(\n  $input: BeginChallengeVerificationInput!\n) {\n  beginChallengeVerification(input: $input) {\n    ceremonyQid\n    optionsJson\n  }\n}\n"
  }
};
})();

(node as any).hash = "eb5e67bb03dd8ad33e72c11d8c782cb1";

export default node;
