---
title: Security model
description: What awzy can and can't see, and how releases reach your install.
---

- **Single-tenant, in your account.** Each client runs its own install in its own cloud
  account. There is no shared awzy service holding client data.
- **No vendor access.** awzy has no credentials for your install, even in an emergency. Repairs
  are run by your team.
- **Pull-based, signed releases.** Your install pulls releases from awzy's registry with its own
  revocable token. Every image is signed with awzy's release key, and the cluster refuses any
  image in awzy's namespaces that isn't from that registry with that signature, third-party
  images included.
- **A heartbeat, and nothing else.** The only thing that leaves your install is a heartbeat to
  awzy's corp portal: versions, readiness, and a few aggregate numbers, in a fixed schema with
  no free-form fields. You can turn it off.
- **Services authenticate to each other.** Inside the install, every call between awzy's
  services carries a short-lived Kubernetes service-account token, checked by the receiver.
- **Policy is signed.** PDPs load only bundles signed by awzy-policy's key, and every publish,
  rollback, and mode change is recorded with who did it.
