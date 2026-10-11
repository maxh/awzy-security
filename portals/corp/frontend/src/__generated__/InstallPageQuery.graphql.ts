/**
 * @generated SignedSource<<68d77cea4b92d2c5687a22cfea188d67>>
 * @relayHash 5ee87055a013971c0b9706bad26264e8
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 0a35645f54555793d91788a6c5c0e9e39aa98c2874f51e13662df462e238064c

import { ConcreteRequest } from 'relay-runtime';
export type InstallHealth = "DEGRADED" | "NEVER_REPORTED" | "NOT_REPORTING" | "OK" | "%future added value";
export type ReleaseChannel = "FAST" | "PINNED" | "STABLE" | "%future added value";
export type InstallPageQuery$variables = {
  id: string;
};
export type InstallPageQuery$data = {
  readonly node: {
    readonly channel?: ReleaseChannel;
    readonly chartVersion?: string | null | undefined;
    readonly client?: string;
    readonly cloud?: string;
    readonly createdAt?: string;
    readonly createdBy?: {
      readonly label: string;
    };
    readonly environment?: string;
    readonly health?: InstallHealth;
    readonly lastSeenAt?: string | null | undefined;
    readonly metrics?: {
      readonly clickhouseDiskUsedPercent: number | null | undefined;
      readonly decisionsPerSecond: number | null | undefined;
      readonly errorRate: number | null | undefined;
      readonly kafkaLag: number | null | undefined;
      readonly pdpP99Ms: number | null | undefined;
    } | null | undefined;
    readonly pinnedVersion?: string | null | undefined;
    readonly region?: string;
    readonly retiredAt?: string | null | undefined;
    readonly retiredBy?: {
      readonly label: string;
    } | null | undefined;
    readonly services?: ReadonlyArray<{
      readonly healthy: boolean;
      readonly imageDigest: string | null | undefined;
      readonly name: string;
      readonly replicasDesired: number;
      readonly replicasReady: number;
      readonly restarts24h: number | null | undefined;
      readonly version: string | null | undefined;
    }>;
    readonly versionHistory?: ReadonlyArray<{
      readonly chartVersion: string;
      readonly firstSeenAt: string;
    }>;
  } | null | undefined;
};
export type InstallPageQuery = {
  response: InstallPageQuery$data;
  variables: InstallPageQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "id"
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
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "chartVersion",
  "storageKey": null
},
v3 = [
  {
    "alias": null,
    "args": null,
    "kind": "ScalarField",
    "name": "label",
    "storageKey": null
  }
],
v4 = {
  "kind": "InlineFragment",
  "selections": [
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "client",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "environment",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "cloud",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "region",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "channel",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "pinnedVersion",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "health",
      "storageKey": null
    },
    (v2/*:: as any*/),
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "lastSeenAt",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "concreteType": "Actor",
      "kind": "LinkedField",
      "name": "createdBy",
      "plural": false,
      "selections": (v3/*:: as any*/),
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "createdAt",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "retiredAt",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "concreteType": "Actor",
      "kind": "LinkedField",
      "name": "retiredBy",
      "plural": false,
      "selections": (v3/*:: as any*/),
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "concreteType": "DeployedService",
      "kind": "LinkedField",
      "name": "services",
      "plural": true,
      "selections": [
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "name",
          "storageKey": null
        },
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
          "kind": "ScalarField",
          "name": "imageDigest",
          "storageKey": null
        },
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "replicasReady",
          "storageKey": null
        },
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "replicasDesired",
          "storageKey": null
        },
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "restarts24h",
          "storageKey": null
        },
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "healthy",
          "storageKey": null
        }
      ],
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "concreteType": "InstallMetrics",
      "kind": "LinkedField",
      "name": "metrics",
      "plural": false,
      "selections": [
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
        }
      ],
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "concreteType": "VersionSeen",
      "kind": "LinkedField",
      "name": "versionHistory",
      "plural": true,
      "selections": [
        (v2/*:: as any*/),
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "firstSeenAt",
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
    "name": "InstallPageQuery",
    "selections": [
      {
        "alias": null,
        "args": (v1/*:: as any*/),
        "concreteType": null,
        "kind": "LinkedField",
        "name": "node",
        "plural": false,
        "selections": [
          (v4/*:: as any*/)
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
    "name": "InstallPageQuery",
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
          (v4/*:: as any*/),
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
    "id": "0a35645f54555793d91788a6c5c0e9e39aa98c2874f51e13662df462e238064c",
    "metadata": {},
    "name": "InstallPageQuery",
    "operationKind": "query",
    "text": null
  }
};
})();

(node as any).hash = "b743bc21e25028c7f78c75710dd4aaff";

export default node;
