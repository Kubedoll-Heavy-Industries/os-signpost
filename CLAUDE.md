# CLAUDE.md

## Project Overview

Cargo workspace with two crates for Apple's signpost and activity tracing APIs:

- **os-signpost** -- Write signposts (intervals, events) via direct FFI to `libsystem_trace.dylib`. Zero-cost no-ops on non-Apple platforms. Includes Activity Tracing API for causal correlation.
- **os-signpost-reader** -- Read system signposts from Metal, Core Animation, and other Apple frameworks via `OSActivityStream`. Exports to OpenTelemetry spans or `tracing` events. macOS only.

## Build & Test

```bash
# Workspace-wide
cargo build                          # build both crates
cargo test                           # run all tests
cargo clippy --all-targets           # lint
cargo fmt --check                    # format check
cargo deny check                     # license + advisory + ban + source checks
cargo semver-checks check-release    # semver compatibility

# Individual crates
cargo build -p os-signpost
cargo build -p os-signpost-reader
cargo test -p os-signpost-reader --features opentelemetry
cargo test -p os-signpost-reader --features tracing
cargo test -p os-signpost --features tracing
```

MSRV is 1.94. Edition 2024.

## Workspace Structure

```
os-signpost/           # write-side crate
  src/
    lib.rs             # Public API: Signposter, SignpostInterval, Category
    backend.rs         # macOS/iOS FFI to libsystem_trace
    noop.rs            # Zero-cost no-op implementations
    activity.rs        # os_activity_* FFI for causal correlation
    layer.rs           # tracing_subscriber::Layer (feature = "tracing")

os-signpost-reader/    # read-side crate
  src/
    lib.rs             # Crate root, feature-gated module exports
    reader.rs          # SignpostReader, SignpostEntry, SignpostFilter
    subsystem.rs       # Subsystem constants (METAL, CORE_ANIMATION, etc.)
    otel_exporter.rs   # SignpostOtelExporter builder (feature = "opentelemetry")
    otel_bridge.rs     # Internal OTel span emission
    tracing_bridge.rs  # SignpostTracingBridge (feature = "tracing")
    ffi/               # OSActivityStream Objective-C runtime bindings
```

## Feature Flags

### os-signpost
- `disable-signposts` -- force no-op backend on all platforms
- `tracing` -- `SignpostLayer` for tracing-subscriber integration

### os-signpost-reader
- `opentelemetry` -- `SignpostOtelExporter`: system signposts as OTel spans
- `tracing` -- `SignpostTracingBridge`: system signposts as tracing spans/events

## FFI Details

### os-signpost (write)
The `os_signpost_interval_begin/end` C macros are not callable from Rust. Instead, we call `_os_signpost_emit_with_name_impl` directly -- the function these macros expand to. Stable since macOS 10.14, exported from `libsystem_trace.dylib`.

For messages, we encode a `%{public}s` format string with the os_log buffer format (summary byte, arg count, type descriptor, pointer).

### os-signpost-reader (read)
Uses `objc2` to call `OSActivityStream` APIs at runtime. The stream callback runs on a private thread; entries are sent through a bounded `flume` channel to avoid blocking the system stream.

## CI/CD

- **ci.yml** -- fmt, clippy, test matrix (MSRV/stable/nightly x ubuntu + stable x macOS), docs, semver
- **security.yml** -- cargo-audit, cargo-deny, cargo-vet, daily schedule
- **pr.yml** -- conventional commit title enforcement
- **release.yml** -- release-please -> CI/security gate -> trusted publishing -> provenance -> SBOM
