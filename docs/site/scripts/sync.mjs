// Copies the repository's INSTALL.md into the site as the install guide, so the runbook has one
// source. Links relative to the repository become links to it on GitHub.
import { readFileSync, writeFileSync } from "node:fs";

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
