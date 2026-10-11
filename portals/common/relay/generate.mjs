// Generates a portal's Relay artifacts (src/**/__generated__) and persisted_queries.json from its
// queries and schema.graphql, from nothing: the compiler adds to persisted_queries.json but never
// removes, and persists only the artifacts it writes. Neither is committed (ADR 0090).
//
// Run from the frontend's directory: by `pnpm relay`, and by its Bazel `relay` target.
import { spawnSync } from "node:child_process";
import { readdirSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

const frontend = process.cwd();

function removeGenerated(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const path = join(dir, entry.name);
    if (entry.name === "__generated__") rmSync(path, { recursive: true, force: true });
    else removeGenerated(path);
  }
}
removeGenerated(join(frontend, "src"));
writeFileSync(join(frontend, "persisted_queries.json"), "{}\n");

const compiler = createRequire(join(frontend, "package.json")).resolve("relay-compiler/package.json");
const run = spawnSync(process.execPath, [join(dirname(compiler), "cli.js")], { cwd: frontend, stdio: "inherit" });
process.exit(run.status ?? 1);
