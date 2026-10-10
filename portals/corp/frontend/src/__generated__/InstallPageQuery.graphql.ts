/**
 * @generated SignedSource<<736b7cb5e2f1e4b246a273722f307801>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

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
v3 = {
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
      "name": "createdAt",
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
          (v3/*:: as any*/)
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
          (v3/*:: as any*/),
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
    "cacheID": "6d56f4b436cdb3de1493813e0d697580",
    "id": null,
    "metadata": {},
    "name": "InstallPageQuery",
    "operationKind": "query",
    "text": "query InstallPageQuery(\n  $id: ID!\n) {\n  node(id: $id) {\n    __typename\n    ... on Install {\n      client\n      environment\n      cloud\n      region\n      channel\n      pinnedVersion\n      health\n      chartVersion\n      lastSeenAt\n      createdBy {\n        label\n      }\n      createdAt\n      services {\n        name\n        version\n        imageDigest\n        replicasReady\n        replicasDesired\n        restarts24h\n        healthy\n      }\n      metrics {\n        decisionsPerSecond\n        pdpP99Ms\n        errorRate\n        kafkaLag\n        clickhouseDiskUsedPercent\n      }\n      versionHistory {\n        chartVersion\n        firstSeenAt\n      }\n    }\n    id\n  }\n}\n"
  }
};
})();

(node as any).hash = "d28d390cad2c3cc0fab6c48da80a9a5b";

export default node;
