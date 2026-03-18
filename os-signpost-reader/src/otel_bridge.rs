//! OpenTelemetry adapter — emits signpost entries as OTel spans via the global tracer.
//!
//! Enable the `opentelemetry` feature to use this module.
//!
//! This bridge uses the **global tracer** (`opentelemetry::global::tracer()`),
//! following the OTel library instrumentation pattern. The application owns
//! the `TracerProvider` and exporter configuration — this crate only depends
//! on the `opentelemetry` API crate, not the SDK or any exporter.
//!
//! # Usage
//!
//! ```rust,no_run
//! use os_signpost_reader::{SignpostReader, SignpostFilter};
//! use os_signpost_reader::otel_bridge::SignpostOtelBridge;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Application has already called global::set_tracer_provider(...)
//!
//! let (guard, rx) = SignpostReader::new(SignpostFilter::default()).start()?;
//! SignpostOtelBridge::new(rx).run().await;
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;

use opentelemetry::trace::{SpanBuilder, SpanKind, Tracer, TracerProvider};
use opentelemetry::{KeyValue, global};

use crate::{SignpostEntry, SignpostType};

/// Bridges signpost entries to OpenTelemetry spans via the global tracer.
///
/// Drains a [`flume::Receiver<SignpostEntry>`] and creates OTel spans using
/// whatever [`TracerProvider`] the application has configured globally.
///
/// Interval Begin/End pairs are correlated by `signpost_id` into a single
/// OTel span. Point events become zero-duration spans.
pub struct SignpostOtelBridge {
    receiver: flume::Receiver<SignpostEntry>,
}

impl SignpostOtelBridge {
    /// Create a new bridge that will drain entries from the given receiver.
    pub fn new(receiver: flume::Receiver<SignpostEntry>) -> Self {
        Self { receiver }
    }

    /// Run the bridge, draining the receiver and emitting OTel spans.
    ///
    /// Runs until the receiver is disconnected (all senders dropped).
    /// Uses `opentelemetry::global::tracer("os-signpost-reader")`.
    pub async fn run(self) {
        let provider = global::tracer_provider();
        let tracer = provider.tracer("os-signpost-reader");

        // In-flight intervals: signpost_id → begin entry
        let mut in_flight: HashMap<u64, SignpostEntry> = HashMap::new();

        while let Ok(entry) = self.receiver.recv_async().await {
            match entry.signpost_type {
                SignpostType::IntervalBegin => {
                    in_flight.insert(entry.signpost_id, entry);
                }
                SignpostType::IntervalEnd => {
                    if let Some(begin) = in_flight.remove(&entry.signpost_id) {
                        emit_interval_span(&tracer, &begin, &entry);
                    }
                }
                SignpostType::Event => {
                    emit_event_span(&tracer, &entry);
                }
            }
        }
    }
}

/// Emit a span for a correlated Begin/End interval.
fn emit_interval_span(tracer: &impl Tracer, begin: &SignpostEntry, end: &SignpostEntry) {
    let mut attributes = entry_attributes(begin);
    if let Some(ref msg) = end.message {
        attributes.push(KeyValue::new("signpost.end_message", msg.clone()));
    }

    let span = tracer.build(
        SpanBuilder::from_name(begin.signpost_name.clone())
            .with_kind(SpanKind::Internal)
            .with_start_time(begin.timestamp)
            .with_end_time(end.timestamp)
            .with_attributes(attributes),
    );
    drop(span);
}

/// Emit a zero-duration span for a point-in-time event.
fn emit_event_span(tracer: &impl Tracer, entry: &SignpostEntry) {
    let span = tracer.build(
        SpanBuilder::from_name(entry.signpost_name.clone())
            .with_kind(SpanKind::Internal)
            .with_start_time(entry.timestamp)
            .with_end_time(entry.timestamp)
            .with_attributes(entry_attributes(entry)),
    );
    drop(span);
}

/// Build OTel attributes from a signpost entry.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn make_entry(
        signpost_type: SignpostType,
        signpost_id: u64,
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
            trace_id: 0xAA,
            activity_id: 0xBB,
            parent_activity_id: 0xCC,
        }
    }

    #[test]
    fn attributes_include_all_fields() {
        let entry = make_entry(SignpostType::Event, 1, SystemTime::now());
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
        let mut entry = make_entry(SignpostType::Event, 1, SystemTime::now());
        entry.message = None;
        let attrs = entry_attributes(&entry);
        let find = |key: &str| attrs.iter().find(|kv| kv.key.as_str() == key);
        assert!(find("signpost.message").is_none());
    }

    #[tokio::test]
    async fn bridge_drains_receiver_without_panic() {
        let (tx, rx) = flume::bounded(16);
        let t0 = SystemTime::now();
        let t1 = t0 + Duration::from_millis(50);

        tx.send(make_entry(SignpostType::IntervalBegin, 1, t0))
            .unwrap();
        tx.send(make_entry(SignpostType::IntervalEnd, 1, t1))
            .unwrap();
        tx.send(make_entry(SignpostType::Event, 2, t1)).unwrap();
        // Orphan end — should not panic
        tx.send(make_entry(SignpostType::IntervalEnd, 999, t1))
            .unwrap();
        drop(tx);

        // Uses the no-op global tracer (no provider set up in tests)
        SignpostOtelBridge::new(rx).run().await;
    }
}
