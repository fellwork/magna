// SPDX-License-Identifier: MIT OR Apache-2.0
//! Wasm shim — allocator, panic handler, and extern "C" exports.
//!
//! This entire module is compiled only when `feature = "wasm"` is active.
//! It MUST NOT pull in `std`; the build uses `--no-default-features --features
//! "ops,wasm"` which activates `#![no_std]` (via lib.rs `cfg_attr`).
//!
//! **No `String`, no `format!`, no allocating error paths here.**
//! Any allocation is through the raw `alloc::alloc` API.

// ---------------------------------------------------------------------------
// Bump allocator (R8) — replaces dlmalloc.
// ---------------------------------------------------------------------------
//
// Lifecycle: `gqlmin_parse` is parse-once-then-drop. Each call allocates a
// source buffer, a parser arena, and a result buffer; the caller reads the
// result, then calls `gqlmin_result_free` and `gqlmin_free`. We never reuse
// memory mid-parse, so a no-op `dealloc` is correct.
//
// Reset strategy: **fill-then-rollover** (no reset). 256 KiB supports many
// parse calls of typical query sizes (median GraphQL operation is well under
// 1 KiB; the parser arena and result buffer add a few KiB at most). We do
// NOT reset `OFFSET` inside `gqlmin_parse` because the source bytes that
// `gqlmin_alloc` placed at offset 0 are still being read by the parser at
// that point — resetting would let the parser's own allocations overwrite
// its input. Once `OFFSET` reaches `ARENA_SIZE` further allocations return
// null, which `handle_alloc_error` traps via `unreachable` (no panic
// strings emitted, see panic_handler below). For the production wasm
// runtime the host is expected to instantiate a fresh module per long-
// running session if it needs to parse more than ~hundreds of operations
// before reset.
//
// SAFETY: wasm32 linear memory is single-threaded; access to `OFFSET` does
// not need atomics.

#[cfg(feature = "wasm")]
const ARENA_SIZE: usize = 256 * 1024;

#[cfg(feature = "wasm")]
static mut ARENA: [u8; ARENA_SIZE] = [0; ARENA_SIZE];

#[cfg(feature = "wasm")]
static mut OFFSET: usize = 0;

#[cfg(feature = "wasm")]
struct BumpAllocator;

#[cfg(feature = "wasm")]
unsafe impl core::alloc::GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        // SAFETY: single-threaded wasm linear memory; no concurrent access.
        let align = layout.align();
        let size = layout.size();
        let off = OFFSET;
        // Align the bump pointer up. align is always a power of two per
        // the Layout invariants, so the mask form is correct.
        let mask = align.wrapping_sub(1);
        let start = off.wrapping_add(mask) & !mask;
        // Bail if alignment overflowed `usize` or pushed past the arena.
        if start < off || start > ARENA_SIZE {
            return core::ptr::null_mut();
        }
        let end = match start.checked_add(size) {
            Some(e) => e,
            None => return core::ptr::null_mut(),
        };
        if end > ARENA_SIZE {
            return core::ptr::null_mut();
        }
        OFFSET = end;
        // Use a raw pointer to the static to avoid creating a reference to
        // a `mut static` (forbidden under Rust 2024). `core::ptr::addr_of_mut!`
        // gives us the address without going through a reference.
        let base: *mut u8 = core::ptr::addr_of_mut!(ARENA) as *mut u8;
        base.add(start)
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {
        // No-op: parse-once-then-drop lifecycle. Memory is reclaimed only
        // when the wasm instance is recycled.
    }
}

#[cfg(feature = "wasm")]
#[global_allocator]
static ALLOC: BumpAllocator = BumpAllocator;

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
    // SAFETY: The lexer works byte-by-byte for all ASCII tokens. It only
    // reads past ASCII in string literal content (which it never decodes),
    // so an invalid UTF-8 sequence in a string literal body will be passed
    // through as an opaque byte range and returned as a StringValue span.
    // Using from_utf8_unchecked avoids pulling in the UTF-8 validation
    // machinery (which brings in core::str::Utf8Error formatting paths and
    // source-location strings that add ~1 KB to the wasm gz size).
    // The JS caller is responsible for sending valid UTF-8; any stray bytes
    // will at worst produce an UnexpectedChar error from the lexer.
    let src = unsafe { core::str::from_utf8_unchecked(bytes) };
    // R5 phase 1: AST is owned by `Document<'src>` (single lifetime). The
    // value is dropped at end of scope, freeing the AST. ABI unchanged.
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
