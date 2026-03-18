# CLAUDE.md

## Project Overview

os-signpost provides Rust bindings to Apple's `os_signpost` API for performance instrumentation visible in Instruments.app. It uses direct FFI to `libsystem_trace.dylib` — no C shim, no objc2, no proc macros.

## Build & Test

```bash
cargo build                          # build (no-op backend on non-macOS)
cargo test                           # run all tests
cargo clippy --all-targets           # lint
cargo fmt --check                    # format check
cargo deny check                     # license + advisory + ban + source checks
cargo semver-checks check-release    # semver compatibility
```

MSRV is 1.94. Edition 2024. Target: crates.io publication as `os-signpost`.

## Architecture

Single crate with cfg-switched backends:

- `src/lib.rs` — Public API: `Signposter`, `SignpostInterval`, `Category`
- `src/backend.rs` — macOS/iOS FFI to `os_log_create`, `os_signpost_id_generate`, `_os_signpost_emit_with_name_impl`
- `src/noop.rs` — Zero-cost no-op implementations for other platforms

The `disable-signposts` feature forces the no-op backend on all platforms.

## FFI Details

The `os_signpost_interval_begin/end` C macros are not callable from Rust. Instead, we call `_os_signpost_emit_with_name_impl` directly — the function these macros expand to. It has been stable since macOS 10.14 and is exported from `libsystem_trace.dylib` (part of `libSystem`).

For messages, we encode a `%{public}s` format string with the os_log buffer format (summary byte, arg count, type descriptor, pointer).

## CI/CD

- **ci.yml** — fmt, clippy, test matrix (MSRV/stable/nightly × ubuntu + stable × macOS), docs, semver
- **security.yml** — cargo-audit, cargo-deny, cargo-vet, daily schedule
- **pr.yml** — conventional commit title enforcement
- **release.yml** — release-please → CI/security gate → trusted publishing → provenance → SBOM
