//! objc2 class bindings for the OSActivityEvent hierarchy.
//!
//! Class hierarchy (from runtime introspection):
//!   NSObject
//!   └── OSActivityEvent
//!       └── OSActivityEventMessage
//!           └── OSActivityLogMessageEvent
//!               └── OSActivitySignpostEvent
//!
//! These are private classes from LoggingSupport.framework.

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{extern_class, extern_methods};
use objc2_foundation::{NSDate, NSString};

// Link against the private LoggingSupport framework.
#[link(name = "LoggingSupport", kind = "framework")]
unsafe extern "C" {}

// ---------------------------------------------------------------------------
// OSActivityEvent
// ---------------------------------------------------------------------------

extern_class!(
    /// Base event from the activity stream.
    ///
    /// Properties (from runtime introspection):
    /// - timestamp: NSDate (R,C)
    /// - processID: i32 (R)
    /// - processUniqueID: u64 (R)
    /// - threadID: u64 (R)
    /// - traceID: u64 (R)
    /// - activityID: u64 (R)
    /// - parentActivityID: u64 (R)
    /// - machTimestamp: u64 (R)
    /// - eventMessage: NSString (C)
    /// - senderImagePath: NSString (R,C)
    /// - processImagePath: NSString (R,C)
    /// - eventType: u64 (R)
    #[unsafe(super(NSObject))]
    pub(crate) struct OSActivityEvent;
);

impl OSActivityEvent {
    extern_methods!(
        #[unsafe(method(timestamp))]
        pub fn timestamp(&self) -> Option<Retained<NSDate>>;

        #[unsafe(method(processID))]
        pub fn process_id(&self) -> i32;

        #[unsafe(method(processUniqueID))]
        pub fn process_unique_id(&self) -> u64;

        #[unsafe(method(threadID))]
        pub fn thread_id(&self) -> u64;

        #[unsafe(method(traceID))]
        pub fn trace_id(&self) -> u64;

        #[unsafe(method(activityID))]
        pub fn activity_id(&self) -> u64;

        #[unsafe(method(parentActivityID))]
        pub fn parent_activity_id(&self) -> u64;

        #[unsafe(method(machTimestamp))]
        pub fn mach_timestamp(&self) -> u64;

        #[unsafe(method(eventMessage))]
        pub fn event_message(&self) -> Option<Retained<NSString>>;

        #[unsafe(method(senderImagePath))]
        pub fn sender_image_path(&self) -> Option<Retained<NSString>>;

        #[unsafe(method(processImagePath))]
        pub fn process_image_path(&self) -> Option<Retained<NSString>>;

        #[unsafe(method(eventType))]
        pub fn event_type(&self) -> u64;
    );
}

// ---------------------------------------------------------------------------
// OSActivityEventMessage
// ---------------------------------------------------------------------------

extern_class!(
    /// An event with a format string message.
    #[unsafe(super(OSActivityEvent, NSObject))]
    pub(crate) struct OSActivityEventMessage;
);

impl OSActivityEventMessage {
    extern_methods!(
        #[unsafe(method(format))]
        pub fn format(&self) -> Option<Retained<NSString>>;
    );
}

// ---------------------------------------------------------------------------
// OSActivityLogMessageEvent
// ---------------------------------------------------------------------------

extern_class!(
    /// A log message event with subsystem and category.
    #[unsafe(super(OSActivityEventMessage, OSActivityEvent, NSObject))]
    pub(crate) struct OSActivityLogMessageEvent;
);

impl OSActivityLogMessageEvent {
    extern_methods!(
        #[unsafe(method(subsystem))]
        pub fn subsystem(&self) -> Option<Retained<NSString>>;

        #[unsafe(method(category))]
        pub fn category(&self) -> Option<Retained<NSString>>;

        #[unsafe(method(messageType))]
        pub fn message_type(&self) -> i8;
    );
}

// ---------------------------------------------------------------------------
// OSActivitySignpostEvent
// ---------------------------------------------------------------------------

extern_class!(
    /// A signpost event — the primary type we care about.
    #[unsafe(super(
        OSActivityLogMessageEvent,
        OSActivityEventMessage,
        OSActivityEvent,
        NSObject
    ))]
    pub(crate) struct OSActivitySignpostEvent;
);

impl OSActivitySignpostEvent {
    extern_methods!(
        #[unsafe(method(signpostID))]
        pub fn signpost_id(&self) -> u64;

        /// The signpost name passed to `os_signpost_emit_with_name_impl`.
        /// Falls back to `None` if the runtime class doesn't expose this selector.
        #[unsafe(method(signpostName))]
        pub fn signpost_name(&self) -> Option<Retained<NSString>>;
    );
}
