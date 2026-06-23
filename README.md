# os-signpost

Rust bindings for Apple's signpost and activity tracing APIs.

**Write** signposts visible in Instruments.app. **Read** system signposts from Metal, Core Animation, and other Apple frameworks -- and export them as OpenTelemetry spans.

## Crates

| Crate | Description |
|-------|-------------|
| [`os-signpost`](os-signpost/) | Write signpost intervals and events via direct FFI to `libsystem_trace` |
| [`os-signpost-reader`](os-signpost-reader/) | Read system signposts via `OSActivityStream` and export to OTel/tracing |

## Quick start: Writing signposts

```rust
use os_signpost::{Signposter, Category};

let profiler = Signposter::new("com.example.myapp", Category::PointsOfInterest);
let _interval = profiler.begin_interval("compute");
profiler.event("checkpoint");
```

Signpost intervals appear as ranges in Instruments. Events appear as point markers.

## Quick start: Reading system signposts

```rust,no_run
use os_signpost_reader::{SignpostOtelExporter, Subsystem};

// Application must have already called global::set_tracer_provider(...)
let _guard = SignpostOtelExporter::builder()
    .subsystems(&[Subsystem::METAL, Subsystem::CORE_ANIMATION])
    .start()
    .unwrap();
```

System signposts from Metal shader compilation, Core Animation frame commits, and other Apple frameworks are captured and exported as OpenTelemetry spans.

## Activity correlation

Apple's Activity Tracing API (`os_activity_*`) organizes signposts into causal trees. The `os_signpost::activity::Activity` type lets you push an activity scope onto the current thread. All signposts emitted within that scope inherit the activity's trace ID and parent chain. When `os-signpost-reader` captures these signposts, the `trace_id`, `activity_id`, and `parent_activity_id` fields on each `SignpostEntry` carry this correlation forward into your OTel pipeline.

```rust
use os_signpost::{Signposter, Category};
use os_signpost::activity::Activity;

let activity = Activity::new("inference-request");
let _scope = activity.enter();

// All signposts on this thread now belong to this activity
let profiler = Signposter::new("ai.example", Category::DynamicTracing);
let _interval = profiler.begin_interval("forward_pass");
```

## Available subsystems

Constants provided by `os_signpost_reader::Subsystem` for filtering system signposts:

| Constant | Subsystem ID | What it captures |
|----------|-------------|-----------------|
| `METAL` | `com.apple.Metal` | Shader compilation, command buffer execution, resource allocation |
| `CORE_ANIMATION` | `com.apple.CoreAnimation` | Layer compositing, frame commits, animation timing |
| `GPU` | `com.apple.gpu` | GPU driver-level events |
| `IO_SURFACE` | `com.apple.IOSurface` | Pixel buffer allocation and inter-process sharing |
| `SKYLIGHT` | `com.apple.SkyLight` | Window server rendering engine |
| `DISPATCH` | `com.apple.Dispatch` | GCD queue scheduling and block execution |
| `WINDOW_SERVER` | `com.apple.windowserver` | Display compositor and input event routing |

Use `Subsystem::custom("your.reverse.dns")` for custom subsystems.

## Feature flags

### os-signpost

| Feature | Description |
|---------|-------------|
| `disable-signposts` | Force no-op backend on all platforms (for benchmarking overhead) |
| `tracing` | `SignpostLayer` -- a `tracing_subscriber::Layer` that emits signpost intervals from tracing spans |

### os-signpost-reader

| Feature | Description |
|---------|-------------|
| `opentelemetry` | `SignpostOtelExporter` -- reads system signposts, exports as OTel spans via global tracer |
| `tracing` | `SignpostTracingBridge` -- converts signpost entries to `tracing::Span` / `tracing::Event` |

## Platform support

| Crate | macOS / iOS | Linux / Windows |
|-------|-------------|-----------------|
| `os-signpost` | Full signpost emission | Zero-cost no-ops |
| `os-signpost-reader` | Full reader via `OSActivityStream` | `Error::UnsupportedPlatform` |

## MSRV

1.94 -- Edition 2024.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
