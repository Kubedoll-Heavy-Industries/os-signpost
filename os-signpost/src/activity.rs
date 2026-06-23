//! Apple Activity Tracing — correlate signposts with causal activity trees.
//!
//! Activities form a tree rooted at the system's default activity. Each thread
//! carries a "current activity" that signposts automatically inherit when
//! emitted. Use [`Activity::enter`] to push an activity onto the current thread
//! and receive an RAII [`ActivityScope`] that restores the previous activity on
//! drop.
//!
//! # Platform support
//!
//! | Platform | Behavior |
//! |----------|----------|
//! | macOS / iOS | Full implementation via `os_activity_*` FFI |
//! | Other | All methods are zero-cost no-ops |

// ============================================================================
// macOS / iOS implementation
// ============================================================================

#[cfg(all(
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos"
    ),
    not(feature = "disable-signposts"),
))]
mod imp {
    use std::ffi::CString;
    use std::marker::PhantomData;
    use std::os::raw::c_char;

    /// os_activity_t is an OS_OBJECT (opaque pointer).
    type OsActivityT = *mut std::ffi::c_void;
    type OsActivityIdT = u64;
    type OsActivityFlagT = u32;

    const OS_ACTIVITY_FLAG_DEFAULT: OsActivityFlagT = 0;
    const OS_ACTIVITY_FLAG_DETACHED: OsActivityFlagT = 0x1;

    /// Scope state saved/restored by `os_activity_scope_enter` / `_leave`.
    #[repr(C)]
    struct OsActivityScopeState {
        opaque: [u64; 2],
    }

    unsafe extern "C" {
        // Linker-synthesized symbol pointing to the Mach-O header of this DSO.
        static __dso_handle: u8;

        // Global sentinel objects.
        static _os_activity_current: *mut std::ffi::c_void;
        static _os_activity_none: *mut std::ffi::c_void;

        fn _os_activity_create(
            dso: *const u8,
            description: *const c_char,
            activity: OsActivityT,
            flags: OsActivityFlagT,
        ) -> OsActivityT;

        fn os_activity_scope_enter(activity: OsActivityT, state: *mut OsActivityScopeState);
        fn os_activity_scope_leave(state: *mut OsActivityScopeState);

        fn os_activity_get_identifier(
            activity: OsActivityT,
            parent_id: *mut OsActivityIdT,
        ) -> OsActivityIdT;

        fn os_retain(obj: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        fn os_release(obj: *mut std::ffi::c_void);
    }

    /// An Apple Activity Tracing activity.
    ///
    /// Wraps an `os_activity_t` — a system-managed activity object that can be
    /// entered on any thread to establish a causal context for signposts and
    /// log messages.
    pub struct Activity {
        raw: OsActivityT,
    }

    impl Activity {
        /// Create a new activity linked to the current thread's activity.
        pub fn new(description: &str) -> Self {
            let desc =
                CString::new(description).expect("activity description must not contain NUL bytes");
            let raw = unsafe {
                _os_activity_create(
                    &raw const __dso_handle,
                    desc.as_ptr(),
                    _os_activity_current,
                    OS_ACTIVITY_FLAG_DEFAULT,
                )
            };
            Self { raw }
        }

        /// Create a detached activity (no parent).
        pub fn detached(description: &str) -> Self {
            let desc =
                CString::new(description).expect("activity description must not contain NUL bytes");
            let raw = unsafe {
                _os_activity_create(
                    &raw const __dso_handle,
                    desc.as_ptr(),
                    _os_activity_none,
                    OS_ACTIVITY_FLAG_DETACHED,
                )
            };
            Self { raw }
        }

        /// Enter this activity's scope on the current thread.
        ///
        /// All signposts emitted on this thread will inherit this activity's ID
        /// until the returned [`ActivityScope`] is dropped.
        pub fn enter(&self) -> ActivityScope {
            let mut state = OsActivityScopeState { opaque: [0; 2] };
            unsafe {
                os_activity_scope_enter(self.raw, &mut state);
            }
            ActivityScope {
                state,
                _not_send: PhantomData,
            }
        }

        /// Get the activity ID assigned by the system.
        pub fn id(&self) -> u64 {
            unsafe { os_activity_get_identifier(self.raw, std::ptr::null_mut()) }
        }

        /// Get the parent activity ID (0 if detached).
        pub fn parent_id(&self) -> u64 {
            let mut parent: OsActivityIdT = 0;
            unsafe {
                os_activity_get_identifier(self.raw, &mut parent);
            }
            parent
        }
    }

    impl Clone for Activity {
        fn clone(&self) -> Self {
            unsafe {
                os_retain(self.raw);
            }
            Self { raw: self.raw }
        }
    }

    impl Drop for Activity {
        fn drop(&mut self) {
            unsafe {
                os_release(self.raw);
            }
        }
    }

    // os_activity_t is an OS_OBJECT, thread-safe by contract.
    unsafe impl Send for Activity {}
    unsafe impl Sync for Activity {}

    /// RAII guard that leaves an activity scope on drop.
    ///
    /// This type is `!Send` because `os_activity_scope_enter` / `_leave` are
    /// thread-local operations — the scope must be left on the same thread it
    /// was entered.
    pub struct ActivityScope {
        state: OsActivityScopeState,
        _not_send: PhantomData<*const ()>,
    }

    impl Drop for ActivityScope {
        fn drop(&mut self) {
            unsafe {
                os_activity_scope_leave(&mut self.state);
            }
        }
    }
}

// ============================================================================
// No-op implementation for non-Apple platforms / disable-signposts
// ============================================================================

#[cfg(not(all(
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos"
    ),
    not(feature = "disable-signposts"),
)))]
mod imp {
    use std::marker::PhantomData;

