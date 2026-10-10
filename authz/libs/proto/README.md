# proto

Protobuf definitions and generated Rust types for awzy-authz's gRPC APIs, with conversions to
and from `libs/model`. Part of [awzy-authz](../../README.md).

- `awzy.authz.v1` (`awzy/authz/v1/authz.proto`): `Authorizer/Check`, the decision API
  enforcement points call.
- `awzy.policy.v1`: versioned policy bundles.

These files are a public contract: enforcement points and SDKs outside this repository are
generated from them ([ADR 0045](../../../docs/adr/0045-sdks-are-enforcement-points-built-on-a-stable-proto.md)).
CI runs `buf lint`, `buf format`, and `buf breaking` against `main` (`buf.yaml` at the repository
root); a breaking change needs a new package version, such as `awzy.authz.v2`.

Bindings are built with protox and tonic, so no `protoc` is needed. awzy's own IDs are QID
strings; IDs from the client's systems stay as they are
([ADR 0013](../../../docs/adr/0013-qids-and-utc-timestamps.md)).
