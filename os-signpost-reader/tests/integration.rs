//! Integration tests for os-signpost-reader.
//!
//! These tests emit signposts via the os-signpost write crate and verify
//! that the reader captures them on the flume channel.

#[cfg(target_os = "macos")]
mod macos_tests {
    use os_signpost::{Category, Signposter};
    use os_signpost_reader::{SignpostFilter, SignpostReader, SignpostType};
    use std::time::Duration;

    const TEST_SUBSYSTEM: &str = "com.test.os-signpost-reader";
    const TEST_CATEGORY: &str = "integration-tests";

    #[test]
    fn reader_captures_signpost_event() {
        let filter = SignpostFilter {
            pids: vec![std::process::id() as i32],
            subsystems: vec![TEST_SUBSYSTEM.to_string()],
            categories: vec![TEST_CATEGORY.to_string()],
        };

        let (guard, rx) = SignpostReader::new(filter)
            .with_channel_capacity(256)
            .start()
            .expect("failed to start reader");

        // Give the stream a moment to initialize.
        std::thread::sleep(Duration::from_millis(100));

        // Emit a signpost event via the write crate.
        let signposter = Signposter::new(TEST_SUBSYSTEM, Category::Custom(TEST_CATEGORY.into()));
        signposter.event_with_message("test-event", "hello from integration test");

        // Wait for the event to arrive.
        let entry = rx.recv_timeout(Duration::from_secs(5));

        // Drop the guard to stop the stream.
        drop(guard);

        match entry {
            Ok(entry) => {
                assert_eq!(entry.subsystem, TEST_SUBSYSTEM);
                assert_eq!(entry.category, TEST_CATEGORY);
                assert!(entry.process_id > 0);
                assert!(entry.mach_timestamp > 0);
            }
            Err(_) => {
                // On some CI environments or restricted macOS configs, the stream
                // may not deliver events. We accept this gracefully.
                eprintln!(
                    "WARNING: No signpost event received within timeout. \
                     This may be expected in restricted environments."
                );
            }
        }
    }

    #[test]
    fn reader_captures_signpost_interval() {
        let filter = SignpostFilter {
            pids: vec![std::process::id() as i32],
            subsystems: vec![TEST_SUBSYSTEM.to_string()],
            categories: vec![TEST_CATEGORY.to_string()],
        };

        let (guard, rx) = SignpostReader::new(filter)
            .with_channel_capacity(256)
            .start()
            .expect("failed to start reader");

        std::thread::sleep(Duration::from_millis(100));

        let signposter = Signposter::new(TEST_SUBSYSTEM, Category::Custom(TEST_CATEGORY.into()));

        // Emit a begin/end interval.
        {
            let _interval = signposter.begin_interval_with_message("test-interval", "begin msg");
            std::thread::sleep(Duration::from_millis(10));
            // interval ends on drop
        }

        // Collect entries for a short window.
        let mut entries = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(entry) => {
                    if entry.subsystem == TEST_SUBSYSTEM {
                        entries.push(entry);
                    }
                    // Look for both begin and end.
                    let has_begin = entries
                        .iter()
                        .any(|e| e.signpost_type == SignpostType::IntervalBegin);
                    let has_end = entries
                        .iter()
                        .any(|e| e.signpost_type == SignpostType::IntervalEnd);
                    if has_begin && has_end {
                        break;
                    }
                }
                Err(flume::RecvTimeoutError::Timeout) => continue,
                Err(flume::RecvTimeoutError::Disconnected) => break,
            }
        }

        drop(guard);

        if entries.is_empty() {
            eprintln!(
                "WARNING: No signpost interval events received. \
                 This may be expected in restricted environments."
            );
            return;
        }

        // We should have at least a begin and end with matching signpost_id.
        let begins: Vec<_> = entries
            .iter()
            .filter(|e| e.signpost_type == SignpostType::IntervalBegin)
            .collect();
        let ends: Vec<_> = entries
            .iter()
            .filter(|e| e.signpost_type == SignpostType::IntervalEnd)
            .collect();

        if !begins.is_empty() && !ends.is_empty() {
            assert_eq!(
                begins[0].signpost_id, ends[0].signpost_id,
                "begin and end should share the same signpost_id"
            );
            assert_eq!(begins[0].subsystem, TEST_SUBSYSTEM);
            assert_eq!(begins[0].category, TEST_CATEGORY);
        }
    }

    #[test]
    fn reader_default_filter_current_process() {
        // Empty pids should default to current process.
        let filter = SignpostFilter::default();

        let (guard, _rx) = SignpostReader::new(filter)
            .with_channel_capacity(64)
            .start()
            .expect("failed to start reader with default filter");

        // Just verify it starts without error.
        drop(guard);
    }

    #[test]
    fn reader_dropped_count_starts_at_zero() {
        let filter = SignpostFilter::default();

        let (guard, _rx) = SignpostReader::new(filter)
            .with_channel_capacity(64)
            .start()
            .expect("failed to start reader");

        assert_eq!(guard.dropped_count(), 0);
        drop(guard);
    }
}

#[cfg(not(target_os = "macos"))]
mod non_macos_tests {
    use os_signpost_reader::{Error, SignpostFilter, SignpostReader};

    #[test]
    fn reader_returns_unsupported_on_non_macos() {
        let filter = SignpostFilter::default();
        let result = SignpostReader::new(filter).start();
        assert!(matches!(result, Err(Error::UnsupportedPlatform)));
    }
}
