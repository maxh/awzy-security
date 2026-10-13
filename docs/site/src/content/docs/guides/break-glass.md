---
title: Break-glass rollback
description: Rolling back a policy bundle that locks your admins out, without the portal or your IdP.
---

A bad policy bundle can deny the very actions your admins need to fix it in the client portal.
Break-glass is the way out that depends on none of the usual pieces: an admin signs a rollback
with a key enrolled at install, and awzy-policy restores an earlier bundle. It works when your
IdP, the PDPs, or the portal is the problem.

The portal tries to keep you from needing it: publishing a draft that would stop you
administering policies asks you to confirm first.

## Enrolling admin keys

Each admin who may roll back makes an ECDSA P-256 key pair and keeps the private key offline, on
a machine or in a password manager they trust:

```sh
openssl ecparam -name prime256v1 -genkey -noout | openssl pkcs8 -topk8 -nocrypt -out break-glass.pem
openssl ec -in break-glass.pem -pubout -out break-glass.pub.pem
```

Collect the public keys (the `.pub.pem` files) into one file and set it in the cluster layer's
Terraform, then apply it as in [Installing awzy](/guides/install/), step 5:

```hcl
break_glass_public_keys = file("break-glass-admins.pem")
```

Without it, break-glass is off. To add or remove an admin, change the file and apply again.

## Rolling back

An admin with an enrolled key and `kubectl` access to the cluster runs the tool inside
awzy-policy's own pod, passing their key on standard input so it's never written there:

```sh
kubectl -n awzy-system exec -i deploy/awzy-policy -- \
  awzy-break-glass --endpoint http://127.0.0.1:8080 --key - \
  --reason "forbid-all bundle locked us out" < break-glass.pem
```

It restores the bundle published before the active one, republished as a new version, and every
PDP picks it up at once. To restore a particular version instead, add `--to-version <N>`.

- A request is signed with the time it was made and refused after five minutes, and each is used
  once, so a captured request can't be replayed.
- Every use is recorded with the key that signed it and the reason, listed on the client
  portal's **Policies** page under "Break-glass rollbacks" with the version it restored and the
  bundle it published, and posted to your security team's Slack channel when Slack is connected.
- The restored bundle is signed like any other, so the PDPs accept it.

Then fix the bad policy in the client portal as usual, and publish the fix.
