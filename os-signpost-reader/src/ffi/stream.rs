//! objc2 bindings for OSActivityStream.
//!
//! OSActivityStream is the main entry point for reading system signposts.
//! It uses a delegate pattern to deliver events on Apple's internal thread.

use crate::reader::{Error, SignpostEntry, SignpostFilter, SignpostReaderGuard};

// TODO: implement extern_class! + extern_methods! bindings for OSActivityStream
// TODO: implement OSActivityStreamDelegate protocol
// TODO: implement start_stream function
// This is the main implementation work for Track 2.

/// Internal state for a running stream, held by SignpostReaderGuard.
pub(crate) struct StreamGuardInner {
    // Will hold Retained<OSActivityStream> and the flume sender
}

impl Drop for StreamGuardInner {
    fn drop(&mut self) {
        // TODO: call stream.stop()
    }
}

pub(crate) fn start_stream(
    _filter: SignpostFilter,
    _channel_capacity: usize,
) -> Result<(SignpostReaderGuard, flume::Receiver<SignpostEntry>), Error> {
    // TODO: implement
    Err(Error::StreamInit("not yet implemented".into()))
}
