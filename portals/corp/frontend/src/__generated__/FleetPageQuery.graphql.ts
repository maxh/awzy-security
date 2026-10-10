/**
 * @generated SignedSource<<6d078a4f408732c8071b5969e154cfc6>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type InstallHealth = "DEGRADED" | "NEVER_REPORTED" | "NOT_REPORTING" | "OK" | "%future added value";
export type ReleaseChannel = "FAST" | "PINNED" | "STABLE" | "%future added value";
export type FleetPageQuery$variables = Record<PropertyKey, never>;
export type FleetPageQuery$data = {
  readonly installs: {
    readonly edges: ReadonlyArray<{
      readonly node: {
        readonly channel: ReleaseChannel;
        readonly chartVersion: string | null | undefined;
        readonly client: string;
        readonly cloud: string;
        readonly environment: string;
        readonly health: InstallHealth;
        readonly id: string;
        readonly lastSeenAt: string | null | undefined;
        readonly metrics: {
          readonly decisionsPerSecond: number | null | undefined;
          readonly pdpP99Ms: number | null | undefined;
        } | null | undefined;
        readonly pinnedVersion: string | null | undefined;
        readonly region: string;
        readonly services: ReadonlyArray<{
          readonly healthy: boolean;
        }>;
      };
    }>;
  };
};
export type FleetPageQuery = {
  response: FleetPageQuery$data;
  variables: FleetPageQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Literal",
        "name": "first",
        "value": 200
      }
    ],
    "concreteType": "InstallConnection",
    "kind": "LinkedField",
    "name": "installs",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "InstallEdge",
        "kind": "LinkedField",
        "name": "edges",
        "plural": true,
        "selections": [
          {
            "alias": null,
            "args": null,
            "concreteType": "Install",
            "kind": "LinkedField",
            "name": "node",
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
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "chartVersion",
                "storageKey": null
              },
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
                "concreteType": "DeployedService",
                "kind": "LinkedField",
                "name": "services",
                "plural": true,
                "selections": [
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
                  }
                ],
                "storageKey": null
              }
            ],
            "storageKey": null
          }
        ],
        "storageKey": null
      }
    ],
    "storageKey": "installs(first:200)"
  }
];
return {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "FleetPageQuery",
    "selections": (v0/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "FleetPageQuery",
    "selections": (v0/*:: as any*/)
  },
  "params": {
    "cacheID": "793d50cf055b4ca543f6663757041066",
    "id": null,
    "metadata": {},
    "name": "FleetPageQuery",
    "operationKind": "query",
    "text": "query FleetPageQuery {\n  installs(first: 200) {\n    edges {\n      node {\n        id\n        client\n        environment\n        cloud\n        region\n        channel\n        pinnedVersion\n        health\n        chartVersion\n        lastSeenAt\n        services {\n          healthy\n        }\n        metrics {\n          decisionsPerSecond\n          pdpP99Ms\n        }\n      }\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "aba50291882efdce2958f1c8ae41739c";

export default node;
