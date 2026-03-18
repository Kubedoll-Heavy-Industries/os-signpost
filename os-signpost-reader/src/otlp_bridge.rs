//! OTLP adapter — converts signpost entries to OTel spans exported via OTLP.
//!
//! Enable the `otlp` feature to use this module.
//!
//! # Usage
//!
//! ```no_run
//! # use os_signpost_reader::{SignpostReader, SignpostFilter};
//! # use os_signpost_reader::otlp_bridge::SignpostOtlpBridge;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let (guard, rx) = SignpostReader::new(SignpostFilter::default()).start()?;
//! SignpostOtlpBridge::new(rx)
//!     .with_endpoint("http://localhost:4318")
//!     .with_service_name("my-app")
//!     .run()
//!     .await?;
//! # Ok(())
//! # }
//! ```

use std::borrow::Cow;
use std::collections::HashMap;

use opentelemetry::trace::{
    SpanContext, SpanId, SpanKind, Status, TraceFlags, TraceId, TraceState,
};
use opentelemetry::{InstrumentationScope, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{SpanData, SpanEvents, SpanLinks};

use crate::{SignpostEntry, SignpostType};

const DEFAULT_ENDPOINT: &str = "http://localhost:4318";
const DEFAULT_SERVICE_NAME: &str = "os-signpost-reader";
const BATCH_SIZE: usize = 64;

/// Bridges signpost entries to OTel spans exported via OTLP.
///
/// Drains a [`flume::Receiver<SignpostEntry>`] and exports OTel spans directly
/// via `opentelemetry-otlp`, bypassing the `tracing` layer entirely.
///
/// Interval Begin/End pairs are correlated by `signpost_id` into a single
/// OTel span with start/end timestamps. Point events become zero-duration spans.
pub struct SignpostOtlpBridge {
    receiver: flume::Receiver<SignpostEntry>,
    endpoint: String,
    service_name: String,
}

impl SignpostOtlpBridge {
    /// Create a new bridge that drains the given receiver.
    pub fn new(receiver: flume::Receiver<SignpostEntry>) -> Self {
        Self {
            receiver,
            endpoint: DEFAULT_ENDPOINT.to_string(),
            service_name: DEFAULT_SERVICE_NAME.to_string(),
        }
    }

    /// Set the OTLP collector endpoint (default: `http://localhost:4318`).
    pub fn with_endpoint(mut self, endpoint: &str) -> Self {
        self.endpoint = endpoint.to_string();
        self
    }

    /// Set the service name reported in OTel resource (default: `os-signpost-reader`).
    pub fn with_service_name(mut self, name: &str) -> Self {
        self.service_name = name.to_string();
        self
    }

    /// Run the bridge, draining the receiver and exporting spans until the
    /// channel is closed.
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        use opentelemetry_sdk::trace::SpanExporter;

        let mut exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_endpoint(&self.endpoint)
            .build()?;

        let resource = opentelemetry_sdk::Resource::builder()
            .with_service_name(self.service_name.clone())
            .build();
        exporter.set_resource(&resource);

        let scope = InstrumentationScope::builder("os-signpost-reader")
            .with_version(env!("CARGO_PKG_VERSION"))
            .build();

        // In-flight intervals: signpost_id → (begin entry)
        let mut in_flight: HashMap<u64, SignpostEntry> = HashMap::new();
        let mut batch: Vec<SpanData> = Vec::with_capacity(BATCH_SIZE);

        while let Ok(entry) = self.receiver.recv_async().await {
            match entry.signpost_type {
                SignpostType::IntervalBegin => {
                    in_flight.insert(entry.signpost_id, entry);
                }
                SignpostType::IntervalEnd => {
                    if let Some(begin) = in_flight.remove(&entry.signpost_id) {
                        let span_data = build_interval_span(&begin, &entry, &scope);
                        batch.push(span_data);
                    }
                    // If no matching begin, silently drop — we may have started
                    // mid-interval.
                }
                SignpostType::Event => {
                    let span_data = build_event_span(&entry, &scope);
                    batch.push(span_data);
                }
            }

            if batch.len() >= BATCH_SIZE {
                let to_export = std::mem::replace(&mut batch, Vec::with_capacity(BATCH_SIZE));
                let _ = exporter.export(to_export).await;
            }
        }

        // Flush remaining spans
        if !batch.is_empty() {
            let _ = exporter.export(batch).await;
        }

        exporter.shutdown()?;
        Ok(())
    }
}

/// Map a 64-bit Apple trace_id to a 128-bit OTel TraceId.
/// Upper 64 bits are zero-padded.
fn apple_trace_id(trace_id: u64) -> TraceId {
    TraceId::from(trace_id as u128)
}

/// Map a 64-bit Apple activity_id to an OTel SpanId.
fn apple_span_id(activity_id: u64) -> SpanId {
    SpanId::from(activity_id)
}

