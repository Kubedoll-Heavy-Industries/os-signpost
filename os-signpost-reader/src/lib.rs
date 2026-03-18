//! In-process reader for macOS system signposts.
//!
//! Captures signpost events emitted by any framework (Metal, Core Animation,
//! IOSurface, your own app) via Apple's `OSActivityStream` and exposes them
//! as typed [`SignpostEntry`] values.
//!
//! # Quick start (OpenTelemetry)
//!
//! ```rust,no_run
//! use os_signpost_reader::{SignpostOtelExporter, Subsystem};
//!
//! # async fn example() -> Result<(), os_signpost_reader::Error> {
//! // Application has already called global::set_tracer_provider(...)
//!
//! let _guard = SignpostOtelExporter::builder()
//!     .subsystem(Subsystem::METAL)
//!     .subsystem(Subsystem::CORE_ANIMATION)
//!     .start()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Platform support
//!
//! macOS only. On other platforms, [`start()`](SignpostOtelExporter::start)
//! returns [`Error::UnsupportedPlatform`].
//!
//! # Feature flags
//!
//! - **`opentelemetry`** — [`SignpostOtelExporter`]: reads system signposts and
//!   emits OTel spans via the global tracer. Application owns the
//!   `TracerProvider` and exporter configuration.
//! - **`tracing`** — [`tracing_bridge::SignpostTracingBridge`]: lower-level
//!   bridge that emits `tracing::Span` / `tracing::Event` from a raw channel.
//!
//! # Advanced usage
//!
//! For custom consumers, use [`SignpostReader`] directly to get a
//! `flume::Receiver<SignpostEntry>` and process entries however you want.

#[cfg(target_os = "macos")]
mod ffi;
mod reader;
mod subsystem;

#[cfg(feature = "tracing")]
pub mod tracing_bridge;

#[cfg(feature = "opentelemetry")]
mod otel_bridge;

#[cfg(feature = "opentelemetry")]
mod otel_exporter;

pub use reader::{
    Error, SignpostEntry, SignpostFilter, SignpostReader, SignpostReaderGuard, SignpostType,
};
pub use subsystem::Subsystem;

#[cfg(feature = "opentelemetry")]
pub use otel_exporter::SignpostOtelExporter;
