/**
 * @generated SignedSource<<a77ccb3558f37061c7e2e6304f382cfd>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type MetricWindow = "DAY" | "MONTH" | "WEEK" | "%future added value";
export type InstallChartsQuery$variables = {
  id: string;
  window: MetricWindow;
};
export type InstallChartsQuery$data = {
  readonly node: {
    readonly metricHistory?: ReadonlyArray<{
      readonly at: string;
      readonly clickhouseDiskUsedPercent: number | null | undefined;
      readonly decisionsPerSecond: number | null | undefined;
      readonly degraded: boolean;
      readonly errorRate: number | null | undefined;
      readonly kafkaLag: number | null | undefined;
      readonly pdpP99Ms: number | null | undefined;
    }>;
  } | null | undefined;
};
export type InstallChartsQuery = {
  response: InstallChartsQuery$data;
  variables: InstallChartsQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "id"
  },
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "window"
  }
],
v1 = [
  {
    "kind": "Variable",
    "name": "id",
    "variableName": "id"
  }
],
v2 = {
  "kind": "InlineFragment",
  "selections": [
    {
      "alias": null,
      "args": [
        {
          "kind": "Variable",
          "name": "window",
          "variableName": "window"
        }
      ],
      "concreteType": "MetricSample",
      "kind": "LinkedField",
      "name": "metricHistory",
      "plural": true,
      "selections": [
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
          "kind": "ScalarField",
          "name": "decisionsPerSecond",
          "storageKey": null
        },
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "pdpP99Ms",
          "storageKey": null
        },
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "errorRate",
          "storageKey": null
        },
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "kafkaLag",
          "storageKey": null
        },
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "clickhouseDiskUsedPercent",
          "storageKey": null
        },
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "degraded",
          "storageKey": null
        }
      ],
      "storageKey": null
    }
  ],
  "type": "Install",
  "abstractKey": null
};
return {
  "fragment": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Fragment",
    "metadata": null,
    "name": "InstallChartsQuery",
    "selections": [
      {
        "alias": null,
        "args": (v1/*:: as any*/),
        "concreteType": null,
        "kind": "LinkedField",
        "name": "node",
        "plural": false,
        "selections": [
          (v2/*:: as any*/)
        ],
        "storageKey": null
      }
    ],
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "InstallChartsQuery",
    "selections": [
      {
        "alias": null,
        "args": (v1/*:: as any*/),
        "concreteType": null,
        "kind": "LinkedField",
        "name": "node",
        "plural": false,
        "selections": [
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "__typename",
            "storageKey": null
          },
          (v2/*:: as any*/),
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
    ]
  },
  "params": {
    "cacheID": "17debe259b5b5317966791b19aa289b5",
    "id": null,
    "metadata": {},
    "name": "InstallChartsQuery",
    "operationKind": "query",
    "text": "query InstallChartsQuery(\n  $id: ID!\n  $window: MetricWindow!\n) {\n  node(id: $id) {\n    __typename\n    ... on Install {\n      metricHistory(window: $window) {\n        at\n        decisionsPerSecond\n        pdpP99Ms\n        errorRate\n        kafkaLag\n        clickhouseDiskUsedPercent\n        degraded\n      }\n    }\n    id\n  }\n}\n"
  }
};
})();

(node as any).hash = "0b44f29266cba336ed5dacd445afd259";

export default node;
