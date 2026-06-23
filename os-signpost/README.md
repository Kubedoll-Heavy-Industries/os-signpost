# os-signpost

Rust bindings to Apple's [`os_signpost`](https://developer.apple.com/documentation/os/signpost) API for performance instrumentation in [Instruments.app](https://developer.apple.com/instruments/).

## Features

- **RAII intervals** that automatically end on drop
- **Point-in-time events** with optional metadata messages
- **Activity tracing** -- correlate signposts into causal trees via `os_activity_*`
- **Zero-cost no-ops** on non-Apple platforms
- **No C compiler required** -- direct FFI to stable `libsystem_trace` functions
- **`disable-signposts` feature** to force no-ops everywhere (for benchmarking overhead)
- **`tracing` feature** -- a `tracing_subscriber::Layer` that emits signpost intervals from tracing spans

## Usage

```rust
use os_signpost::{Signposter, Category};
use std::sync::LazyLock;

static PROFILER: LazyLock<Signposter> = LazyLock::new(|| {
    Signposter::new("com.example.myapp", Category::PointsOfInterest)
});

fn do_work() {
    let _interval = PROFILER.begin_interval("compute");
    PROFILER.event("checkpoint");

    let interval = PROFILER.begin_interval_with_message("batch", "size=1024");
    // ... work ...
    interval.end_with_message("processed 1024 items");
}
```

## Platform support

| Platform | Behavior |
|----------|----------|
| macOS / iOS / tvOS / watchOS | Full signpost emission |
| Linux / Windows / other | All methods are zero-cost no-ops |

## MSRV

1.94 -- Edition 2024.

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or [MIT License](../LICENSE-MIT) at your option.

## See also

- [`os-signpost-reader`](../os-signpost-reader/) -- read system signposts from Metal, Core Animation, and other Apple frameworks and export them as OpenTelemetry spans or tracing events
