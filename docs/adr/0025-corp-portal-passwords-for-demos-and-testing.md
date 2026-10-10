# 0025. Corp portal sign-in: usernames and passwords, for demos and testing only

- Status: Accepted
- Date: 2026-10-09
- Amends: 0024 (the corp portal needs no IdP client for local frontends)

## Context

0024 required corp portal sign-in before the corp portal is deployed anywhere reachable, and
assumed it would sign in against awzy's own IdP. awzy doesn't run an IdP for staff yet, and
setting one up (with SSO, groups, and security keys) is work that doesn't move the product. We
still want to deploy the corp portal for demos and to test it.

## Decision

- The corp portal signs staff in with a username and password. **This is for demos and testing
  only.** It is labelled that way in the code, on the sign-in page, and in the backend's log at
  startup.
- Accounts are a file of argon2id hashes (`AWZY_CORP_ACCOUNTS_FILE`), made with the
  `awzy-corp-user` CLI. There is no sign-up and no password reset; an admin edits the file.
- A session is a cookie signed with HMAC-SHA256: HttpOnly, SameSite=Strict, Secure, valid for 12
  hours. Changing the secret signs everyone out; there is no other revocation.
- After 5 failed attempts a username is locked for 15 minutes. Unknown usernames get the same
  answer, after the same delay, as wrong passwords.
- `/graphql` takes only `application/json` POSTs, so a cross-site form can't reach it.

## Consequences

- A local corp frontend can run against a deployed corp backend (0024) with no IdP client: the
  session cookie passes through the Vite proxy.
- Passwords can be phished and reused, and a file of accounts doesn't scale or audit well. That's
  acceptable while the corp portal shows install records and health and holds no client data
  (0009).
- **Before the corp portal can do anything that changes an install** (releases, fleet actions,
  support sessions, break-glass assistance), this is replaced by SSO against awzy's IdP with
  security keys, and a new record supersedes this one. Nothing new should be built on the
  password accounts.
- Lockouts are per backend replica and reset on restart. Fine for one replica in a demo; not a
  real rate limit.
