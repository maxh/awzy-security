# awzyauthz

awzy's Go client library: an enforcement point in your own service or agent (ADR 0045, 0114). It
validates callers' tokens, asks awzy-pdp within a budget, applies the mode, surfaces challenges as
a result to wait on, falls back as awzy-gateway does, and records activity. It's held to the same
conformance spec as every awzy client library, `authz/sdks/conformance/scenarios.json`.

```sh
go vet ./... && go test -race ./...
```

How to use it is on the docs site's Client libraries section.
