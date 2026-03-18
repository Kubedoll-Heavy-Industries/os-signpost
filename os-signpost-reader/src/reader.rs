//! Core reader types and implementation.

use std::time::SystemTime;

/// A decoded signpost entry from the system.
#[derive(Debug, Clone)]
pub struct SignpostEntry {
    /// The subsystem that emitted the signpost (e.g. `"com.apple.Metal"`).
    pub subsystem: String,
    /// The category within the subsystem.
    pub category: String,
    /// The signpost name.
    pub signpost_name: String,
    /// The signpost ID, used to correlate Begin/End pairs.
    pub signpost_id: u64,
    /// Whether this is a begin, end, or point event.
    pub signpost_type: SignpostType,

    /// Wall-clock timestamp from `OSActivityEvent.timestamp`.
    /// May be non-monotonic across NTP adjustments.
    /// Use [`mach_timestamp`](Self::mach_timestamp) for ordering.
    pub timestamp: SystemTime,
    /// Mach absolute time — monotonic, high-precision. Authoritative ordering key.
    pub mach_timestamp: u64,

    /// The process that emitted the signpost.
    pub process_id: i32,
    /// Process name.
    pub process_name: String,
    /// Thread ID within the process.
    pub thread_id: u64,
    /// Path to the dylib/binary that emitted the signpost.
    pub sender_image_path: String,

    /// The formatted message payload, if any.
    pub message: Option<String>,

    /// Apple's 64-bit trace identifier. Maps to the lower 64 bits of an OTel
    /// TraceId (upper 64 bits zero-padded).
    pub trace_id: u64,
    /// Apple's activity identifier. Maps to OTel SpanId.
    pub activity_id: u64,
    /// Parent activity identifier. Maps to OTel parent SpanId.
    pub parent_activity_id: u64,
}

/// The type of signpost entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignpostType {
    /// The beginning of a signpost interval.
    IntervalBegin,
    /// The end of a signpost interval.
    IntervalEnd,
    /// A point-in-time signpost event.
    Event,
}

/// Controls which signposts the reader captures.
///
/// Filtering happens at the `OSActivityStream` level (kernel-side) to avoid
/// flooding the channel with irrelevant entries.
#[derive(Debug, Clone, Default)]
pub struct SignpostFilter {
    /// Process IDs to monitor. Empty = current process only.
    pub pids: Vec<i32>,
    /// Subsystem allowlist. Empty = all subsystems.
    pub subsystems: Vec<String>,
    /// Category allowlist within matched subsystems. Empty = all categories.
    pub categories: Vec<String>,
}

/// Errors from the signpost reader.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The reader is not supported on this platform.
    #[error("os-signpost-reader is only supported on macOS")]
    UnsupportedPlatform,
    /// Failed to initialize the OSActivityStream.
    #[error("failed to initialize OSActivityStream: {0}")]
    StreamInit(String),
}

/// Builder for a signpost reader.
pub struct SignpostReader {
    filter: SignpostFilter,
    channel_capacity: usize,
}

impl SignpostReader {
    /// Create a new reader with the given filter.
    pub fn new(filter: SignpostFilter) -> Self {
        Self {
            filter,
            channel_capacity: 4096,
        }
    }

    /// Set the bounded channel capacity (default: 4096).
    pub fn with_channel_capacity(mut self, cap: usize) -> Self {
        self.channel_capacity = cap;
        self
    }

    /// Start reading system signposts.
    ///
    /// Returns a guard (drop to stop) and a receiver for signpost entries.
    pub fn start(self) -> Result<(SignpostReaderGuard, flume::Receiver<SignpostEntry>), Error> {
        #[cfg(target_os = "macos")]
        {
            crate::ffi::start_stream(self.filter, self.channel_capacity)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = self;
            Err(Error::UnsupportedPlatform)
        }
    }
}

/// RAII guard for a running signpost reader. Stops the stream on drop.
///
/// This type is `!Send` — it must be dropped on the same thread that
/// called [`SignpostReader::start`]. This is because `OSActivityStream.stop()`
/// has thread affinity requirements.
pub struct SignpostReaderGuard {
    #[cfg(target_os = "macos")]
    pub(crate) inner: crate::ffi::StreamGuardInner,
    dropped: std::sync::atomic::AtomicU64,
    // Marker to make this type !Send (PhantomData<*const ()> is !Send)
    _not_send: std::marker::PhantomData<*const ()>,
}

impl SignpostReaderGuard {
    /// Number of entries dropped due to channel backpressure.
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(std::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn new(inner: crate::ffi::StreamGuardInner) -> Self {
        Self {
            inner,
            dropped: std::sync::atomic::AtomicU64::new(0),
            _not_send: std::marker::PhantomData,
        }
    }
}
