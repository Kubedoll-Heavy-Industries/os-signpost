//! High-level OTel exporter for macOS system signposts.
//!
//! [`SignpostOtelExporter`] is the primary entry point for most users.
//! It combines the reader, channel, and OTel bridge into a single builder.

use crate::otel_bridge::SignpostOtelBridge;
use crate::reader::{Error, SignpostFilter, SignpostReader};
use crate::subsystem::Subsystem;

/// Reads macOS system signposts and exports them as OpenTelemetry spans
/// via the global tracer.
///
/// # Example
///
/// ```rust,no_run
/// use os_signpost_reader::{SignpostOtelExporter, Subsystem};
///
/// # async fn example() -> Result<(), os_signpost_reader::Error> {
/// // Application has already called global::set_tracer_provider(...)
///
/// let _guard = SignpostOtelExporter::builder()
///     .subsystem(Subsystem::METAL)
///     .subsystem(Subsystem::CORE_ANIMATION)
///     .channel_capacity(8192)
///     .start()?;
/// # Ok(())
/// # }
/// ```
pub struct SignpostOtelExporter;

impl SignpostOtelExporter {
    /// Create a new builder.
    pub fn builder() -> SignpostOtelExporterBuilder {
        SignpostOtelExporterBuilder {
            subsystems: Vec::new(),
            categories: Vec::new(),
            pids: Vec::new(),
            channel_capacity: 4096,
        }
    }
}

/// Builder for [`SignpostOtelExporter`].
pub struct SignpostOtelExporterBuilder {
    subsystems: Vec<String>,
    categories: Vec<String>,
    pids: Vec<i32>,
    channel_capacity: usize,
}

impl SignpostOtelExporterBuilder {
    /// Add a subsystem to capture signposts from.
    ///
    /// Can be called multiple times. If no subsystems are added, all
    /// subsystems are captured (high volume — consider filtering).
    ///
    /// ```rust,no_run
    /// # use os_signpost_reader::{SignpostOtelExporter, Subsystem};
    /// SignpostOtelExporter::builder()
    ///     .subsystem(Subsystem::METAL)
    ///     .subsystem(Subsystem::CORE_ANIMATION)
    ///     .subsystem(Subsystem::custom("ai.mistralrs"))
    /// # ;
    /// ```
    pub fn subsystem(mut self, subsystem: Subsystem) -> Self {
        self.subsystems.push(subsystem.into_string());
        self
    }

    /// Add a category filter within matched subsystems.
    ///
    /// If no categories are added, all categories are captured.
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.categories.push(category.into());
        self
    }

    /// Add a process ID to monitor. Can be called multiple times.
    ///
    /// If no PIDs are added, only the current process is monitored.
    pub fn pid(mut self, pid: i32) -> Self {
        self.pids.push(pid);
        self
    }

    /// Set the internal channel capacity (default: 4096).
    ///
    /// If the OTel export can't keep up, entries are dropped and counted
    /// via [`SignpostOtelExporterGuard::dropped_count`].
    pub fn channel_capacity(mut self, capacity: usize) -> Self {
        self.channel_capacity = capacity;
        self
    }

    /// Start reading signposts and exporting to the global OTel tracer.
    ///
    /// Returns a guard — drop it to stop. Spawns a background tokio task
    /// for the export loop.
    pub fn start(self) -> Result<SignpostOtelExporterGuard, Error> {
        let filter = SignpostFilter {
            pids: self.pids,
            subsystems: self.subsystems,
            categories: self.categories,
        };

        let (reader_guard, rx) = SignpostReader::new(filter)
            .with_channel_capacity(self.channel_capacity)
            .start()?;

        let bridge = SignpostOtelBridge::new(rx);

        // Spawn the bridge on the tokio runtime.
        tokio::spawn(bridge.run());

        Ok(SignpostOtelExporterGuard {
            _reader: reader_guard,
        })
    }
}

/// Guard for a running signpost exporter. Drop to stop.
///
/// This is `!Send` because the underlying reader guard has thread affinity.
pub struct SignpostOtelExporterGuard {
    _reader: crate::reader::SignpostReaderGuard,
}

impl SignpostOtelExporterGuard {
    /// Number of signpost entries dropped due to backpressure.
    pub fn dropped_count(&self) -> u64 {
        self._reader.dropped_count()
    }
}
