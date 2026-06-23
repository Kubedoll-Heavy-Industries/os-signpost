//! objc2 bindings for OSActivityStream.
//!
//! OSActivityStream is the main entry point for reading system signposts.
//! It uses a delegate pattern to deliver events on Apple's internal thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::SystemTime;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, NSObject, NSObjectProtocol};
use objc2::{
    AnyThread, ClassType, DefinedClass, define_class, extern_class, extern_conformance,
    extern_methods, extern_protocol, msg_send,
};
use objc2_foundation::{NSDate, NSError};

use super::events::{OSActivityEvent, OSActivitySignpostEvent};
use crate::reader::{Error, SignpostEntry, SignpostFilter, SignpostReaderGuard, SignpostType};

// ---------------------------------------------------------------------------
// OSActivityStream
// ---------------------------------------------------------------------------

extern_class!(
    /// The main stream class from LoggingSupport.framework.
    #[unsafe(super(NSObject))]
    pub(crate) struct OSActivityStream;
);

extern_conformance!(
    unsafe impl NSObjectProtocol for OSActivityStream {}
);

impl OSActivityStream {
    extern_methods!(
        #[unsafe(method(init))]
        #[unsafe(method_family = init)]
        pub fn init(this: objc2::rc::Allocated<Self>) -> Retained<Self>;

        #[unsafe(method(setEvents:))]
        pub fn set_events(&self, events: u64);

        #[unsafe(method(addProcessID:))]
        pub fn add_process_id(&self, pid: i32);

        #[unsafe(method(start))]
        pub fn start(&self);

        #[unsafe(method(stop))]
        pub fn stop(&self);

        #[unsafe(method(setDelegate:))]
        pub fn set_delegate(&self, delegate: &AnyObject);
    );
}

// ---------------------------------------------------------------------------
// OSActivityStreamDelegate protocol
// ---------------------------------------------------------------------------

extern_protocol!(
    /// The delegate protocol for OSActivityStream.
    ///
    /// # Safety
    ///
    /// Implementors must correctly handle `streamEvent:error:` callbacks from
    /// Apple's internal dispatch queue thread. The callback must not block.
    #[name = "OSActivityStreamDelegate"]
    pub(crate) unsafe trait OSActivityStreamDelegate: NSObjectProtocol {
        /// Called for each event delivered by the stream.
        ///
        /// Return true to continue receiving events, false to stop.
        #[unsafe(method(streamEvent:error:))]
        #[optional]
        unsafe fn stream_event(&self, event: &OSActivityEvent, error: Option<&NSError>) -> Bool;
    }
);

// ---------------------------------------------------------------------------
// Delegate implementation
// ---------------------------------------------------------------------------

/// Shared state between the delegate callback and the reader guard.
///
/// All fields are `Send + Sync`:
/// - `sender`: `flume::Sender` is `Send + Sync`
/// - `dropped`: `Arc<AtomicU64>` is `Send + Sync`
/// - `cancelled`: `Arc<AtomicBool>` is `Send + Sync`
/// - `subsystems`/`categories`: `Vec<String>` is `Send + Sync` (immutable after init)
struct DelegateIvars {
    sender: flume::Sender<SignpostEntry>,
    dropped: Arc<AtomicU64>,
    cancelled: Arc<AtomicBool>,
    /// Subsystem allowlist for filtering. Empty = accept all.
    subsystems: Vec<String>,
    /// Category allowlist for filtering. Empty = accept all.
    categories: Vec<String>,
}

