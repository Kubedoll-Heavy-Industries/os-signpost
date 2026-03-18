//! In-process reader for macOS system signposts.
//!
//! Captures signpost events emitted by any framework (Metal, Core Animation,
//! IOSurface, your own app) via Apple's `OSActivityStream` and exposes them
//! as a typed [`SignpostEntry`] stream over a [`flume`] channel.
//!
//! # Platform support
//!
//! macOS only. On other platforms, [`SignpostReader::start`] returns
//! [`Error::UnsupportedPlatform`].
//!
//! # Feature flags
//!
//! - **`tracing`** — [`SignpostTracingBridge`](tracing_bridge::SignpostTracingBridge):
//!   drains the channel and emits `tracing::Span` / `tracing::Event`.
//! - **`opentelemetry`** — [`SignpostOtelBridge`](otel_bridge::SignpostOtelBridge):
//!   drains the channel and emits OTel spans via the global tracer.
//!   Application owns the `TracerProvider` and exporter configuration.

#[cfg(target_os = "macos")]
mod ffi;
mod reader;

#[cfg(feature = "tracing")]
pub mod tracing_bridge;

#[cfg(feature = "opentelemetry")]
pub mod otel_bridge;

pub use reader::{
    Error, SignpostEntry, SignpostFilter, SignpostReader, SignpostReaderGuard, SignpostType,
};
