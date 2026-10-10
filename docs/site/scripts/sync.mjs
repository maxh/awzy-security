// Copies the repository's INSTALL.md into the site as the install guide, so the runbook has one
// source. Links relative to the repository become links to it on GitHub.
import { readdirSync, readFileSync, writeFileSync } from "node:fs";

const repo = "https://github.com/maxh/awzy-security/blob/main/";
const source = new URL("../../../INSTALL.md", import.meta.url);
const target = new URL("../src/content/docs/guides/install.md", import.meta.url);

let body = readFileSync(source, "utf8");
// The first heading is the page title, from the frontmatter below.
body = body.replace(/^# .*\n+/, "");
// [text](path) where path isn't a URL or an anchor: a file in the repository.
body = body.replace(/\]\((?!https?:|#|mailto:)([^)]+)\)/g, (_, path) => `](${repo}${path.replace(/^\.\//, "")})`);

writeFileSync(
  target,
  `---
title: Installing awzy
description: Install awzy in your own cloud account, from the cloud layer to the first decision.
editUrl: ${repo.replace("/blob/", "/edit/")}INSTALL.md
---

${body}`,
);
console.log("synced INSTALL.md");

// The public docs describe the product, not how it was decided: no architecture decision records
// (ADRs), which are for people working on awzy. Fails the build if one is cited.
const docs = new URL("../src/content/docs/", import.meta.url);
const cited = [];
for (const file of readdirSync(docs, { recursive: true })) {
  if (!/\.mdx?$/.test(file)) continue;
  readFileSync(new URL(file, docs), "utf8")
    .split("\n")
    .forEach((line, i) => {
      if (/\bADRs?\b|docs\/adr\//.test(line)) cited.push(`${file}:${i + 1}: ${line.trim()}`);
    });
}
if (cited.length) {
  console.error(`The public docs cite ADRs; describe the behavior instead:\n${cited.join("\n")}`);
  process.exit(1);
}