define_class!(
    // SAFETY:
    // - NSObject has no subclassing requirements.
    // - StreamDelegate does not implement Drop.
    // - The delegate callback (`streamEvent:error:`) is called by Apple on an
    //   internal dispatch queue thread. Shared access to `DelegateIvars` is
    //   sound because:
    //   - `sender` (flume::Sender) is Send+Sync; `try_send` is safe from any
    //     thread.
    //   - `dropped` and `cancelled` are accessed through atomics with
    //     Acquire/Release ordering, providing the necessary synchronization
    //     between the callback thread and the thread that sets cancellation.
    //   - `subsystems` and `categories` are immutable after construction —
    //     they are written once in `StreamDelegate::new` before any callback
    //     can fire (the stream hasn't started yet), and only read thereafter.
    #[unsafe(super(NSObject))]
    #[ivars = DelegateIvars]
    #[name = "OSSignpostReaderDelegate"]
    struct StreamDelegate;

    // SAFETY: The method signatures match the OSActivityStreamDelegate protocol.
    impl StreamDelegate {
        #[unsafe(method(streamEvent:error:))]
        fn stream_event(&self, event: &OSActivityEvent, _error: Option<&NSError>) -> Bool {
            // Check cancellation before doing any work.
            // Acquire pairs with the Release store in StreamGuardInner::drop.
            if self.ivars().cancelled.load(Ordering::Acquire) {
                return Bool::NO;
            }

            // Try to downcast to signpost event. We check the class at runtime.
            let event_ptr: *const OSActivityEvent = event;
            let obj: &AnyObject = unsafe { &*(event_ptr as *const AnyObject) };

            let is_signpost: bool = unsafe {
                let signpost_class = OSActivitySignpostEvent::class();
                msg_send![obj, isKindOfClass: signpost_class]
            };

            if !is_signpost {
                return Bool::YES;
            }

            // SAFETY: We verified the class above.
            let signpost: &OSActivitySignpostEvent =
                unsafe { &*(event_ptr as *const OSActivitySignpostEvent) };

            // Apply subsystem/category filters in the callback.
            let subsystem = signpost
                .subsystem()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let category = signpost
                .category()
                .map(|s| s.to_string())
                .unwrap_or_default();

            let ivars = self.ivars();
            if !ivars.subsystems.is_empty() && !ivars.subsystems.contains(&subsystem) {
                return Bool::YES;
            }
            if !ivars.categories.is_empty() && !ivars.categories.contains(&category) {
                return Bool::YES;
            }

            // Convert NSDate timestamp to SystemTime.
            let timestamp = signpost
                .timestamp()
                .map(|date| nsdate_to_system_time(&date))
                .unwrap_or(SystemTime::UNIX_EPOCH);

            // Determine signpost type from eventType.
            // eventType values from Apple's private headers:
            //   0x0200 = signpost event
            //   0x0201 = signpost interval begin
            //   0x0202 = signpost interval end
            let event_type_raw = signpost.event_type();
            let signpost_type = match event_type_raw & 0xFF {
                1 => SignpostType::IntervalBegin,
                2 => SignpostType::IntervalEnd,
                _ => SignpostType::Event,
            };

            // Read event_message once and reuse for both signpost_name fallback
            // and message field.
            let event_message = signpost.event_message().map(|s| s.to_string());

            // signpost_name: prefer the dedicated signpostName property (from
            // OSLogEntrySignpost). Fall back to event_message if the selector
            // doesn't exist at runtime or returns nil, then to the format string.
            let signpost_name = signpost
                .signpost_name()
                .map(|s| s.to_string())
                .or_else(|| event_message.clone())
                .or_else(|| signpost.format().map(|s| s.to_string()))
                .unwrap_or_default();

            let entry = SignpostEntry {
                subsystem,
                category,
                signpost_name,
                signpost_id: signpost.signpost_id(),
                signpost_type,
                timestamp,
                mach_timestamp: signpost.mach_timestamp(),
                process_id: signpost.process_id(),
                process_name: signpost
                    .process_image_path()
                    .map(|s| {
                        let path = s.to_string();
                        path.rsplit('/').next().unwrap_or(&path).to_string()
                    })
                    .unwrap_or_default(),
                thread_id: signpost.thread_id(),
                sender_image_path: signpost
                    .sender_image_path()
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                message: event_message,
                trace_id: signpost.trace_id(),
                activity_id: signpost.activity_id(),
                parent_activity_id: signpost.parent_activity_id(),
            };

            // Non-blocking send — drop on full channel.
            match ivars.sender.try_send(entry) {
                Ok(()) => {}
                Err(flume::TrySendError::Full(_)) => {
                    ivars.dropped.fetch_add(1, Ordering::Relaxed);
                }
                Err(flume::TrySendError::Disconnected(_)) => {
                    // Receiver dropped — stop the stream.
                    return Bool::NO;
                }
            }

            Bool::YES
        }
    }
);

extern_conformance!(
    // SAFETY: StreamDelegate implements NSObjectProtocol via NSObject.
    unsafe impl NSObjectProtocol for StreamDelegate {}
);

extern_conformance!(
    // SAFETY: StreamDelegate implements the delegate method.
    unsafe impl OSActivityStreamDelegate for StreamDelegate {}
);

impl StreamDelegate {
    fn new(
        sender: flume::Sender<SignpostEntry>,
        dropped: Arc<AtomicU64>,
        cancelled: Arc<AtomicBool>,
        subsystems: Vec<String>,
        categories: Vec<String>,
    ) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars {
            sender,
            dropped,
            cancelled,
            subsystems,
            categories,
        });
        unsafe { msg_send![super(this), init] }
    }
}

