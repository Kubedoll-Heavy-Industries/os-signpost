#![cfg(feature = "tracing")]

use os_signpost::layer::SignpostLayer;
use tracing_subscriber::prelude::*;

#[test]
fn layer_with_span_does_not_panic() {
    let _guard = tracing_subscriber::registry()
        .with(SignpostLayer::new("com.test.os-signpost"))
        .set_default();

    let span = tracing::info_span!("test_span");
    let _enter = span.enter();
}

#[test]
fn layer_with_event_does_not_panic() {
    let _guard = tracing_subscriber::registry()
        .with(SignpostLayer::new("com.test.os-signpost"))
        .set_default();

    tracing::info!("test event");
}

#[test]
fn layer_nested_spans() {
    let _guard = tracing_subscriber::registry()
        .with(SignpostLayer::new("com.test.os-signpost"))
        .set_default();

    let outer = tracing::info_span!("outer");
    let _outer_enter = outer.enter();
    {
        let inner = tracing::info_span!("inner");
        let _inner_enter = inner.enter();
        tracing::debug!("inside inner");
    }
    // inner dropped, outer still active
}

#[test]
fn layer_span_reenter() {
    let _guard = tracing_subscriber::registry()
        .with(SignpostLayer::new("com.test.os-signpost"))
        .set_default();

    let span = tracing::info_span!("reentrant");
    {
        let _enter = span.enter();
    }
    // Re-enter the same span — should create a new signpost interval
    {
        let _enter = span.enter();
    }
}

#[test]
fn layer_with_custom_category() {
    let _guard = tracing_subscriber::registry()
        .with(
            SignpostLayer::new("com.test.os-signpost")
                .with_category(os_signpost::Category::PointsOfInterest),
        )
        .set_default();

    let span = tracing::info_span!("poi_span");
    let _enter = span.enter();
    tracing::info!("poi event");
}