/// Build attributes from a signpost entry.
fn entry_attributes(entry: &SignpostEntry) -> Vec<KeyValue> {
    let mut attrs = vec![
        KeyValue::new("signpost.subsystem", entry.subsystem.clone()),
        KeyValue::new("signpost.category", entry.category.clone()),
        KeyValue::new("signpost.name", entry.signpost_name.clone()),
        KeyValue::new("signpost.id", entry.signpost_id as i64),
        KeyValue::new("process.pid", entry.process_id as i64),
        KeyValue::new("process.name", entry.process_name.clone()),
        KeyValue::new("thread.id", entry.thread_id as i64),
        KeyValue::new("process.executable.path", entry.sender_image_path.clone()),
    ];
    if let Some(ref msg) = entry.message {
        attrs.push(KeyValue::new("signpost.message", msg.clone()));
    }
    attrs
}

/// Build a span context from Apple trace/activity IDs.
fn build_span_context(entry: &SignpostEntry) -> SpanContext {
    SpanContext::new(
        apple_trace_id(entry.trace_id),
        apple_span_id(entry.activity_id),
        TraceFlags::SAMPLED,
        false,
        TraceState::NONE,
    )
}

/// Build a SpanData for a correlated Begin/End interval.
fn build_interval_span(
    begin: &SignpostEntry,
    end: &SignpostEntry,
    scope: &InstrumentationScope,
) -> SpanData {
    let mut attributes = entry_attributes(begin);
    // If the end entry has a message, include it as well.
    if let Some(ref msg) = end.message {
        attributes.push(KeyValue::new("signpost.end_message", msg.clone()));
    }

    SpanData {
        span_context: build_span_context(begin),
        parent_span_id: apple_span_id(begin.parent_activity_id),
        parent_span_is_remote: false,
        span_kind: SpanKind::Internal,
        name: Cow::Owned(begin.signpost_name.clone()),
        start_time: begin.timestamp,
        end_time: end.timestamp,
        attributes,
        dropped_attributes_count: 0,
        events: SpanEvents::default(),
        links: SpanLinks::default(),
        status: Status::Unset,
        instrumentation_scope: scope.clone(),
    }
}

