# signing

Library: signs and verifies policy bundles with ECDSA P-256 and SHA-256, which every cloud KMS
supports ([ADR 0021](../../../docs/adr/0021-signed-policy-bundles.md)). Part of
[awzy-authz](../../README.md).

awzy-policy signs a bundle when it's published, and awzy-pdp verifies it before loading. A
bundle changed in the database after publishing no longer verifies, so tampering with the
policy store can't change what's enforced. The signed payload is text with one field per line
so it can't be ambiguous; the format is in `src/lib.rs`.
