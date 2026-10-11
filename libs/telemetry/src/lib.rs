//! Logging and tracing setup shared by every service.
//!
//! `RUST_LOG` sets the filter (default `info`). `AWZY_LOG_FORMAT=json` switches to JSON lines.
//! With `OTEL_EXPORTER_OTLP_ENDPOINT` set, spans also go out as OpenTelemetry traces over OTLP
//! (gRPC), to the install's own collector (ADR 0097); `OTEL_TRACES_SAMPLER_ARG` is the share of
//! traces sent (default all, so the collector can keep the slow ones). Services never configure
//! any of it themselves: [`trace_headers`] and [`continue_trace`] carry a trace across calls.

use std::collections::HashMap;

use opentelemetry::{
    global,
    propagation::{Extractor, Injector},
    trace::TracerProvider as _,
};
use opentelemetry_sdk::{
    propagation::TraceContextPropagator,
    trace::{Sampler, SdkTracerProvider},
    Resource,
};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init(service: &'static str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let json = std::env::var("AWZY_LOG_FORMAT").is_ok_and(|v| v == "json");
    let traces = match std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        Ok(_) => match tracer(service) {
            Ok(tracer) => Some(tracing_opentelemetry::layer().with_tracer(tracer)),
            Err(e) => {
                eprintln!("traces are off: {e}");
                None
            }
        },
        Err(_) => None,
    };
    let registry = tracing_subscriber::registry().with(filter).with(traces);
    if json {
        registry.with(fmt::layer().json()).init();
    } else {
        registry.with(fmt::layer()).init();
    }
    tracing::debug!(service, "telemetry initialized");
}

/// Spans for `service`, batched to the OTLP endpoint the standard variables name.
fn tracer(service: &'static str) -> anyhow::Result<opentelemetry_sdk::trace::SdkTracer> {
    let exporter = opentelemetry_otlp::SpanExporter::builder().with_tonic().build()?;
    let ratio = std::env::var("OTEL_TRACES_SAMPLER_ARG").ok().and_then(|r| r.parse::<f64>().ok()).unwrap_or(1.0);
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(ratio.clamp(0.0, 1.0)))))
        .with_resource(Resource::builder().with_service_name(service).build())
        .build();
    global::set_tracer_provider(provider.clone());
    global::set_text_map_propagator(TraceContextPropagator::new());
    Ok(provider.tracer(service))
}

struct Headers(HashMap<String, String>);

impl Injector for Headers {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key.to_string(), value);
    }
}

impl Extractor for Headers {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(String::as_str).collect()
    }
}

/// The current span's trace context, as headers to add to an outgoing call (`traceparent`,
/// `tracestate`). Empty when traces are off.
pub fn trace_headers() -> Vec<(String, String)> {
    let mut headers = Headers(HashMap::new());
    let context = tracing::Span::current().context();
    global::get_text_map_propagator(|p| p.inject_context(&context, &mut headers));
    headers.0.into_iter().collect()
}

/// Makes `span` part of the trace an incoming call carries, reading its headers with `get`.
pub fn continue_trace(span: &tracing::Span, get: impl Fn(&str) -> Option<String>) {
    let headers =
        Headers(["traceparent", "tracestate"].iter().filter_map(|k| Some((k.to_string(), get(k)?))).collect());
    let parent = global::get_text_map_propagator(|p| p.extract(&headers));
    let _ = span.set_parent(parent);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trace_crosses_a_call_in_its_headers() {
        global::set_text_map_propagator(TraceContextPropagator::new());
        let incoming = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let headers = Headers([("traceparent".to_string(), incoming.to_string())].into());
        let context = global::get_text_map_propagator(|p| p.extract(&headers));
        let mut out = Headers(HashMap::new());
        global::get_text_map_propagator(|p| p.inject_context(&context, &mut out));
        assert_eq!(out.0.get("traceparent").map(String::as_str), Some(incoming));
    }
}
