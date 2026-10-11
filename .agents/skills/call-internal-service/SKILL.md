---
name: call-internal-service
description: Call one awzy service from another (awzy-context, awzy-connectors, awzy-reporter, the PDP, the Kubernetes API) over HTTP or gRPC. Use whenever code reads another service's API.
---

# Calling another awzy service

## Typed, never by field name

Deserialize a response into a serde struct that names its fields, and let a mismatch fail. Don't
read `value["field"]` with `unwrap_or_default()`: a renamed field then shows up as empty data
instead of an error. `tools/ci/check-quality-ratchets.sh` counts `["field"]` indexing outside
tests and fails if it grows (#221).

## One client per API

Use the service's client (and add the call to it, with its typed models) rather than building a
request where you need it (#222). Don't build a hyper client or read `KUBERNETES_SERVICE_HOST`
yourself; the ratchet counts those too (#223).

## Service tokens

Every call to an awzy service carries this workload's service token for that audience
(`awzy_service_auth::TokenSource`, ADR 0055), and the callee's guard lists who may call it. A new
caller needs adding to that list in the chart.

## Errors

Map the callee's answer to a typed error (not found, refused with a reason, unavailable) rather
than a string, so the caller can act on it (#224).
