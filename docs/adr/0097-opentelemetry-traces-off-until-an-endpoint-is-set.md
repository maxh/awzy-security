# 0097. OpenTelemetry traces, off until an endpoint is set

- Status: Accepted
- Date: 2026-10-11

## Context

Metrics are Prometheus on every service (0068); nothing traced a request across services. The
deployment plan's performance testing wants a slow check to open as one trace across the services
it touched: the enforcement point, the PDP, awzy-context, and awzy-challenge.

## Decision

- **`libs/telemetry` exports spans over OTLP (gRPC)** when `OTEL_EXPORTER_OTLP_ENDPOINT` is set,
  to the install's own collector; without it, nothing changes. `OTEL_TRACES_SAMPLER_ARG` is the
  share of traces sent, default all, so the collector can sample by tail and keep the slow ones.
  Services don't configure tracing themselves.
- **A trace crosses calls in W3C trace context.** `awzy-server` opens a span for each request that
  continues the caller's `traceparent`; the service-auth gRPC interceptor sends the current one
  with every call between awzy's services.
- **Spans stay in the install.** They carry the request's method and path, which can name
  awzy's own IDs, and go only to the collector the client configures, in their account.
- The chart's `tracing.otlpEndpoint` sets it for every service.

## Consequences

- Turning traces on is one chart value; a collector (OpenTelemetry or ADOT, with tail sampling and
  an X-Ray or Tempo backend) is the client's to run, or comes with the AWS monitoring module later.
- Every request has a span even with traces off; it costs nothing beyond what `tracing` already
  does.
