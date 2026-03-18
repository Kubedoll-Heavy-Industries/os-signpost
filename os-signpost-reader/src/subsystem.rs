//! Well-known Apple framework subsystem constants.

use std::borrow::Cow;

/// A macOS subsystem identifier for signpost filtering.
///
/// Use the provided constants for well-known Apple frameworks, or
/// construct a custom subsystem with [`Subsystem::custom`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Subsystem(Cow<'static, str>);

impl Subsystem {
    // ── Graphics & GPU ──────────────────────────────────────────────

    /// Metal GPU framework — shader compilation, command buffer execution,
    /// resource allocation.
    pub const METAL: Self = Self(Cow::Borrowed("com.apple.Metal"));

    /// Core Animation / QuartzCore — layer compositing, frame commits,
    /// animation timing.
    pub const CORE_ANIMATION: Self = Self(Cow::Borrowed("com.apple.CoreAnimation"));

    /// GPU driver-level events.
    pub const GPU: Self = Self(Cow::Borrowed("com.apple.gpu"));

    /// IOSurface — pixel buffer allocation and sharing between processes.
    pub const IO_SURFACE: Self = Self(Cow::Borrowed("com.apple.IOSurface"));

    /// SkyLight — the window server rendering engine. One of the most active
    /// signpost emitters on macOS.
    pub const SKYLIGHT: Self = Self(Cow::Borrowed("com.apple.SkyLight"));

    // ── System ──────────────────────────────────────────────────────

    /// Grand Central Dispatch — queue scheduling, block execution.
    pub const DISPATCH: Self = Self(Cow::Borrowed("com.apple.Dispatch"));

    /// Window server — display compositor and input event routing.
    pub const WINDOW_SERVER: Self = Self(Cow::Borrowed("com.apple.windowserver"));

    // ── Construction ────────────────────────────────────────────────

    /// Create a subsystem from a custom reverse-DNS identifier.
    ///
    /// ```
    /// use os_signpost_reader::Subsystem;
    /// let custom = Subsystem::custom("ai.mistralrs.inference");
    /// ```
    pub fn custom(subsystem: impl Into<String>) -> Self {
        Self(Cow::Owned(subsystem.into()))
    }

    /// Get the subsystem string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert to owned String for use in filters.
    pub(crate) fn into_string(self) -> String {
        self.0.into_owned()
    }
}

impl std::fmt::Display for Subsystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Subsystem {
    fn from(s: &str) -> Self {
        Self::custom(s)
    }
}

impl From<String> for Subsystem {
    fn from(s: String) -> Self {
        Self(Cow::Owned(s))
    }
}