// ---------------------------------------------------------------------------
// NSDate → SystemTime conversion
// ---------------------------------------------------------------------------

/// Convert an `NSDate` to `SystemTime`. Must never panic — this runs inside
/// an ObjC delegate callback where unwinding is undefined behavior.
fn nsdate_to_system_time(date: &NSDate) -> SystemTime {
    // NSDate.timeIntervalSince1970 returns seconds since Unix epoch.
    let interval: f64 = unsafe { msg_send![date, timeIntervalSince1970] };

    // Guard against NaN, infinity, or other non-finite values that would
    // panic in Duration::try_from_secs_f64.
    if !interval.is_finite() {
        return SystemTime::UNIX_EPOCH;
    }

    if interval >= 0.0 {
        match std::time::Duration::try_from_secs_f64(interval) {
            Ok(dur) => SystemTime::UNIX_EPOCH
                .checked_add(dur)
                .unwrap_or(SystemTime::UNIX_EPOCH),
            Err(_) => SystemTime::UNIX_EPOCH,
        }
    } else {
        match std::time::Duration::try_from_secs_f64(-interval) {
            Ok(dur) => SystemTime::UNIX_EPOCH
                .checked_sub(dur)
                .unwrap_or(SystemTime::UNIX_EPOCH),
            Err(_) => SystemTime::UNIX_EPOCH,
        }
    }
}

// ---------------------------------------------------------------------------
// Stream event type flags
// ---------------------------------------------------------------------------

/// We want signpost events. The exact flag combination that includes signposts:
/// From Apple's private headers:
///   OS_ACTIVITY_STREAM_LOG        = 0x02
///   OS_ACTIVITY_STREAM_ACTIVITY   = 0x04
/// The stream must have at least the log flag to receive signpost events.
const STREAM_FLAGS_ALL_SIGNPOSTS: u64 = 0x0002 | 0x0004;

// ---------------------------------------------------------------------------
// StreamGuardInner + start_stream
// ---------------------------------------------------------------------------

/// Internal state for a running stream, held by `SignpostReaderGuard`.
pub(crate) struct StreamGuardInner {
    stream: Retained<OSActivityStream>,
    // Keep the delegate alive as long as the stream is running.
    _delegate: Retained<StreamDelegate>,
    cancelled: Arc<AtomicBool>,
    /// Shared dropped counter — the same Arc is held by the delegate's ivars.
    pub(crate) dropped: Arc<AtomicU64>,
}

impl Drop for StreamGuardInner {
    fn drop(&mut self) {
        // Signal the delegate to stop processing events.
        // Release pairs with the Acquire load in the delegate callback.
        self.cancelled.store(true, Ordering::Release);
        self.stream.stop();
    }
}

/// Create and start an `OSActivityStream`, returning a guard and receiver.
pub(crate) fn start_stream(
    filter: SignpostFilter,
    channel_capacity: usize,
) -> Result<(SignpostReaderGuard, flume::Receiver<SignpostEntry>), Error> {
    let (tx, rx) = flume::bounded(channel_capacity);

    let dropped = Arc::new(AtomicU64::new(0));
    let cancelled = Arc::new(AtomicBool::new(false));

    // Create the stream.
    let stream = OSActivityStream::init(OSActivityStream::alloc());

    // Configure event types.
    stream.set_events(STREAM_FLAGS_ALL_SIGNPOSTS);

    // Add process filters.
    if filter.pids.is_empty() {
        // Default: current process only.
        let pid = std::process::id() as i32;
        stream.add_process_id(pid);
    } else {
        for pid in &filter.pids {
            stream.add_process_id(*pid);
        }
    }

    // Subsystem/category filtering happens in the delegate callback (Rust-side)
    // rather than via setPredicate: — the OSActivityStream predicate API uses
    // internal key paths that differ from the property names and can crash when
    // called with NSPredicate format strings.

    // Create and set the delegate.
    let delegate = StreamDelegate::new(
        tx,
        dropped.clone(),
        cancelled.clone(),
        filter.subsystems,
        filter.categories,
    );
    let delegate_obj: &AnyObject = unsafe {
        let ptr: *const StreamDelegate = &*delegate;
        &*(ptr as *const AnyObject)
    };
    stream.set_delegate(delegate_obj);

    // Start the stream.
    stream.start();

    let guard_inner = StreamGuardInner {
        stream,
        _delegate: delegate,
        cancelled,
        dropped,
    };

    let guard = SignpostReaderGuard::new(guard_inner);

    Ok((guard, rx))
}
