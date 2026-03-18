# os-signpost-reader

In-process reader for macOS system signposts via Apple's `OSActivityStream` API.

Captures signpost events emitted by Apple frameworks (Metal, Core Animation, IOSurface, GCD, etc.) and your own application, then exports them as OpenTelemetry spans or `tracing` events.

## OpenTelemetry integration

The primary use case. Your application owns the `TracerProvider` and exporter configuration. This crate uses `opentelemetry::global::tracer("os-signpost-reader")` to emit spans.

```rust,no_run
use os_signpost_reader::{SignpostOtelExporter, Subsystem};

// Your app has already called:
//   opentelemetry::global::set_tracer_provider(provider);

let _guard = SignpostOtelExporter::builder()
    .subsystems(&[Subsystem::METAL, Subsystem::CORE_ANIMATION])
    .start()
    .unwrap();

// Metal shader compilations, CA frame commits, etc. now appear
// as OTel spans in your configured exporter (Tempo, Jaeger, etc.)
//
// Drop `_guard` to stop.
```

The builder also supports `.categories()`, `.pids()`, and `.channel_capacity()` for fine-grained control.

## Subsystem constants

| Constant | Subsystem ID | Captures |
|----------|-------------|----------|
| `METAL` | `com.apple.Metal` | Shader compilation, command buffers, resource allocation |
| `CORE_ANIMATION` | `com.apple.CoreAnimation` | Layer compositing, frame commits |
| `GPU` | `com.apple.gpu` | GPU driver events |
| `IO_SURFACE` | `com.apple.IOSurface` | Pixel buffer allocation/sharing |
| `SKYLIGHT` | `com.apple.SkyLight` | Window server rendering |
| `DISPATCH` | `com.apple.Dispatch` | GCD scheduling |
| `WINDOW_SERVER` | `com.apple.windowserver` | Display compositor, input routing |

For custom subsystems: `Subsystem::custom("your.reverse.dns.id")`.

## Advanced: SignpostReader

For custom consumers that don't use OTel or tracing, use `SignpostReader` directly to get a `flume::Receiver<SignpostEntry>`:

```rust,no_run
use os_signpost_reader::{SignpostReader, SignpostFilter, Subsystem};

let filter = SignpostFilter {
    subsystems: vec![Subsystem::METAL.as_str().to_string()],
    ..Default::default()
};

let (guard, rx) = SignpostReader::new(filter)
    .with_channel_capacity(8192)
    .start()
    .unwrap();

// Process entries however you want
while let Ok(entry) = rx.recv() {
    println!("{}: {} ({})", entry.subsystem, entry.signpost_name, entry.signpost_type as u8);
}
```

Each `SignpostEntry` includes subsystem, category, name, signpost ID, timestamps (wall-clock and Mach absolute time), process/thread info, message payload, and activity correlation IDs (`trace_id`, `activity_id`, `parent_activity_id`).

## Feature flags

| Feature | Description |
|---------|-------------|
| `opentelemetry` | `SignpostOtelExporter` -- system signposts as OTel spans via global tracer |
| `tracing` | `SignpostTracingBridge` -- system signposts as `tracing::Span` / `tracing::Event` |

Both features require a tokio runtime.

## Platform support

macOS only. On other platforms, `SignpostReader::start()` returns `Error::UnsupportedPlatform`.

## MSRV

1.94 -- Edition 2024.

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or [MIT License](../LICENSE-MIT) at your option.

## See also

- [`os-signpost`](../os-signpost/) -- the write-side crate for emitting your own signposts
