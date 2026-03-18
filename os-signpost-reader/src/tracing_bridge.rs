//! Tracing adapter — converts signpost entries to `tracing` spans and events.
//!
//! Enable the `tracing` feature to use this module.
//!
//! # Usage
//!
//! ```rust,no_run
//! use os_signpost_reader::{SignpostReader, SignpostFilter};
//! use os_signpost_reader::tracing_bridge::SignpostTracingBridge;
//!
//! # async fn example() {
//! let (guard, rx) = SignpostReader::new(SignpostFilter::default())
//!     .start()
//!     .unwrap();
//! let bridge = SignpostTracingBridge::new(rx);
//! bridge.run().await;
//! # }
//! ```

use std::collections::HashMap;

use crate::reader::{SignpostEntry, SignpostType};

/// Maximum number of open (unmatched) spans before pruning stale entries.
const STALE_SPAN_THRESHOLD: usize = 10_000;

/// Bridges signpost entries to the `tracing` ecosystem.
///
/// Drains a [`flume::Receiver<SignpostEntry>`] and converts entries to
/// [`tracing::Span`] and [`tracing::Event`] values. With `tracing-opentelemetry`
/// in the subscriber stack, these flow directly to Tempo/Grafana.
pub struct SignpostTracingBridge {
    receiver: flume::Receiver<SignpostEntry>,
}

impl SignpostTracingBridge {
    /// Create a new bridge that will drain entries from the given receiver.
    pub fn new(receiver: flume::Receiver<SignpostEntry>) -> Self {
        Self { receiver }
    }

    /// Run the bridge, draining the receiver and emitting tracing spans/events.
    ///
    /// Runs until the receiver is disconnected (all senders dropped).
    pub async fn run(self) {
        // Map from signpost_id → (span, mach_timestamp) for interval correlation.
        // mach_timestamp is stored for stale-span pruning (oldest first).
        let mut open_spans: HashMap<u64, (tracing::Span, u64)> = HashMap::new();

        while let Ok(entry) = self.receiver.recv_async().await {
            match entry.signpost_type {
                SignpostType::IntervalBegin => {
                    let span = make_span(&entry);
                    // Enter briefly so the span is "activated" and its creation
                    // time is recorded by the subscriber.
                    let _enter = span.enter();
                    drop(_enter);
                    open_spans.insert(entry.signpost_id, (span, entry.mach_timestamp));

                    // Prune stale spans if the map grows too large.
                    if open_spans.len() > STALE_SPAN_THRESHOLD {
                        prune_stale_spans(&mut open_spans);
                    }
                }
                SignpostType::IntervalEnd => {
                    if let Some((span, _ts)) = open_spans.remove(&entry.signpost_id) {
                        // Enter and immediately drop to close the span, recording
                        // its duration in the subscriber.
                        let _enter = span.enter();
                        // If the end entry has a message, record it on the span.
                        if let Some(ref msg) = entry.message {
                            span.record("end_message", msg.as_str());
                        }
                    }
                    // If no matching begin was found, silently ignore — the begin
                    // may have been emitted before the bridge started.
                }
                SignpostType::Event => {
                    emit_event(&entry);
                }
            }
        }
    }
}

/// Create a tracing span from a signpost IntervalBegin entry.
fn make_span(entry: &SignpostEntry) -> tracing::Span {
    tracing::info_span!(
        "signpost",
        subsystem = %entry.subsystem,
        category = %entry.category,
        signpost_name = %entry.signpost_name,
        process_id = entry.process_id,
        thread_id = entry.thread_id,
        message = entry.message.as_deref().unwrap_or(""),
        end_message = tracing::field::Empty,
    )
}

/// Emit a tracing event from a signpost Event entry.
fn emit_event(entry: &SignpostEntry) {
    tracing::info!(
        subsystem = %entry.subsystem,
        category = %entry.category,
        signpost_name = %entry.signpost_name,
        process_id = entry.process_id,
        thread_id = entry.thread_id,
        message = entry.message.as_deref().unwrap_or(""),
    );
}

