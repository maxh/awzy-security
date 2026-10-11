#!/usr/bin/env node
// Every QID field in awzy's protobuf APIs says which entity it holds (ADR 0101): a `string`
// named `qid`, `<name>_qid`, or `<name>_qids` carries `[(awzy.authz.v1.qid) = "<entity>"]`, with
// one of the entities in libs/model/src/qid.rs; a field named for an entity (`challenge_qid`)
// names that one; and only QID fields carry it.
//
// Run by `pnpm lint`, so CI's lint-ts job checks it.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const PROTOS = "authz/libs/proto/awzy";
const OPTION = /\[\s*\(awzy\.authz\.v1\.qid\)\s*=\s*"([a-z_]+)"\s*\]/;

/** The entities awzy mints QIDs for, by prefix, from the macro that defines their types. */
export function entities(qidRs) {
  return new Set([...qidRs.matchAll(/\b[A-Z]\w*Id,[^"]*?"([a-z_]+)"/g)].map((m) => m[1]));
}

/** The problems in one .proto file's text, as `line: what's wrong`. */
export function check(proto, known) {
  const problems = [];
  let inExtend = false; // the option's own declaration isn't a field to check
  proto.split("\n").forEach((line, i) => {
    if (/^\s*extend\s/.test(line)) inExtend = true;
    if (inExtend) {
      if (/^\s*\}/.test(line)) inExtend = false;
      return;
    }
    const field = line.match(/^\s*(repeated\s+)?(\w[\w.]*)\s+(\w+)\s*=\s*\d+/);
    if (!field) return;
    const [, , type, name] = field;
    const isQid = name === "qid" || name.endsWith("_qid") || name.endsWith("_qids");
    const option = line.match(OPTION)?.[1];
    const at = `${i + 1}: ${name}`;
    if (!isQid) {
      if (option) problems.push(`${at}: only a qid or *_qid field is marked with (awzy.authz.v1.qid)`);
      return;
    }
    if (type !== "string") problems.push(`${at}: a QID is a string`);
    if (!option) {
      problems.push(`${at}: mark it with [(awzy.authz.v1.qid) = "<entity>"]`);
      return;
    }
    if (!known.has(option)) problems.push(`${at}: "${option}" isn't an entity in libs/model/src/qid.rs`);
    const named = name.replace(/_qids?$/, "");
    if (known.has(named) && named !== option) problems.push(`${at}: it's named for ${named}, but marked ${option}`);
  });
  return problems;
}

const files = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? files(join(dir, e.name)) : e.name.endsWith(".proto") ? [join(dir, e.name)] : [],
  );

if (import.meta.url === `file://${process.argv[1]}`) {
  const known = entities(readFileSync("libs/model/src/qid.rs", "utf8"));
  let failed = false;
  for (const path of files(PROTOS)) {
    for (const p of check(readFileSync(path, "utf8"), known)) {
      console.error(`${path}:${p}`);
      failed = true;
    }
  }
  if (failed) process.exit(1);
}
