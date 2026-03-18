//! macOS/iOS backend — direct FFI to `libsystem_trace.dylib`.

use std::ffi::CString;
use std::fmt;
use std::mem::ManuallyDrop;
use std::os::raw::c_char;
use std::sync::Once;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::Category;

// ---------------------------------------------------------------------------
// FFI declarations
// ---------------------------------------------------------------------------

// os_log_t is an opaque pointer, represented as usize for atomics.
type OsLogT = usize;
type OsSignpostId = u64;
type OsSignpostType = u8;

const SIGNPOST_EVENT: OsSignpostType = 0;
const SIGNPOST_INTERVAL_BEGIN: OsSignpostType = 1;
const SIGNPOST_INTERVAL_END: OsSignpostType = 2;

unsafe extern "C" {
    // Linker-synthesized symbol pointing to the Mach-O header of this DSO.
    // Declared as immutable u8 — we only ever take its address.
    static __dso_handle: u8;

    fn os_log_create(subsystem: *const c_char, category: *const c_char) -> OsLogT;
    fn os_signpost_id_generate(log: OsLogT) -> OsSignpostId;
    fn os_signpost_enabled(log: OsLogT) -> bool;

    fn _os_signpost_emit_with_name_impl(
        dso: *const u8,
        log: OsLogT,
        r#type: OsSignpostType,
        spid: OsSignpostId,
        name: *const c_char,
        format: *const u8,
        buf: *mut u8,
        size: u32,
    );
}

// ---------------------------------------------------------------------------
// Category → C string
// ---------------------------------------------------------------------------

const CATEGORY_POINTS_OF_INTEREST: &[u8] = b"PointsOfInterest\0";
const CATEGORY_DYNAMIC_TRACING: &[u8] = b"DynamicTracing\0";
const CATEGORY_DYNAMIC_STACK_TRACING: &[u8] = b"DynamicStackTracing\0";

// ---------------------------------------------------------------------------
// SignposterInner
// ---------------------------------------------------------------------------

pub(crate) struct SignposterInner {
    handle: AtomicUsize,
    init: Once,
    subsystem: CString,
    category_raw: Vec<u8>, // null-terminated
}

impl SignposterInner {
    pub(crate) fn new(subsystem: &str, category: Category) -> Self {
        let category_raw = match &category {
            Category::PointsOfInterest => CATEGORY_POINTS_OF_INTEREST.to_vec(),
            Category::DynamicTracing => CATEGORY_DYNAMIC_TRACING.to_vec(),
            Category::DynamicStackTracing => CATEGORY_DYNAMIC_STACK_TRACING.to_vec(),
            Category::Custom(s) => {
                let mut v = s.as_bytes().to_vec();
                v.push(0);
                v
            }
        };

        Self {
            handle: AtomicUsize::new(0),
            init: Once::new(),
            subsystem: CString::new(subsystem).expect("subsystem must not contain NUL bytes"),
            category_raw,
        }
    }

    fn get_log(&self) -> OsLogT {
        self.init.call_once(|| {
            let log = unsafe {
                os_log_create(self.subsystem.as_ptr(), self.category_raw.as_ptr().cast())
            };
            self.handle.store(log, Ordering::Release);
        });
        self.handle.load(Ordering::Acquire)
    }

    pub(crate) fn enabled(&self) -> bool {
        unsafe { os_signpost_enabled(self.get_log()) }
    }

    pub(crate) fn event(&self, name: &str, msg: Option<impl fmt::Display>) {
        let log = self.get_log();
        let name_c = CString::new(name).expect("signpost name must not contain NUL bytes");
        let spid = unsafe { os_signpost_id_generate(log) };

        match msg {
            Some(m) => {
                let formatted = m.to_string();
                emit(log, SIGNPOST_EVENT, spid, &name_c, Some(&formatted));
            }
            None => emit(log, SIGNPOST_EVENT, spid, &name_c, None),
        }
    }

