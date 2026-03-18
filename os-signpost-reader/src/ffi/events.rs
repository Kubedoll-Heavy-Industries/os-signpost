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

// TODO: implement extern_class! + extern_methods! bindings
// This is the main implementation work for Track 2.
