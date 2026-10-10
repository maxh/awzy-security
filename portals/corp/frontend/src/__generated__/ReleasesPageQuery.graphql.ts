/**
 * @generated SignedSource<<94ba2e6fd3eee9f7fc6e6485ada15c04>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ConcreteRequest } from 'relay-runtime';
export type InstallHealth = "DEGRADED" | "NEVER_REPORTED" | "NOT_REPORTING" | "OK" | "%future added value";
export type ReleaseChannel = "FAST" | "PINNED" | "STABLE" | "%future added value";
export type ReleasesPageQuery$variables = Record<PropertyKey, never>;
export type ReleasesPageQuery$data = {
  readonly releases: ReadonlyArray<{
    readonly installCount: number;
    readonly installs: ReadonlyArray<{
      readonly channel: ReleaseChannel;
      readonly client: string;
      readonly environment: string;
      readonly health: InstallHealth;
      readonly id: string;
      readonly pinnedVersion: string | null | undefined;
    }>;
    readonly version: string;
  }>;
};
export type ReleasesPageQuery = {
  response: ReleasesPageQuery$data;
  variables: ReleasesPageQuery$variables;
};

const node: ConcreteRequest = (function(){
var v0 = [
  {
    "alias": null,
    "args": null,
    "concreteType": "Release",
    "kind": "LinkedField",
    "name": "releases",
    "plural": true,
    "selections": [
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
        "name": "installCount",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "concreteType": "Install",
        "kind": "LinkedField",
        "name": "installs",
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
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "ReleasesPageQuery",
    "selections": (v0/*:: as any*/),
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "ReleasesPageQuery",
    "selections": (v0/*:: as any*/)
  },
  "params": {
    "cacheID": "2b1c524e41ac3ce4a7daa3aee28dc9ed",
    "id": null,
    "metadata": {},
    "name": "ReleasesPageQuery",
    "operationKind": "query",
    "text": "query ReleasesPageQuery {\n  releases {\n    version\n    installCount\n    installs {\n      id\n      client\n      environment\n      channel\n      pinnedVersion\n      health\n    }\n  }\n}\n"
  }
};
})();

(node as any).hash = "0d2230d5c32a6bb9ebbe7eb8d0b0dcfc";

export default node;
