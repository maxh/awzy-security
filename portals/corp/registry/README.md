# awzy-registry · release registry proxy

The pull-only registry at https://registry.awzysecurity.com that installs pull awzy's images,
signatures, and Helm chart from, in any cloud
([ADR 0028](../../../docs/adr/0028-release-registry-for-any-cloud.md)). Part of the
[corp portal](../).

- Installs authenticate with HTTP Basic: the install ID as the username, its pull token as the
  password. Tokens are stored hashed in Postgres (the `registry` schema).
- Only `GET` and `HEAD` on `awzy/*` repositories are allowed; nothing can be pushed through it.
- Manifests are fetched from ECR with the proxy's own AWS credentials (SigV4). Layer downloads
  follow ECR's redirect to a presigned S3 URL, so image bytes never pass through the proxy.

```sh
awzy-registry serve                              # needs AWZY_POSTGRES_URL, AWZY_REGISTRY_UPSTREAM
awzy-registry token create <install_id> <by>     # prints a new pull token, once
awzy-registry token revoke <install_id>
```
