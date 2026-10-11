// Checks a portal's committed Relay artifacts and persisted_queries.json against its queries and
// schema: `relay-compiler --validate`, run on a writable copy of the frontend, since the compiler
// writes persisted_queries.json even when validating and Bazel's inputs are read-only. Fails too
// if the compiler changed that file, which means it's missing a query (ADR 0088).
//
// Run by each frontend's relay_test, from the frontend's directory.
import { spawnSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, symlinkSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

const frontend = process.cwd();
const copy = mkdtempSync(join(process.env.TEST_TMPDIR ?? "/tmp", "relay-"));
cpSync(frontend, copy, {
  recursive: true,
  dereference: true,
  filter: (path) => !path.startsWith(join(frontend, "node_modules")),
});
// Bazel's inputs are read-only, and so is a copy of them.
spawnSync("chmod", ["-R", "u+w", copy]);
symlinkSync(join(frontend, "node_modules"), join(copy, "node_modules"));
const persisted = "persisted_queries.json";

const cli = join(
  dirname(createRequire(join(frontend, "package.json")).resolve("relay-compiler/package.json")),
  "cli.js",
);
const run = spawnSync(process.execPath, [cli, "--validate"], { cwd: copy, stdio: "inherit" });
if (run.status !== 0) process.exit(run.status ?? 1);

if (readFileSync(join(copy, persisted), "utf8") !== readFileSync(join(frontend, persisted), "utf8")) {
  console.error(`${persisted} is stale: run \`pnpm relay\` in this frontend and commit it.`);
  process.exit(1);
}
