//! objc2 bindings to LoggingSupport.framework (private API).

mod events;
mod stream;

pub(crate) use stream::{StreamGuardInner, start_stream};
