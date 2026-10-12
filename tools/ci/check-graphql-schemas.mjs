#!/usr/bin/env node
// Holds both portals' GraphQL schemas to the conventions in AGENTS.md (ADR 0013, 0058, 0100):
//
// - An object with `id: ID!` implements Node, so Relay can refetch it with `node(id)`, and a Node
//   has `id: ID!`. `ID` is for nothing else, except `node(id)`'s argument, which Relay names.
// - Any other field, argument, or input field holding one of awzy's QIDs is a `Qid`, named `qid`
//   or `<entity>Qid`; the backend types each with its entity (`Qid<ChallengeId>`). IDs from the
//   client's systems are Strings.
// - A field ending in `At` is a UtcTimestamp, and there's no DateTime scalar.
// - A list of objects is a Relay connection (ADR 0012), so a page can start short and load more.
//   Mutation payloads and subscriptions are exempt, and so is a list that's part of its parent
//   and can't grow on its own, named in BOUNDED with why.
//
// Run by `pnpm lint`, so CI's lint-ts job checks the committed schemas.
import { readFileSync } from "node:fs";

import {
  buildSchema,
  getNamedType,
  getNullableType,
  isInputObjectType,
  isInterfaceType,
  isListType,
  isObjectType,
  isScalarType,
} from "graphql";

const SCHEMAS = ["portals/client/frontend/schema.graphql", "portals/corp/frontend/schema.graphql"];

/** Lists of objects that stay plain lists: each is part of its parent and bounded by it. */
const BOUNDED = new Map([
  // Corp portal
  ["Install.services", "the install's few deployed services"],
  ["Install.metricHistory", "a chart's samples for one window, read whole"],
  ["Query.cloudRegions", "the clouds awzy installs in"],
  ["Query.channels", "the release channels, at most three"],
  // Client portal
  ["Connector.settings", "one connector's settings"],
  ["Decision.determiningPolicies", "the policies behind one decision"],
  ["OnBehalfOf.determiningPolicies", "the policies behind one decision"],
  ["Investigation.evidence", "one finding's evidence"],
  ["ModelRouting.connections", "the model connections the install can reach"],
  ["OutageFallback.tools", "one tier setting per tool"],
  ["OutageFallback.servers", "one tier setting per server"],
  ["PolicyDryRun.changes", "a dry run's verdict changes, capped by the run"],
  ["PolicyDryRunChange.examples", "a few examples of one change"],
  ["Query.enforcementScopes", "the mode in force for each action and tool"],
  ["Query.installStatus", "the install's components"],
  ["Query.connectors", "the connectors awzy ships"],
  ["Query.connectorConfigs", "one configuration per connector"],
  ["Query.repairKinds", "the repairs awzy ships"],
  ["Query.mySecurityKeys", "one person's few security keys"],
  ["Query.securityKeys", "one person's few security keys"],
]);

/** The client portal's record lists, until they're connections too. Remove each as it's converted. */
const CLIENT_NOT_YET = new Set([
  "Query.policyBundles",
  "Query.enforcementHistory",
  "Query.humans",
  "Query.groups",
  "Query.serviceAccounts",
  "Query.tokenIssuers",
  "Query.myChallenges",
  "Query.challengesToApprove",
  "Query.challenges",
  "Query.containments",
  "Query.investigations",
  "Query.supportSessions",
  "Query.searchResources",
  "Query.repairRuns",
]);

const isId = (type) => getNamedType(type).name === "ID";
const isQid = (type) => getNamedType(type).name === "Qid";
const qidNamed = (name) => name === "qid" || name.endsWith("Qid");

/** The problems in one schema's SDL, as `Type.field: what's wrong`. */
export function check(sdl, notYet = new Set()) {
  const schema = buildSchema(sdl);
  const problems = [];
  const roots = new Set([schema.getQueryType(), schema.getMutationType(), schema.getSubscriptionType()]);

  for (const type of Object.values(schema.getTypeMap())) {
    if (type.name.startsWith("__")) continue;
    if (isScalarType(type) && type.name === "DateTime") problems.push("DateTime: use UtcTimestamp");
    if (!isObjectType(type) && !isInterfaceType(type) && !isInputObjectType(type)) continue;
    const isNode = isObjectType(type) && type.getInterfaces().some((i) => i.name === "Node");

    for (const field of Object.values(type.getFields())) {
      const at = `${type.name}.${field.name}`;
      const ownId = field.name === "id" && !isInputObjectType(type);
      if (ownId && isId(field.type)) {
        if (isObjectType(type) && !isNode && !roots.has(type)) {
          problems.push(`${at}: an object with an ID implements Node`);
        }
        if (String(field.type) !== "ID!") problems.push(`${at}: a Node's id is ID!`);
      } else if (isId(field.type)) {
        problems.push(`${at}: ID is only a Node's id; a QID is a Qid`);
      }
      if (qidNamed(field.name) && !isQid(field.type)) problems.push(`${at}: a QID is a Qid`);
      if (isQid(field.type) && !qidNamed(field.name)) problems.push(`${at}: a Qid is named qid or …Qid`);
      if (/[a-z]At$/.test(field.name) && getNamedType(field.type).name !== "UtcTimestamp") {
        problems.push(`${at}: a time is a UtcTimestamp`);
      }
      const listOfObjects = isListType(getNullableType(field.type)) && isObjectType(getNamedType(field.type));
      const exempt =
        type === schema.getSubscriptionType() ||
        type.name.endsWith("Payload") ||
        type.name.endsWith("Connection") ||
        BOUNDED.has(at) ||
        notYet.has(at);
      if (listOfObjects && isObjectType(type) && !exempt) {
        problems.push(`${at}: a list of objects is a connection (or BOUNDED, with why)`);
      }
      for (const arg of field.args ?? []) {
        const relayNode = type === schema.getQueryType() && field.name === "node" && arg.name === "id";
        if (isId(arg.type) && !relayNode) problems.push(`${at}(${arg.name}): an ID argument is a Qid`);
        if (qidNamed(arg.name) && !isQid(arg.type)) problems.push(`${at}(${arg.name}): a QID is a Qid`);
        if (isQid(arg.type) && !qidNamed(arg.name)) problems.push(`${at}(${arg.name}): a Qid is named qid or …Qid`);
      }
    }
    if (isNode && !type.getFields().id) problems.push(`${type.name}: a Node has id: ID!`);
  }
  return problems;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  let failed = false;
  for (const path of SCHEMAS) {
    const notYet = path.includes("/client/") ? CLIENT_NOT_YET : new Set();
    for (const p of check(readFileSync(path, "utf8"), notYet)) {
      console.error(`${path}: ${p}`);
      failed = true;
    }
  }
  if (failed) {
    console.error("check-graphql-schemas: fix the Rust that generates these, then regenerate the schemas.");
    process.exit(1);
  }
}