/// Remove the oldest half of open spans when the map exceeds the threshold.
fn prune_stale_spans(open_spans: &mut HashMap<u64, (tracing::Span, u64)>) {
    let mut entries: Vec<(u64, u64)> = open_spans.iter().map(|(&id, &(_, ts))| (id, ts)).collect();
    // Sort by mach_timestamp ascending (oldest first).
    entries.sort_by_key(|&(_, ts)| ts);
    // Remove the oldest half.
    let remove_count = entries.len() / 2;
    for (id, _) in entries.into_iter().take(remove_count) {
        open_spans.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn make_entry(
        signpost_id: u64,
        signpost_type: SignpostType,
        message: Option<&str>,
    ) -> SignpostEntry {
        SignpostEntry {
            subsystem: "com.test.subsystem".to_string(),
            category: "test-category".to_string(),
            signpost_name: "test-signpost".to_string(),
            signpost_id,
            signpost_type,
            timestamp: SystemTime::now(),
            mach_timestamp: signpost_id * 1000, // deterministic ordering
            process_id: 42,
            process_name: "test-process".to_string(),
            thread_id: 1,
            sender_image_path: "/usr/lib/test.dylib".to_string(),
            message: message.map(String::from),
            trace_id: 0,
            activity_id: 0,
            parent_activity_id: 0,
        }
    }

    #[tokio::test]
    async fn bridge_processes_entries_without_panic() {
        let (tx, rx) = flume::bounded(64);
        let bridge = SignpostTracingBridge::new(rx);

        // Send a mix of entry types.
        tx.send(make_entry(1, SignpostType::IntervalBegin, Some("start")))
            .unwrap();
        tx.send(make_entry(1, SignpostType::IntervalEnd, Some("done")))
            .unwrap();
        tx.send(make_entry(2, SignpostType::Event, Some("point event")))
            .unwrap();
        // Orphaned end (no matching begin) — should not panic.
        tx.send(make_entry(99, SignpostType::IntervalEnd, None))
            .unwrap();
        drop(tx);

        // Run the bridge — it will drain all entries then return when the
        // sender is dropped.
        bridge.run().await;
    }

    #[tokio::test]
    async fn interval_correlation() {
        let (tx, rx) = flume::bounded(64);
        let bridge = SignpostTracingBridge::new(rx);

        // Send begin/end pairs with different IDs.
        for id in 0..5 {
            tx.send(make_entry(id, SignpostType::IntervalBegin, None))
                .unwrap();
        }
        // End them in reverse order.
        for id in (0..5).rev() {
            tx.send(make_entry(id, SignpostType::IntervalEnd, None))
                .unwrap();
        }
        drop(tx);

        bridge.run().await;
        // Success = no panic, all spans correlated and closed.
    }

    #[tokio::test]
    async fn stale_span_cleanup() {
        let (tx, rx) = flume::bounded(STALE_SPAN_THRESHOLD + 100);
        let bridge = SignpostTracingBridge::new(rx);

        // Send more begins than the threshold without any ends.
        for id in 0..=(STALE_SPAN_THRESHOLD as u64) {
            tx.send(make_entry(id, SignpostType::IntervalBegin, None))
                .unwrap();
        }
        drop(tx);

        // Bridge should prune stale spans without panicking.
        bridge.run().await;
    }

    #[test]
    fn prune_removes_oldest_half() {
        let mut map: HashMap<u64, (tracing::Span, u64)> = HashMap::new();
        for i in 0..100 {
            map.insert(i, (tracing::Span::none(), i * 10));
        }

        prune_stale_spans(&mut map);

        assert_eq!(map.len(), 50);
        // The oldest 50 (mach_timestamp 0..490) should be removed.
        for i in 0..50 {
            assert!(!map.contains_key(&i), "id {i} should have been pruned");
        }
        for i in 50..100 {
            assert!(map.contains_key(&i), "id {i} should remain");
        }
    }
}
