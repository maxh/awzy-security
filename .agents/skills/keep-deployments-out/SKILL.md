---
name: keep-deployments-out
description: Keep this public repository free of anything that identifies a particular deployment, account, person, or customer. Use whenever writing code, Terraform, docs, ADRs, commit messages, or PR and issue text here, and before every push.
---

# Keep deployments out of the repository

This repository is public, and anyone can deploy it (ADR 0053). So nothing in it may identify
a particular deployment: not awzy's own, not the example client's, not a real client's. That covers
files, commit messages, and PR and issue text alike: all of it is public, indexed, and kept
forever in history.

## Never in the repository

- **Account and organization identifiers:** AWS account IDs (any 12-digit number), organization
  and OU IDs, Cloudflare account and zone IDs, GitHub numeric owner and repository IDs.
- **Resource identifiers of a real deployment:** ARNs, CloudFront distribution IDs, bucket names
  that embed an account ID, ECS revisions, KMS key IDs, security group and subnet IDs, IP
  addresses.
- **Sign-in and support details:** IAM Identity Center portal URLs and instance IDs
  (`ssoins-…`, `d-….awsapps.com`), support case and quota request numbers.
- **Real records:** the QIDs of real installs, decisions, or bundles (`install_<uuid>` from the
  corp portal), install IDs, reporting or pull tokens, and any key or secret.
- **People and clients:** email addresses, user names, and names of clients or design
  partners. The synthetic company (Larkspur Capital) and `example-client` are fine; they're
  ours to invent.

## Where those live instead

- **`deployment.env`** (gitignored, per deployment, backed up with `tools/deployment-env`) for
  everything Terraform needs: accounts, profiles, buckets, domains, zone IDs.
- **GitHub Actions variables and secrets** for what CI needs (`AWZY_CORP_*`).
- **Secrets Manager** for credentials, and the **internal wiki** for runbooks, account maps,
  case numbers, and client notes.
- **The conversation** with the person you're working with: say the ID there, not in a commit.

## What is fine

- The product's own public names as defaults or examples, where the code still takes them as
  configuration: `awzy`, `registry.awzysecurity.com`, `docs.awzysecurity.com`.
- Documentation placeholders: `example.com`, `111111111111` (and AWS's own `111122223333`),
  `0123456789abcdef…`,
  `my-org-terraform-state-111111111111`, `owner@12345/repo@67890`.
- QIDs and IDs in tests and examples that were made up for them.

## How to write around them

- Name the role, not the instance: "the management account", "the build account's state
  bucket", "the example install", "the open quota request".
- Status notes say what happened, not which record: "registered in the corp portal", "the
  account quota increase is pending with AWS".
- Anything a reader must look up names where to look: "`terraform output docs_bucket`", "the
  `awzy-corp-portal/accounts` secret", "the management account's Service Quotas history".

## Checked automatically

`tools/ci/check-identifiers.sh` looks for AWS account IDs, ARNs with an account, sign-in portal
URLs, real install QIDs, and awzy email addresses:

- the pre-commit hook runs it on staged lines (`--staged`);
- CI's `identifiers` workflow runs it on every tracked file and on the PR's title and
  description, again whenever they're edited.

A true placeholder that trips it goes in `tools/ci/identifier-allowlist.txt`. The check can't
know names, case numbers, or resource IDs without a fixed shape, so still read your diff and
your PR text against the list above before posting.

## If one slipped

- **PR or issue text:** edit it (`gh pr edit <n> --body …`, `gh issue edit`). GitHub keeps an
  edit history visible to maintainers, so say so to the user.
- **A file:** remove it in a PR now. It's still in git history; rewriting history is the user's
  call, so tell them what leaked and where, and ask.
- **A secret or token:** treat it as compromised. Tell the user to rotate it first; cleaning up
  comes second.
