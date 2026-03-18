//! Rust bindings to Apple's `os_signpost` API for performance instrumentation.
//!
//! Signposts appear as intervals and events in Apple Instruments, enabling
//! fine-grained profiling of your application's hot paths.
//!
//! # Platform support
//!
//! | Platform | Behavior |
//! |----------|----------|
//! | macOS / iOS | Full signpost emission via direct FFI |
//! | Other | All methods are zero-cost no-ops |
//!
//! The `disable-signposts` feature forces no-op behavior on all platforms.
//!
//! # Example
//!
//! ```rust
//! use os_signpost::{Signposter, Category};
//!
//! let profiler = Signposter::new("com.example.myapp", Category::PointsOfInterest);
//!
//! // Scoped interval — ends when `_interval` drops
//! let _interval = profiler.begin_interval("compute");
//!
//! // Point-in-time event
//! profiler.event("checkpoint");
//! ```

#![cfg_attr(docsrs, feature(doc_auto_cfg))]

// Backend selection: Apple platforms with signposts enabled get the real
// implementation; everything else gets zero-cost no-ops.
#[cfg(all(
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos"
    ),
    not(feature = "disable-signposts"),
))]
mod backend;

#[cfg(not(all(
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos"
    ),
    not(feature = "disable-signposts"),
)))]
mod noop;

#[cfg(all(
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos"
    ),
    not(feature = "disable-signposts"),
))]
use backend as imp;

#[cfg(not(all(
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos"
    ),
    not(feature = "disable-signposts"),
)))]
use noop as imp;

#[cfg(feature = "tracing")]
pub mod layer;

use std::fmt;

/// A signpost logger bound to a subsystem and category.
///
/// Create one per subsystem (typically a reverse-DNS identifier) and reuse it
/// for all signpost operations in that subsystem. Thread-safe via interior
/// synchronization.
///
/// # Example
///
/// ```rust
/// use os_signpost::{Signposter, Category};
/// use std::sync::LazyLock;
///
/// static PROFILER: LazyLock<Signposter> = LazyLock::new(|| {
///     Signposter::new("ai.mistralrs", Category::DynamicTracing)
/// });
/// ```
pub struct Signposter {
    inner: imp::SignposterInner,
}

/// RAII guard for a signpost interval. Ends the interval on drop.
///
/// Call [`SignpostInterval::end_with_message`] to attach a message at the end,
/// or simply let it drop for an unadorned interval end.
pub struct SignpostInterval {
    inner: imp::SignpostIntervalInner,
}

/// Signpost log category, controlling visibility in Instruments.
///
/// See [Apple documentation](https://developer.apple.com/documentation/os/os_log_category_points_of_interest)
/// for details on how categories affect trace capture.
#[derive(Debug, Clone)]
pub enum Category {
    /// Appears by default in the "Points of Interest" instrument.
    PointsOfInterest,
    /// Recorded only when the app runs under Instruments. Low overhead when
    /// Instruments is not attached.
    DynamicTracing,
    /// Like [`DynamicTracing`](Category::DynamicTracing), but also captures
    /// stack traces at each signpost.
    DynamicStackTracing,
    /// A custom category string.
    Custom(String),
}

impl Signposter {
    /// Create a new signposter for the given subsystem and category.
    ///
    /// `subsystem` should be a reverse-DNS identifier (e.g. `"com.example.myapp"`).
    pub fn new(subsystem: &str, category: Category) -> Self {
        Self {
            inner: imp::SignposterInner::new(subsystem, category),
        }
    }

    /// Convenience constructor for `Category::PointsOfInterest`.
    pub fn points_of_interest(subsystem: &str) -> Self {
        Self::new(subsystem, Category::PointsOfInterest)
    }

    /// Emit a point-in-time event visible in Instruments.
    pub fn event(&self, name: &str) {
        self.inner.event(name, None::<&str>);
    }

    /// Emit a point-in-time event with an attached message.
    pub fn event_with_message(&self, name: &str, msg: impl fmt::Display) {
        self.inner.event(name, Some(msg));
    }

    /// Begin a signpost interval. The interval ends when the returned
    /// [`SignpostInterval`] is dropped.
    pub fn begin_interval(&self, name: &str) -> SignpostInterval {
        SignpostInterval {
            inner: self.inner.begin_interval(name, None::<&str>),
        }
    }

    /// Begin a signpost interval with an attached message at the start.
    pub fn begin_interval_with_message(
        &self,
        name: &str,
        msg: impl fmt::Display,
    ) -> SignpostInterval {
        SignpostInterval {
            inner: self.inner.begin_interval(name, Some(msg)),
        }
    }

    /// Returns `true` if signpost emission is currently enabled for this logger.
    ///
    /// Use this to skip expensive message construction when Instruments is not
    /// attached:
    ///
    /// ```rust
    /// # use os_signpost::{Signposter, Category};
    /// # let profiler = Signposter::new("com.example", Category::DynamicTracing);
    /// if profiler.enabled() {
    ///     let msg = format!("processed {} items", 42);
    ///     profiler.event_with_message("batch", msg);
    /// }
    /// ```
    pub fn enabled(&self) -> bool {
        self.inner.enabled()
    }
}

impl SignpostInterval {
    /// End the interval with a message. Consumes `self`.
    ///
    /// If you don't need a message, just let the interval drop.
    pub fn end_with_message(self, msg: impl fmt::Display) {
        self.inner.end_with_message(msg);
    }
}

// Safety: The underlying os_log_t handle is thread-safe for emission.
// Apple documents os_log_t as safe to use from multiple threads.
unsafe impl Send for Signposter {}
unsafe impl Sync for Signposter {}

// SignpostInterval is Send (can be moved to another thread to end there)
// but intentionally NOT Sync — concurrent Drop + end_with_message would
// produce duplicate interval-end signposts.
unsafe impl Send for SignpostInterval {}
