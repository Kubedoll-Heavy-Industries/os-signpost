# os-signpost

Rust bindings to Apple's [`os_signpost`](https://developer.apple.com/documentation/os/signpost) API for performance instrumentation in [Instruments.app](https://developer.apple.com/instruments/).

Forked from [mhallin/signpost-rs](https://github.com/mhallin/signpost-rs) and rewritten with a modern API, direct FFI (no C shim), and production-grade CI/CD.

## Features

- **RAII intervals** that automatically end on drop
- **Point-in-time events** with optional metadata messages
- **Zero-cost no-ops** on non-Apple platforms
- **No C compiler required** — direct FFI to stable `libsystem_trace` functions
- **`disable-signposts` feature** to force no-ops everywhere (for benchmarking overhead)

## Usage

```rust
use os_signpost::{Signposter, Category};
use std::sync::LazyLock;

static PROFILER: LazyLock<Signposter> = LazyLock::new(|| {
    Signposter::new("com.example.myapp", Category::PointsOfInterest)
});

fn do_work() {
    // Scoped interval — visible as a range in Instruments
    let _interval = PROFILER.begin_interval("compute");

    // Point-in-time event
    PROFILER.event("checkpoint");

    // Interval with metadata
    let interval = PROFILER.begin_interval_with_message("batch", "size=1024");
    // ... work ...
    interval.end_with_message("processed 1024 items");
}
```

## Platform Support

| Platform | Behavior |
|----------|----------|
| macOS / iOS / tvOS / watchOS | Full signpost emission |
| Linux / Windows / other | All methods are zero-cost no-ops |

## Minimum Supported Rust Version

1.94

## License

Licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.
