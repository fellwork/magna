# R6 audit — panic-on-bounds operations

Round: R6 phase 1 (audit) for rung 1 (Unicode/slice-panic elimination).
Catalogues every panic-on-bounds slice/index access on the parser hot
path. Phase 2 replaces each with an `Option`-returning equivalent.

## Hypothesis (from R5)

R5 traced ~3–4 KB gz of bloat to the Unicode `printable.rs` property
tables in `core::str`. Those tables are reachable from `core::str::Debug`
formatting, which is reachable from any `&str` slice operation that can
panic with a message like `byte index ... is not a char boundary`. The
fix is mechanical: replace every `self.src[s..e]` with
`self.src.get(s..e).unwrap_or("")`.

`&[u8]` indexing pulls in the simpler `core::slice::index` panic path
(no Unicode tables, but still some filename + format-string baggage).
Same fix shape: `self.bytes.get(i).copied().unwrap_or(0)` or pattern-match
`Some(&b) => …, None => …`.

## Catalog

### `crates/magna-gqlmin/src/lex.rs`

#### `&str` slice operations (highest-priority — pulls in Unicode tables)

| Line | Op | Notes |
|---|---|---|
| 112 | `&self.src[s..e]` in `slice()` | already defensively clamped; replace with `.get(s..e).unwrap_or("")` to fully eliminate the `core::str::index` panic edge |

#### `&[u8]` byte-index operations

All guarded by an explicit length check above. Replace each with
`get(i)`-shaped access so `core::slice::index_fail` is unreachable.

| Line | Op |
|---|---|
| 136 | `self.bytes[start]` |
| 180 | `self.bytes[self.pos]` (skip_insignificant outer) |
| 189 | `self.bytes[self.pos]` (comment scan) |
| 204 | `self.bytes[start + 1]` (lex_spread) |
| 205 | `self.bytes[start + 2]` (lex_spread) |
| 224 | `self.bytes[self.pos]` (lex_name) |
| 349 | `self.bytes[start + 1]` (block string open) |
| 350 | `self.bytes[start + 2]` (block string open) |
| 360 | `self.bytes[self.pos]` (block string body) |
| 363 | `self.bytes[self.pos + 1]` (block string `\"""`) |
| 364 | `self.bytes[self.pos + 2]` |
| 365 | `self.bytes[self.pos + 3]` |
| 372 | `self.bytes[self.pos + 1]` (block string `"""` close) |
| 373 | `self.bytes[self.pos + 2]` |
| 374 | `self.bytes[self.pos + 2]` |
| 395 | `self.bytes[self.pos]` (regular string body) |
| 419 | `self.bytes[self.pos + 1]` (regular string escape lookup) |
| 432 | `self.bytes[self.pos + i]` (\uXXXX hex digits) |
| 458 | `self.bytes[self.pos]` (peek_byte) |

Total: 1 `&str` slice, 19 byte indexes (some duplicated above —
collapsed in phase 2 by hoisting `get()` results).

### `crates/magna-gqlmin/src/parse/mod.rs`

| Line | Op | Notes |
|---|---|---|
| 526 | `&self.src[s..e]` in parser `slice()` | same shape as lex.rs L112 |
| 295 | `&self.nodes[i]` in `NodeSlice::Index` impl | replace with `match self.nodes.get(i) { Some(n) => …, None => panic_invariant() }` so the `core::slice::index` panic edge is unreachable; `panic_invariant()` is already format-free (`#[cold] panic!()` no message) |

### `crates/magna-gqlmin/src/wasm.rs`

No panic-on-bounds operations on the hot path. `from_utf8_unchecked` is
already in use; raw pointer ops (`copy_nonoverlapping`, `add`) never
panic. Allocator paths return `Result` from `Layout::from_size_align`.
No changes needed in this file.

### `crates/magna-gqlmin/src/error.rs`

No slice/index ops. `Display` impl uses `core::fmt::Display` for `u32`
which is gated behind `feature = "std"` and excluded from the wasm build.
No changes needed.

### Other (non-hot-path)

- `pretty.rs`, `validate.rs`, `napi.rs` — gated behind features not in
  the wasm build (`pretty`, `validate`, `napi`); irrelevant for size.

## `unwrap()` / `expect()` / `panic!()` / `assert!()` outside `#[cfg(test)]`

- `parse/mod.rs:397`: `panic!()` in `panic_invariant()` — **already
  format-free** (no message argument). Resolves to `wasm32 unreachable`
  under the panic-handler in `wasm.rs`.
- All `unwrap()`, `assert!`, `assert_eq!` are inside `#[cfg(test)]`
  modules in `lex.rs`. Not reachable by the wasm build.

## Phase 2 plan

For each `&str` slice → `self.src.get(s..e).unwrap_or("")`. For each
byte index → `self.bytes.get(i).copied().unwrap_or(0)` (when the
fallback is harmless because every match arm rejects 0) or a `match`
expression that returns directly on `None` (when the loop should
terminate or an error should be raised).

No new `unsafe`. No API changes. All replacements stay inside
`forbid(unsafe_code)` per `lib.rs` for non-`wasm.rs` modules.