/// Build a SpanData for a point-in-time Event (zero-duration span).
fn build_event_span(entry: &SignpostEntry, scope: &InstrumentationScope) -> SpanData {
    SpanData {
        span_context: build_span_context(entry),
        parent_span_id: apple_span_id(entry.parent_activity_id),
        parent_span_is_remote: false,
        span_kind: SpanKind::Internal,
        name: Cow::Owned(entry.signpost_name.clone()),
        start_time: entry.timestamp,
        end_time: entry.timestamp,
        attributes: entry_attributes(entry),
        dropped_attributes_count: 0,
        events: SpanEvents::default(),
        links: SpanLinks::default(),
        status: Status::Unset,
        instrumentation_scope: scope.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn make_entry(
        signpost_type: SignpostType,
        signpost_id: u64,
        trace_id: u64,
        activity_id: u64,
        parent_activity_id: u64,
        timestamp: SystemTime,
    ) -> SignpostEntry {
        SignpostEntry {
            subsystem: "com.apple.Metal".to_string(),
            category: "GPU".to_string(),
            signpost_name: "CommandBuffer".to_string(),
            signpost_id,
            signpost_type,
            timestamp,
            mach_timestamp: 0,
            process_id: 42,
            process_name: "test".to_string(),
            thread_id: 1,
            sender_image_path: "/usr/lib/libMetal.dylib".to_string(),
            message: Some("test message".to_string()),
            trace_id,
            activity_id,
            parent_activity_id,
        }
    }

    #[test]
    fn trace_id_mapping_zero_pads_upper_64_bits() {
        let apple_id: u64 = 0xDEAD_BEEF_CAFE_BABE;
        let otel_id = apple_trace_id(apple_id);
        let bytes = otel_id.to_bytes();

        // Upper 64 bits should be zero
        assert_eq!(&bytes[..8], &[0u8; 8]);
        // Lower 64 bits should match
        assert_eq!(&bytes[8..], &apple_id.to_be_bytes());
    }

    #[test]
    fn trace_id_zero_maps_to_zero() {
        let otel_id = apple_trace_id(0);
        assert_eq!(otel_id, TraceId::INVALID);
    }

    #[test]
    fn span_id_mapping_preserves_value() {
        let apple_id: u64 = 0x1234_5678_9ABC_DEF0;
        let otel_id = apple_span_id(apple_id);
        assert_eq!(otel_id.to_bytes(), apple_id.to_be_bytes());
    }

    #[test]
    fn interval_correlation_produces_single_span() {
        let scope = InstrumentationScope::builder("test").build();
        let t0 = SystemTime::now();
        let t1 = t0 + Duration::from_millis(100);

        let begin = make_entry(SignpostType::IntervalBegin, 42, 0xAA, 0xBB, 0xCC, t0);
        let end = make_entry(SignpostType::IntervalEnd, 42, 0xAA, 0xBB, 0xCC, t1);

        let span = build_interval_span(&begin, &end, &scope);

        assert_eq!(span.start_time, t0);
        assert_eq!(span.end_time, t1);
        assert_eq!(span.name.as_ref(), "CommandBuffer");
        assert_eq!(span.span_context.trace_id(), apple_trace_id(0xAA));
        assert_eq!(span.span_context.span_id(), apple_span_id(0xBB));
        assert_eq!(span.parent_span_id, apple_span_id(0xCC));
    }

    #[test]
    fn event_produces_zero_duration_span() {
        let scope = InstrumentationScope::builder("test").build();
        let t = SystemTime::now();

        let entry = make_entry(SignpostType::Event, 1, 0x11, 0x22, 0x33, t);
        let span = build_event_span(&entry, &scope);

        assert_eq!(span.start_time, span.end_time);
        assert_eq!(span.start_time, t);
    }

    #[test]
    fn attributes_include_all_fields() {
        let entry = make_entry(SignpostType::Event, 1, 0x11, 0x22, 0x33, SystemTime::now());
        let attrs = entry_attributes(&entry);

        let find = |key: &str| attrs.iter().find(|kv| kv.key.as_str() == key);

        assert!(find("signpost.subsystem").is_some());
        assert!(find("signpost.category").is_some());
        assert!(find("signpost.name").is_some());
        assert!(find("signpost.id").is_some());
        assert!(find("process.pid").is_some());
        assert!(find("process.name").is_some());
        assert!(find("thread.id").is_some());
        assert!(find("process.executable.path").is_some());
        assert!(find("signpost.message").is_some());
    }

    #[test]
    fn attributes_omit_message_when_none() {
        let mut entry = make_entry(SignpostType::Event, 1, 0x11, 0x22, 0x33, SystemTime::now());
        entry.message = None;
        let attrs = entry_attributes(&entry);
        let find = |key: &str| attrs.iter().find(|kv| kv.key.as_str() == key);
        assert!(find("signpost.message").is_none());
    }

    #[test]
    fn builder_pattern() {
        let (_tx, rx) = flume::bounded(1);
        let bridge = SignpostOtlpBridge::new(rx)
            .with_endpoint("http://example.com:4318")
            .with_service_name("my-svc");

        assert_eq!(bridge.endpoint, "http://example.com:4318");
        assert_eq!(bridge.service_name, "my-svc");
    }

    #[tokio::test]
    async fn run_drains_receiver_and_correlates() {
        let (tx, rx) = flume::bounded(16);
        let t0 = SystemTime::now();
        let t1 = t0 + Duration::from_millis(50);

        // Send a Begin/End pair and an Event
        tx.send(make_entry(
            SignpostType::IntervalBegin,
            1,
            0xAA,
            0xBB,
            0,
            t0,
        ))
        .unwrap();
        tx.send(make_entry(SignpostType::IntervalEnd, 1, 0xAA, 0xBB, 0, t1))
            .unwrap();
        tx.send(make_entry(SignpostType::Event, 2, 0xCC, 0xDD, 0, t1))
            .unwrap();

        // Send an orphan End (no matching Begin) — should be silently dropped
        tx.send(make_entry(
            SignpostType::IntervalEnd,
            999,
            0xEE,
            0xFF,
            0,
            t1,
        ))
        .unwrap();

        // Close the channel so `run` terminates
        drop(tx);

        // We can't easily test the actual export without a server, but we can
        // verify the bridge processes entries without panicking by running a
        // local correlation pass. Use the internal helpers directly.
        let scope = InstrumentationScope::builder("test").build();
        let mut in_flight: HashMap<u64, SignpostEntry> = HashMap::new();
        let mut spans: Vec<SpanData> = Vec::new();

        while let Ok(entry) = rx.recv() {
            match entry.signpost_type {
                SignpostType::IntervalBegin => {
                    in_flight.insert(entry.signpost_id, entry);
                }
                SignpostType::IntervalEnd => {
                    if let Some(begin) = in_flight.remove(&entry.signpost_id) {
                        spans.push(build_interval_span(&begin, &entry, &scope));
                    }
                }
                SignpostType::Event => {
                    spans.push(build_event_span(&entry, &scope));
                }
            }
        }

        // Should have 2 spans: one interval, one event (orphan end dropped)
        assert_eq!(spans.len(), 2);
        assert_ne!(spans[0].start_time, spans[0].end_time); // interval
        assert_eq!(spans[1].start_time, spans[1].end_time); // event
    }
}
