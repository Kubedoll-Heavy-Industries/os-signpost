//! objc2 bindings to LoggingSupport.framework (private API).
// The extern_protocol! macro generates an unsafe trait without a # Safety
// section in the doc comment. Clippy sees the generated code and warns.
#![allow(clippy::missing_safety_doc)]

mod events;
mod stream;

pub(crate) use stream::{StreamGuardInner, start_stream};