    pub(crate) fn begin_interval(
        &self,
        name: &str,
        msg: Option<impl fmt::Display>,
    ) -> SignpostIntervalInner {
        let log = self.get_log();
        let name_c = CString::new(name).expect("signpost name must not contain NUL bytes");
        let spid = unsafe { os_signpost_id_generate(log) };

        match msg {
            Some(m) => {
                let formatted = m.to_string();
                emit(
                    log,
                    SIGNPOST_INTERVAL_BEGIN,
                    spid,
                    &name_c,
                    Some(&formatted),
                );
            }
            None => emit(log, SIGNPOST_INTERVAL_BEGIN, spid, &name_c, None),
        }

        SignpostIntervalInner {
            log,
            spid,
            name: name_c,
        }
    }
}

// ---------------------------------------------------------------------------
// SignpostIntervalInner
// ---------------------------------------------------------------------------

pub(crate) struct SignpostIntervalInner {
    log: OsLogT,
    spid: OsSignpostId,
    name: CString,
}

impl SignpostIntervalInner {
    pub(crate) fn end_with_message(self, msg: impl fmt::Display) {
        let this = ManuallyDrop::new(self);
        let formatted = msg.to_string();
        emit(
            this.log,
            SIGNPOST_INTERVAL_END,
            this.spid,
            &this.name,
            Some(&formatted),
        );
        // Drop is suppressed — interval ended exactly once.
    }
}

impl Drop for SignpostIntervalInner {
    fn drop(&mut self) {
        emit(self.log, SIGNPOST_INTERVAL_END, self.spid, &self.name, None);
    }
}

// ---------------------------------------------------------------------------
// Shared emit helper
// ---------------------------------------------------------------------------

/// Emit a signpost with an optional `%{public}s` message.
fn emit(
    log: OsLogT,
    r#type: OsSignpostType,
    spid: OsSignpostId,
    name: &CString,
    msg: Option<&str>,
) {
    // When a message is provided we pass a `%{public}s` format string and
    // encode the pointer + length into the buffer the way the os_signpost
    // macros do. When no message is provided, format and buffer are both
    // null/empty — producing a bare signpost with just the name.
    match msg {
        Some(text) => {
            let text_c = CString::new(text).unwrap_or_else(|_| CString::new("?").unwrap());
            // The os_log buffer format for a single %{public}s argument:
            //   [summary byte] [arg count byte] [arg descriptor...] [arg data...]
            // For one public string:
            //   summary = 0x02 (has_non_scalar_items)
            //   count = 0x01
            //   descriptor: type=0x22 (StringKind=2 << 4 | IsPublic=0x2)
            //   size: 0x08 (pointer size on 64-bit)
            //   data: 8-byte pointer to the C string
            let ptr_bytes = (text_c.as_ptr() as u64).to_le_bytes();
            let mut buf = [0u8; 12];
            buf[0] = 0x02; // summary: has non-scalar items
            buf[1] = 0x01; // one argument
            buf[2] = 0x22; // descriptor: public string (StringKind << 4 | IsPublic)
            buf[3] = 0x08; // size: 8 bytes (pointer)
            buf[4..12].copy_from_slice(&ptr_bytes);

            // Format string: %{public}s
            static FMT: &[u8] = b"%{public}s\0";

            unsafe {
                _os_signpost_emit_with_name_impl(
                    &raw const __dso_handle,
                    log,
                    r#type,
                    spid,
                    name.as_ptr(),
                    FMT.as_ptr(),
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                );
            }
        }
        None => {
            let mut buf = [0u8; 2];
            buf[0] = 0x00; // summary: no items
            buf[1] = 0x00; // zero arguments

            unsafe {
                _os_signpost_emit_with_name_impl(
                    &raw const __dso_handle,
                    log,
                    r#type,
                    spid,
                    name.as_ptr(),
                    std::ptr::null(),
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                );
            }
        }
    }
}
