//! End-to-end test: emit signposts and verify they appear via `log stream`.
//!
//! Signpost entries go to the kernel signpost buffer, not the unified log.
//! They are not readable via `OSLogStore`. Instead, we use `log stream
//! --signpost` to capture them from a background process.
//!
//! This test only runs on macOS.

#![cfg(target_os = "macos")]

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use os_signpost::{Category, Signposter};

/// Spawn `log stream --signpost`, emit signposts, collect output, assert.
///
/// Retries up to 3 times because `log stream` needs to fully attach to the
/// kernel signpost buffer before it can capture events, and there is no
/// reliable way to know when attachment is complete.
fn capture_signposts<F>(subsystem: &str, emit: F) -> Vec<String>
where
    F: FnOnce(&Signposter) + Copy,
{
    for attempt in 1..=3 {
        let lines = capture_signposts_once(subsystem, emit);
        if !lines.is_empty() {
            return lines;
        }
        eprintln!("attempt {attempt}: log stream returned 0 lines, retrying...");
        thread::sleep(Duration::from_millis(500));
    }
    // Return empty on final failure — caller will assert and produce a good error
    Vec::new()
}

fn capture_signposts_once<F>(subsystem: &str, emit: F) -> Vec<String>
where
    F: FnOnce(&Signposter),
{
    let mut child = Command::new("/usr/bin/log")
        .args([
            "stream",
            "--signpost",
            "--predicate",
            &format!("subsystem == \"{}\"", subsystem),
            "--style",
            "ndjson",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn `log stream`");

    // Give log stream time to attach to the kernel signpost buffer
    thread::sleep(Duration::from_secs(1));

    let signposter = Signposter::new(subsystem, Category::PointsOfInterest);
    emit(&signposter);

    // Wait for the entries to flow through
    thread::sleep(Duration::from_secs(1));

    // Kill the stream
    let _ = child.kill();

    let stdout = child.stdout.take().expect("no stdout");
    let reader = BufReader::new(stdout);
    reader
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.starts_with("Filtering") && !line.contains("\"finished\""))
        .collect()
}

#[test]
#[ignore = "requires macOS log stream; run with --ignored"]
fn e2e_event_captured_by_log_stream() {
    let lines = capture_signposts("com.test.os-signpost.e2e.ev", |s| {
        s.event("my_test_event");
    });

    let found = lines.iter().any(|l| l.contains("my_test_event"));
    assert!(
        found,
        "expected 'my_test_event' in log stream output, got {} lines: {:?}",
        lines.len(),
        &lines[..lines.len().min(5)]
    );
}

#[test]
#[ignore = "requires macOS log stream; run with --ignored"]
fn e2e_interval_captured_by_log_stream() {
    let lines = capture_signposts("com.test.os-signpost.e2e.iv", |s| {
        let _interval = s.begin_interval("my_test_interval");
        thread::sleep(Duration::from_millis(50));
        // drops here → interval end
    });

    let has_begin = lines
        .iter()
        .any(|l| l.contains("my_test_interval") && l.contains("\"signpostType\":\"begin\""));
    let has_end = lines
        .iter()
        .any(|l| l.contains("my_test_interval") && l.contains("\"signpostType\":\"end\""));

    assert!(
        has_begin,
        "expected interval Begin for 'my_test_interval' in log stream, got: {:?}",
        &lines[..lines.len().min(5)]
    );
    assert!(
        has_end,
        "expected interval End for 'my_test_interval' in log stream, got: {:?}",
        &lines[..lines.len().min(5)]
    );
}

#[test]
#[ignore = "requires macOS log stream; run with --ignored"]
fn e2e_message_captured_by_log_stream() {
    let lines = capture_signposts("com.test.os-signpost.e2e.mg", |s| {
        s.event_with_message("msg_event", "hello_from_e2e");
    });

    let has_message = lines.iter().any(|l| l.contains("hello_from_e2e"));
    assert!(
        has_message,
        "expected message 'hello_from_e2e' in log stream output, got {} lines: {:?}",
        lines.len(),
        &lines[..lines.len().min(5)]
    );
}
