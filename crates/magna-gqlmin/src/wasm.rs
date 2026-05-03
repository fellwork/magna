// SPDX-License-Identifier: MIT OR Apache-2.0
//! Wasm shim — allocator, panic handler, and extern "C" exports.
//!
//! This entire module is compiled only when `feature = "wasm"` is active.
//! It MUST NOT pull in `std`; the build uses `--no-default-features --features
//! "ops,wasm"` which activates `#![no_std]` (via lib.rs `cfg_attr`).
//!
//! **No `String`, no `format!`, no allocating error paths here.**
//! Any allocation is through the raw `alloc::alloc` API.

// Allocator — dlmalloc replaces the default (missing) allocator in no_std.
#[cfg(feature = "wasm")]
#[global_allocator]
static ALLOC: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

// Panic handler — exactly one allowed per binary; exclude from test builds
// because the test harness provides its own.
#[cfg(all(feature = "wasm", not(test)))]
#[panic_handler]
fn panic_handler(_info: &core::panic::PanicInfo<'_>) -> ! {
    // Emit the wasm32 `unreachable` instruction — this traps the program.
    core::arch::wasm32::unreachable()
}

// ---------------------------------------------------------------------------
// Exported ABI
// ---------------------------------------------------------------------------

/// Allocate `len` bytes in wasm linear memory.
/// The caller must free with `gqlmin_free(ptr, len)`.
#[no_mangle]
pub unsafe extern "C" fn gqlmin_alloc(len: usize) -> *mut u8 {
    match alloc::alloc::Layout::from_size_align(len, 1) {
        Ok(layout) => alloc::alloc::alloc(layout),
        Err(_) => core::ptr::null_mut(),
    }
}

/// Free memory previously allocated by `gqlmin_alloc`.
#[no_mangle]
pub unsafe extern "C" fn gqlmin_free(ptr: *mut u8, len: usize) {
    if let Ok(layout) = alloc::alloc::Layout::from_size_align(len, 1) {
        alloc::alloc::dealloc(ptr, layout);
    }
}

/// Parse a GraphQL operation document.
///
/// Input:  UTF-8 bytes at `src_ptr[0..src_len]`.
///
/// Output: pointer to a result buffer in wasm linear memory with layout:
///   `[u8 tag]`  where 0 = success, 1 = parse error
///   `[u32 le]`  payload length (bytes following this header)
///   for tag=0: zero payload bytes (full AST encoding deferred to JS decoder)
///   for tag=1: `[u32 le span_start][u32 le span_end][u8 kind]`
///
/// Caller must free the result with `gqlmin_result_free`.
#[no_mangle]
pub unsafe extern "C" fn gqlmin_parse(src_ptr: *const u8, src_len: usize) -> *const u8 {
    let bytes = core::slice::from_raw_parts(src_ptr, src_len);
    let src = match core::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            return encode_error(0, 0, crate::ParseErrorKind::UnexpectedChar as u8);
        }
    };
    match crate::parse_executable_document(src) {
        Ok(_) => encode_ok(),
        Err(e) => encode_error(e.span.start, e.span.end, e.kind as u8),
    }
}

/// Free a result buffer returned by `gqlmin_parse`.
#[no_mangle]
pub unsafe extern "C" fn gqlmin_result_free(ptr: *const u8) {
    // Read the payload length to determine total allocation size.
    let len_bytes = core::slice::from_raw_parts(ptr.add(1), 4);
    let payload_len =
        u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]) as usize;
    let total = 1 + 4 + payload_len;
    if let Ok(layout) = alloc::alloc::Layout::from_size_align(total, 1) {
        alloc::alloc::dealloc(ptr as *mut u8, layout);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Encode a success result (tag=0, payload_len=0).
unsafe fn encode_ok() -> *const u8 {
    let total = 1 + 4; // tag + payload_len field
    let layout = match alloc::alloc::Layout::from_size_align(total, 1) {
        Ok(l) => l,
        Err(_) => return core::ptr::null(),
    };
    let ptr = alloc::alloc::alloc(layout);
    if ptr.is_null() {
        return core::ptr::null();
    }
    *ptr = 0u8; // tag = ok
    let len_bytes = 0u32.to_le_bytes();
    core::ptr::copy_nonoverlapping(len_bytes.as_ptr(), ptr.add(1), 4);
    ptr
}

/// Encode a parse error result (tag=1, payload = span_start + span_end + kind).
unsafe fn encode_error(span_start: u32, span_end: u32, kind: u8) -> *const u8 {
    let payload: usize = 4 + 4 + 1; // span_start + span_end + kind
    let total = 1 + 4 + payload;
    let layout = match alloc::alloc::Layout::from_size_align(total, 1) {
        Ok(l) => l,
        Err(_) => return core::ptr::null(),
    };
    let ptr = alloc::alloc::alloc(layout);
    if ptr.is_null() {
        return core::ptr::null();
    }
    *ptr = 1u8; // tag = error
    let payload_len_bytes = (payload as u32).to_le_bytes();
    core::ptr::copy_nonoverlapping(payload_len_bytes.as_ptr(), ptr.add(1), 4);
    let start_bytes = span_start.to_le_bytes();
    core::ptr::copy_nonoverlapping(start_bytes.as_ptr(), ptr.add(5), 4);
    let end_bytes = span_end.to_le_bytes();
    core::ptr::copy_nonoverlapping(end_bytes.as_ptr(), ptr.add(9), 4);
    *ptr.add(13) = kind;
    ptr
}
