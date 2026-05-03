# Investigation R2 — wasm size overshoot

## Symptom

Measured gz size: **15783 bytes** against a 5120-byte budget and a 7000-byte
Iron Law threshold.

Reproduce:
```bash
cargo build -p magna-gqlmin \
  --target wasm32-unknown-unknown \
  --no-default-features --features "ops,wasm" \
  --profile release-wasm

wasm-opt -Oz --strip-debug --vacuum \
  --enable-bulk-memory --enable-sign-ext \
  target/wasm32-unknown-unknown/release-wasm/magna_gqlmin.wasm \
  -o /tmp/gqlmin.opt.wasm

RAW=$(wc -c < target/wasm32-unknown-unknown/release-wasm/magna_gqlmin.wasm)
OPT=$(wc -c < /tmp/gqlmin.opt.wasm)
GZ=$(gzip -9 -c /tmp/gqlmin.opt.wasm | wc -c)
echo "raw=$RAW opt=$OPT gz=$GZ"
# Emits: raw=38526 opt=33388 gz=15783
```

Toolchain: rustc 1.89.0 (29483883e 2025-08-04), wasm-opt version 108

---

## Root-cause diagnosis

Examination of the compiled wasm binary via `wasm-dis` revealed three distinct
bloat sources embedded in the data section:

### Cause 1: `core::fmt::Display` for `ParseError` (CONFIRMED PRIMARY)

`src/error.rs` implements `core::fmt::Display for ParseError`, which calls
`core::fmt::Display::fmt(&self.span.start, f)` for `u32` formatting. This
transitively pulls in:

- Integer-to-string formatting machinery (`core::fmt::Formatter::write_fmt`)
- The digit lookup table (`0x00010203...99` — visible in the data section)
- `library/core/src/unicode/printable.rs` tables — the Display machinery for
  chars and strings brings in the Unicode printable-char lookup tables
- All the associated panic-message strings for string/slice operations:
  `"index out of bounds: the len is  but the index is "`,
  `"begin <= end ( <= ) when slicing"`,
  `"byte index N is not a char boundary"`, etc.

These are the dominant contributors to the overshoot. Even with
`panic = "abort"` in the profile, the *message strings* are baked into the
data section because they appear as static string literals in `core`, not as
unreachable panic branches. The `panic = "abort"` profile removes the
formatting machinery for user panics but does NOT strip the static `&str`
panic messages that `core::fmt` uses in its own bounds checks.

**Evidence:** The data section contains `library/core/src/unicode/printable.rs`
as a source location string and a large unicode printable table immediately
following it (hundreds of bytes of packed bit-fields), plus the digit lookup
table `0123456789abcdef0001020304...99` (200 bytes).

### Cause 2: dlmalloc assertion strings (CONFIRMED SECONDARY)

dlmalloc v0.2.13 contains internal `assert!` macros:
- `"assertion failed: psize >= size + min_overhead"`
- `"assertion failed: psize <= size + max_overhead"`

These assertion strings appear in the data section alongside the dlmalloc
source path. Under `panic = "abort"` these assertions still fire (the panic
payload is discarded but the `assert!` condition is still evaluated and the
string is still present in the binary because dlmalloc builds its own static
data). This is a known issue with `panic = "abort"` in Rust: it removes
formatting from *user* panics, but third-party `assert!` strings are baked in
as rodata.

Mitigation: build dlmalloc with `NDEBUG` equivalent — this can be done by
enabling dlmalloc's `"global"` feature only (already done) and ensuring the
build profile reaches it, OR switching to a different allocator.

### Cause 3: `Option::unwrap()` and slice panic strings (CONFIRMED TERTIARY)

The string `"called 'Option::unwrap()' on a 'None' value"` is in the data
section. The wasm shim's `gqlmin_alloc` and helper functions currently use
`Layout::from_size_align(...).unwrap()` in some paths (via `encode_ok`,
`encode_error`). Wait — I already replaced those with `match` + returning null.
The unwrap string must be coming from `core` internals used by the parser, OR
from dlmalloc. Even with `panic = "abort"`, the literal string is encoded as
a static `&str` in the relocation data; it cannot be stripped by the linker
unless LTO dead-strips it, which it evidently does not.

