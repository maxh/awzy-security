/**
 * @generated SignedSource<<adea6236a2abedd9cad7bf602d1076b5>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from 'relay-runtime';
export type EnforcementMode = "ENFORCE" | "MONITOR" | "%future added value";
export type Verdict = "ALLOW" | "CHALLENGE" | "DENY" | "%future added value";
import { FragmentRefs } from "relay-runtime";
export type DecisionDetail_decision$data = {
  readonly action: string;
  readonly at: string;
  readonly challenges: ReadonlyArray<string>;
  readonly context: string;
  readonly determiningPolicies: ReadonlyArray<{
    readonly id: string;
    readonly source: string;
  }>;
  readonly humanPresent: boolean;
  readonly id: string;
  readonly mode: EnforcementMode;
  readonly onBehalfOf: {
    readonly determiningPolicies: ReadonlyArray<{
      readonly id: string;
      readonly source: string;
    }>;
    readonly human: {
      readonly id: string;
      readonly type: string;
    };
    readonly verdict: Verdict;
  } | null | undefined;
  readonly policyVersion: number;
  readonly principal: {
    readonly id: string;
    readonly type: string;
  };
  readonly resource: {
    readonly id: string;
    readonly type: string;
  };
  readonly verdict: Verdict;
  readonly " $fragmentType": "DecisionDetail_decision";
};
export type DecisionDetail_decision$key = {
  readonly " $data"?: DecisionDetail_decision$data;
  readonly " $fragmentSpreads": FragmentRefs<"DecisionDetail_decision">;
};

const node: ReaderFragment = (function(){
var v0 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
},
v1 = [
  {
    "alias": null,
    "args": null,
    "kind": "ScalarField",
    "name": "type",
    "storageKey": null
  },
  (v0/*:: as any*/)
],
v2 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "verdict",
  "storageKey": null
},
v3 = {
  "alias": null,
  "args": null,
  "concreteType": "PolicySource",
  "kind": "LinkedField",
  "name": "determiningPolicies",
  "plural": true,
  "selections": [
    (v0/*:: as any*/),
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "source",
      "storageKey": null
    }
  ],
  "storageKey": null
};
return {
  "argumentDefinitions": [],
  "kind": "Fragment",
  "metadata": null,
  "name": "DecisionDetail_decision",
  "selections": [
    (v0/*:: as any*/),
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "at",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "concreteType": "EntityRef",
      "kind": "LinkedField",
      "name": "principal",
      "plural": false,
      "selections": (v1/*:: as any*/),
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "concreteType": "OnBehalfOf",
      "kind": "LinkedField",
      "name": "onBehalfOf",
      "plural": false,
      "selections": [
        {
          "alias": null,
          "args": null,
          "concreteType": "EntityRef",
          "kind": "LinkedField",
          "name": "human",
          "plural": false,
          "selections": (v1/*:: as any*/),
          "storageKey": null
        },
        (v2/*:: as any*/),
        (v3/*:: as any*/)
      ],
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "humanPresent",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "action",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "concreteType": "EntityRef",
      "kind": "LinkedField",
      "name": "resource",
      "plural": false,
      "selections": (v1/*:: as any*/),
      "storageKey": null
    },
    (v2/*:: as any*/),
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "mode",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "challenges",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "policyVersion",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "context",
      "storageKey": null
    },
    (v3/*:: as any*/)
  ],
  "type": "Decision",
  "abstractKey": null
};
})();

(node as any).hash = "4f2f8df02cca6360f668192db772794d";

export default node;
