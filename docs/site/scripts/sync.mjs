// Copies the repository's INSTALL.md into the site as the install guide, so the runbook has one
// source. Links relative to the repository become links to it on GitHub.
import { readdirSync, readFileSync, writeFileSync } from "node:fs";

const repo = "https://github.com/maxh/awzy-security/blob/main/";
const source = new URL("../../../INSTALL.md", import.meta.url);
const target = new URL("../src/content/docs/guides/install.md", import.meta.url);

let body = readFileSync(source, "utf8");
// The first heading is the page title, from the frontmatter below.
body = body.replace(/^# .*\n+/, "");
// Notes for people reading the repository, between <!-- repo-only --> and <!-- /repo-only -->.
body = body.replace(/<!-- repo-only[\s\S]*?<!-- \/repo-only -->\n?/g, "");
// The site is for people installing awzy; the repository's developer and planning docs aren't
// written for them, so a link to one fails the build instead of reaching the page.
const internal = [...body.matchAll(/\]\((DEVELOPMENT\.md|docs\/(?:plans|adr)\/)[^)]*\)/g)].map((m) => m[0]);
if (internal.length) {
  throw new Error(`INSTALL.md links the repository's internal docs on the public site: ${internal.join(", ")}. Link a docs page instead, or mark the text <!-- repo-only -->.`);
}
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

// The configuration reference: every bootstrap variable, from deploy/terraform/bootstrap, so the
// page can't drift from the module. ADR citations in descriptions are dropped (see below).
{
  const tf = readFileSync(new URL("../../../deploy/terraform/bootstrap/variables.tf", import.meta.url), "utf8");
  const rows = [];
  for (const [, name, block] of tf.matchAll(/^variable "([^"]+)" \{\n([\s\S]*?)^\}/gm)) {
    const description = (block.match(/^  description\s*=\s*"((?:[^"\\]|\\.)*)"/m)?.[1] ?? "")
      .replace(/\\"/g, '"')
      .replace(/ ?\((?:ADRs? [0-9]{4}(?:, [0-9]{4})*(?:; [^)]*)?)\)/g, "")
      .replace(/\bADRs? [0-9]{4}\b,? ?/g, "")
      .replace(/\|/g, "\\|")
      .replace(/</g, "&lt;");
    const required = !/^  default\s*=/m.test(block);
    rows.push(`| \`${name}\` | ${required ? "Yes" : ""} | ${description} |`);
  }
  writeFileSync(
    new URL("../src/content/docs/reference/configuration.md", import.meta.url),
    `---
title: Configuration
description: Every setting an install takes, from bootstrap, the Terraform you apply in your cluster.
editUrl: ${repo.replace("/blob/", "/edit/")}deploy/terraform/bootstrap/variables.tf
---

An install is configured through bootstrap's variables
([deploy/terraform/bootstrap](${repo}deploy/terraform/bootstrap/variables.tf)), which you set in
your own Terraform, as [the example client](${repo}deploy/terraform/examples/example-client/bootstrap/main.tf)
does. Bootstrap turns them into the chart's values and the cluster's Secrets; you don't set chart
values directly. The [install guide](/guides/install/) says where each comes from.

| Variable | Required | What it's for |
| --- | --- | --- |
${rows.join("\n")}
`,
  );
  console.log(`generated the configuration reference (${rows.length} variables)`);
}

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
