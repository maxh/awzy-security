---
name: lint-fix
description: Run awzy's Rust and TypeScript linters and formatters, and fix what they report. Use before committing or opening a PR, when CI's "Lint Rust" or "Lint TypeScript" step fails, or when the pre-commit hook refuses a commit.
---

# Lint and fix

awzy checks Rust with clippy and rustfmt (through Bazel) and TypeScript with oxlint and oxfmt
(through pnpm). Warnings fail in CI. Fix the code; don't weaken the configuration.

## 1. Format first

Formatting fixes are mechanical, so apply them before reading lint output:

```sh
pnpm install                             # once, for oxfmt and oxlint
tools/fmt                                # rustfmt (rustfmt.toml) and oxfmt (.oxfmtrc.json), every file
```

To format only some Rust files:
`bazel run @rules_rust//tools/upstream_wrapper:rustfmt -- --config-path "$PWD/rustfmt.toml" <files>`.
(`bazel run @rules_rust//:rustfmt` formats nothing here: it only picks up targets that set
`edition` explicitly.)

## 2. Lint

```sh
bazel build --config=lint --keep_going //... 2>&1 | tee /tmp/awzy-lint.log
grep -E -A12 '^(error|warning)' /tmp/awzy-lint.log    # clippy and rustfmt findings
pnpm lint                                              # oxlint, with type-aware rules
```

`--keep_going` reports every crate in one pass. The first run in a fresh worktree compiles
everything (about 15 minutes); later runs are incremental.

## 3. Fix

- Prefer changing the code to what the lint asks for. Most clippy suggestions are a line or two.
- Suppress a lint only when following it would make the code worse. Then use
  `#[expect(clippy::<lint>, reason = "...")]` on the smallest item (or `#![expect]` at the top
  of a module), never `#[allow]`: `expect` warns once the code no longer triggers the lint.
- For TypeScript, a deliberate exception gets
  `// oxlint-disable-next-line <rule> -- <why>`. Turning a rule off for the whole repo belongs
  in `.oxlintrc.json` with a comment saying why.
- Rerun step 2 until both pass, then run `bazel test //...`: some fixes change types.

## The pre-commit hook

`tools/git-hooks/pre-commit` (on with `git config core.hooksPath tools/git-hooks`) checks only
staged content: rustfmt, oxfmt, and oxlint's rules that don't need types. When it refuses a
commit, run `tools/fmt` (or fix the oxlint finding it printed), `git add` the files it
named, and commit again. Don't bypass it with
`--no-verify` unless the person you're working with asks.
