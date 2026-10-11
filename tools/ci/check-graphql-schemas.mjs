#!/usr/bin/env node
// Holds both portals' GraphQL schemas to the conventions in AGENTS.md (ADR 0013, 0058, 0100):
//
// - An object with `id: ID!` implements Node, so Relay can refetch it with `node(id)`, and a Node
//   has `id: ID!`.
// - Any other field, argument, or input field typed ID holds one of awzy's QIDs, so it's named
//   `qid` or `<entity>Qid`; IDs from the client's systems are Strings. `node(id)` is Relay's.
// - A `…Qid` is typed ID.
// - A field ending in `At` is a UtcTimestamp, and there's no DateTime scalar.
//
// Run by `pnpm lint`, so CI's lint-ts job checks the committed schemas.
import { readFileSync } from "node:fs";

import { buildSchema, getNamedType, isInputObjectType, isInterfaceType, isObjectType, isScalarType } from "graphql";

const SCHEMAS = ["portals/client/frontend/schema.graphql", "portals/corp/frontend/schema.graphql"];

const isId = (type) => getNamedType(type).name === "ID";
const qidNamed = (name) => name === "qid" || name.endsWith("Qid");

/** The problems in one schema's SDL, as `Type.field: what's wrong`. */
export function check(sdl) {
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
        if (isObjectType(type) && !isNode && !roots.has(type))
          problems.push(`${at}: an object with an ID implements Node`);
        if (String(field.type) !== "ID!") problems.push(`${at}: a Node's id is ID!`);
      } else if (isId(field.type) && !qidNamed(field.name)) {
        problems.push(`${at}: an ID field holds a QID, so it's named qid or …Qid`);
      }
      if (qidNamed(field.name) && !isId(field.type)) problems.push(`${at}: a QID is typed ID`);
      if (/[a-z]At$/.test(field.name) && getNamedType(field.type).name !== "UtcTimestamp") {
        problems.push(`${at}: a time is a UtcTimestamp`);
      }
      for (const arg of field.args ?? []) {
        const relayNode = type === schema.getQueryType() && field.name === "node" && arg.name === "id";
        if (isId(arg.type) && !qidNamed(arg.name) && !relayNode) {
          problems.push(`${at}(${arg.name}): an ID argument holds a QID, so it's named qid or …Qid`);
        }
        if (qidNamed(arg.name) && !isId(arg.type)) problems.push(`${at}(${arg.name}): a QID is typed ID`);
      }
    }
    if (isNode && !type.getFields().id) problems.push(`${type.name}: a Node has id: ID!`);
  }
  return problems;
}

if (import.meta.url === `file://${process.argv[1]}`) {
  let failed = false;
  for (const path of SCHEMAS) {
    for (const p of check(readFileSync(path, "utf8"))) {
      console.error(`${path}: ${p}`);
      failed = true;
    }
  }
  if (failed) {
    console.error("check-graphql-schemas: fix the Rust that generates these, then regenerate the schemas.");
    process.exit(1);
  }
}
