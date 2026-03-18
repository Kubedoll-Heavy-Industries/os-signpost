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
//! - **`otlp`** — [`SignpostOtlpBridge`](otlp_bridge::SignpostOtlpBridge):
//!   drains the channel and exports OTel spans directly via OTLP.

#[cfg(target_os = "macos")]
mod ffi;
mod reader;

#[cfg(feature = "tracing")]
pub mod tracing_bridge;

#[cfg(feature = "otlp")]
pub mod otlp_bridge;

pub use reader::{
    Error, SignpostEntry, SignpostFilter, SignpostReader, SignpostReaderGuard, SignpostType,
};
