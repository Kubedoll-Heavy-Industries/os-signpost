use os_signpost::{Category, Signposter};

#[test]
fn event_does_not_panic() {
    let s = Signposter::new("com.test.os-signpost", Category::PointsOfInterest);
    s.event("test-event");
}

#[test]
fn event_with_message_does_not_panic() {
    let s = Signposter::new("com.test.os-signpost", Category::PointsOfInterest);
    s.event_with_message("test-event", "hello world");
}

#[test]
fn event_with_format_message() {
    let s = Signposter::new("com.test.os-signpost", Category::DynamicTracing);
    s.event_with_message("batch", format_args!("processed {} items", 42));
}

#[test]
fn interval_drop_does_not_panic() {
    let s = Signposter::new("com.test.os-signpost", Category::DynamicTracing);
    let _interval = s.begin_interval("work");
    // interval ends on drop
}

#[test]
fn interval_with_message_does_not_panic() {
    let s = Signposter::new("com.test.os-signpost", Category::DynamicTracing);
    let _interval = s.begin_interval_with_message("work", "starting batch");
}

#[test]
fn interval_end_with_message() {
    let s = Signposter::new("com.test.os-signpost", Category::PointsOfInterest);
    let interval = s.begin_interval("compute");
    interval.end_with_message("done in 42ms");
}

#[test]
fn enabled_returns_bool() {
    let s = Signposter::new("com.test.os-signpost", Category::DynamicTracing);
    // Just verify it returns without panicking. The value depends on whether
    // Instruments is attached.
    let _ = s.enabled();
}

#[test]
fn points_of_interest_convenience() {
    let s = Signposter::points_of_interest("com.test.os-signpost");
    s.event("poi-event");
}

#[test]
fn custom_category() {
    let s = Signposter::new(
        "com.test.os-signpost",
        Category::Custom("MyCategory".into()),
    );
    s.event("custom-event");
}

#[test]
fn dynamic_stack_tracing_category() {
    let s = Signposter::new("com.test.os-signpost", Category::DynamicStackTracing);
    s.event("stack-event");
}

#[test]
fn multiple_overlapping_intervals() {
    let s = Signposter::new("com.test.os-signpost", Category::PointsOfInterest);
    let _a = s.begin_interval("outer");
    let _b = s.begin_interval("inner");
    // inner drops first, then outer — overlapping intervals should work
}

#[test]
fn signposter_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<Signposter>();
}

#[test]
fn signposter_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<Signposter>();
}

#[test]
fn interval_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<os_signpost::SignpostInterval>();
}