---

## Candidate causes ranked by estimated contribution

| # | Cause | Estimated contribution | Cheapest disprove/confirm experiment |
|---|---|---|---|
| 1 | `core::fmt::Display` impl in `error.rs` + unicode tables | ~8000 bytes gz | Remove the `Display` impl entirely (gate on `cfg(feature = "std")`) and rebuild |
| 2 | dlmalloc assertion strings | ~1500 bytes gz | Replace dlmalloc with `wee_alloc` and compare |
| 3 | `Option::unwrap` / slice panic messages from `core` | ~1000 bytes gz | Profile with `build-std` to dead-strip; or switch to `alloc::alloc::alloc` without any panicking paths |

---

## Proposed fix (cheapest first, in order)

### Fix 1 (HIGH IMPACT, LOW RISK): Gate `Display` impl behind `#[cfg(feature = "std")]`

`core::fmt::Display for ParseError` is only needed for host-side rendering.
The wasm build must not include it. The implementation already calls
`core::fmt::Display::fmt` for `u32` values, which is what drags in the
unicode tables and digit lookup table.

Action: wrap the `impl core::fmt::Display for ParseError` block in
`#[cfg(feature = "std")]`. The `std::error::Error` impl is already gated this
way. This should eliminate causes 1 and 3 almost entirely.

Experiment:
```bash
# In error.rs, add #[cfg(feature = "std")] to the Display impl block
# Then rebuild:
cargo build -p magna-gqlmin \
  --target wasm32-unknown-unknown \
  --no-default-features --features "ops,wasm" \
  --profile release-wasm
wasm-opt -Oz --strip-debug --vacuum \
  --enable-bulk-memory --enable-sign-ext \
  target/.../magna_gqlmin.wasm -o /tmp/gqlmin.opt.wasm
gzip -9 -c /tmp/gqlmin.opt.wasm | wc -c
```

Expected result: drops to ~4000–6000 bytes gz (eliminating ~8000 bytes of
unicode tables + formatting machinery).

### Fix 2 (MEDIUM IMPACT): Switch from dlmalloc to wee_alloc or a minimal allocator

If Fix 1 alone is insufficient, replace dlmalloc with `wee_alloc` (smaller
allocator, fewer assertions baked in) and measure delta.

### Fix 3 (OPTIONAL): Use `build-std` to dead-strip more aggressively

`cargo build ... -Z build-std=core,alloc` rebuilds the core + alloc libraries
with the same optimization profile, enabling LTO to dead-strip panic strings
and other unreachable code across crate boundaries. This requires nightly or
a `rust-src` component. This is a scope-shift trigger per the brief.

---

## User-visible API impact assessment

- **Fix 1** (`#[cfg(feature = "std")]` on `Display`): NOT user-visible for wasm
  consumers. Host-side Rust users (who have `std`) continue to get `Display`.
  The wasm binary already does not expose `Display` — it only exposes the
  `gqlmin_parse` function which returns a binary tag+payload. This fix is
  invisible to all current consumers. NOT a user-visible API change.

- **Fix 2** (allocator swap): not user-visible.

- **Fix 3** (`build-std`): scope shift — requires user decision per surface
  conditions.

---

## Conclusion

**Fix 1 is safe and should be attempted immediately.** The `Display` impl has
no business in the wasm build. Gating it behind `cfg(feature = "std")` is
consistent with the existing `std::error::Error` gate and removes the dominant
bloat source (unicode tables + integer formatting).

Fix 1 does NOT require user input and does NOT change any user-visible API.
It is therefore NOT a surface-to-user trigger. The Iron Law investigation is
complete; Builder R2 should proceed to implement Fix 1 and re-measure.