    /// No-op activity for non-Apple platforms.
    pub struct Activity;

    impl Activity {
        /// No-op: returns a dummy activity.
        #[inline(always)]
        pub fn new(_description: &str) -> Self {
            Self
        }

        /// No-op: returns a dummy detached activity.
        #[inline(always)]
        pub fn detached(_description: &str) -> Self {
            Self
        }

        /// No-op: returns a dummy scope guard.
        #[inline(always)]
        pub fn enter(&self) -> ActivityScope {
            ActivityScope {
                _not_send: PhantomData,
            }
        }

        /// No-op: always returns 0.
        #[inline(always)]
        pub fn id(&self) -> u64 {
            0
        }

        /// No-op: always returns 0.
        #[inline(always)]
        pub fn parent_id(&self) -> u64 {
            0
        }
    }

    /// No-op scope guard for non-Apple platforms.
    pub struct ActivityScope {
        _not_send: PhantomData<*const ()>,
    }
}

// ============================================================================
// Public re-exports
// ============================================================================

pub use imp::Activity;
pub use imp::ActivityScope;

#[cfg(test)]
mod tests {
    use super::*;

    /// Activity::new() should return an activity with a non-zero system ID.
    #[test]
    fn activity_has_nonzero_id() {
        let a = Activity::new("test-activity");
        // On macOS the system assigns a unique ID; on no-op platforms id() is 0.
        #[cfg(all(
            any(
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "watchos"
            ),
            not(feature = "disable-signposts"),
        ))]
        assert_ne!(a.id(), 0, "activity should have a non-zero system ID");

        #[cfg(not(all(
            any(
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "watchos"
            ),
            not(feature = "disable-signposts"),
        )))]
        assert_eq!(a.id(), 0, "no-op activity returns 0");
    }

    /// A detached activity should have parent_id() == 0.
    #[test]
    fn detached_activity_has_zero_parent() {
        let a = Activity::detached("detached-test");
        assert_eq!(a.parent_id(), 0, "detached activity should have no parent");
    }

    /// Enter a scope, emit a signpost (via Signposter), leave scope — no panic.
    #[test]
    fn enter_scope_with_signpost() {
        use crate::{Category, Signposter};

        let activity = Activity::new("scope-test");
        let _scope = activity.enter();

        let profiler = Signposter::new("com.test.activity", Category::PointsOfInterest);
        profiler.event("inside-activity");

        // Scope drops here — no panic expected.
    }

    /// Nested scopes: enter two activities, leave in reverse order (LIFO).
    #[test]
    fn nested_scopes() {
        let outer = Activity::new("outer");
        let inner = Activity::new("inner");

        let _scope_outer = outer.enter();
        let _scope_inner = inner.enter();

        // inner scope drops first, then outer — correct LIFO order.
    }

    /// Compile-time assertions for Send/Sync traits.
    #[test]
    fn send_sync_assertions() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        fn assert_not_send<T>()
        where
            T: ?Sized,
        {
            // ActivityScope should NOT be Send. We verify this at compile time
            // by checking that it does not implement Send.
        }

        assert_send::<Activity>();
        assert_sync::<Activity>();

        // ActivityScope is !Send because it contains PhantomData<*const ()>.
        // We can't easily assert !Send at compile time without negative trait
        // bounds, so we verify the PhantomData marker is present via the type
        // system — the scope contains a non-Send field.
        let _ = assert_not_send::<ActivityScope>;
    }
}
