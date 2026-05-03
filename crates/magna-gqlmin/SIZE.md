# magna-gqlmin size record

Updated by each R* round that touches the wasm build.

## R2 baseline

- rustc: `rustc 1.89.0 (29483883e 2025-08-04)`
- wasm-opt: `wasm-opt version 108` (binaryen, installed via `apt-get install binaryen`)
- Build command:
  ```
  cargo build -p magna-gqlmin \
    --target wasm32-unknown-unknown \
    --no-default-features --features "ops,wasm" \
    --profile release-wasm
  wasm-opt -Oz --strip-debug --vacuum \
    --enable-bulk-memory --enable-sign-ext \
    target/wasm32-unknown-unknown/release-wasm/magna_gqlmin.wasm \
    -o /tmp/gqlmin.opt.wasm
  ```
  Note: `--enable-bulk-memory --enable-sign-ext` required because rustc 1.89
  emits `memory.copy` and `i32.extend8_s` instructions that wasm-opt 108
  rejects without explicit feature flags.

- Pipeline:
  | Stage | Bytes |
  |---|---|
  | Raw `.wasm` (initial R2a baseline, before all fixes) | 38526 |
  | Post `wasm-opt -Oz --strip-debug --vacuum` (initial) | 33388 |
  | Post `gzip -9` (initial) | 15783 |
  | Raw `.wasm` (after Fix 1+2: gate `Debug`/`Display` + `from_utf8_unchecked`) | 37420 |
  | Post `wasm-opt -Oz --strip-debug --vacuum` (after Fix 1+2) | 32393 |
  | Post `gzip -9` (after Fix 1+2) | **15375** |

- Budget: 5120 bytes gz
- Status: ❌ over ceiling (15375 bytes gz; 3x over budget, 2x over 7 KB Iron Law ceiling)

## Risk-ladder rungs tried

| Rung | Action | gz bytes | Result |
|---|---|---|---|
| 0 (initial baseline) | dlmalloc 0.2, full ops parser with Vec, `from_utf8` | 15783 | FAIL — over Iron Law ceiling |
| 1 | Gate `Debug` derives behind `cfg_attr(any(std,test))` + gate `Display` behind `cfg(std)` | 15375 | Saves 408 bytes; still 3x over budget |
| 2 | Switch `from_utf8` → `from_utf8_unchecked` in wasm shim | included in rung 1 | Minor saving; part of rung 1 measurement |
| 3 | Switch global allocator from dlmalloc to wee_alloc 0.4 | 13978 | Saves 1805 bytes gz vs initial baseline; still 2.7x over budget |
| 4 | Prune AST kind enums | not tried | Estimated ~200 bytes saving — negligible given structural issue |
| 5 | Drop block-string parsing | not tried | User-visible API change; surface trigger per brief |
| 6 | Accept 7 KB ceiling | N/A | wee_alloc baseline (13978) still exceeds 7 KB ceiling |

## Root cause (summary)

The operations parser uses 7 distinct `Vec<T>` types (Definition, VariableDefinition,
Directive, Argument, Selection, ObjectField, Value::List). Each type monomorphizes
the full Vec grow/drop/realloc machinery, producing ~10 KB extra gz code versus the
architect's estimate of ~1.5 KB for the parser. The estimate implicitly assumed a
bump-allocated or span-indexed AST design without per-type Vec monomorphization.

The wasm binary is **functionally correct** (all 4 exports verified via smoke test;
tag=0 for valid docs, tag=1 kind=34 for empty selection set). Only the size budget
is not met.

See `docs/investigation-r2-wasm-size.md` for full analysis and proposed fix paths.

## Proposed fix paths for R3

1. **Fix A (bumpalo arena):** Replace `Vec<T>` with `bumpalo::collections::Vec<'bump, T>`.
   All list fields share one monomorphization. Estimated gz: 4000–7000 bytes.
   Requires new dep `bumpalo` (optional, under `feature = "wasm"`).
   API change: `Document<'src>` gains a second lifetime.

2. **Fix B (span-indexed flat arrays):** Major parser redesign — no new dep.
   Lists stored as index ranges into a flat backing array. Estimated gz: 3000–5000 bytes.

3. **Fix C (build-std dead stripping):** Requires nightly toolchain — scope shift.

## R3 (Option A — bumpalo arena) — 2026-05-03

- rustc: `rustc 1.89.0 (29483883e 2025-08-04)`
- wasm-opt: `wasm-opt version 108`
- Build command:
  ```
  cargo build -p magna-gqlmin --target wasm32-unknown-unknown \
    --no-default-features --features "ops,wasm" --profile release-wasm
  wasm-opt -Oz --strip-debug --vacuum --enable-bulk-memory --enable-sign-ext \
    target/wasm32-unknown-unknown/release-wasm/magna_gqlmin.wasm \
    -o /tmp/gqlmin.opt.wasm
  ```

- Pipeline:
  | Stage | Bytes |
  |---|---|
  | Raw `.wasm` | 43342 |
  | Post `wasm-opt -Oz --strip-debug --vacuum` | 37152 |
  | Post `gzip -9` | **17490** |

- Budget: 5120 bytes gz
- R2 baseline: 15375 bytes gz
- R3 result: **17490 bytes gz** — Δ = +2115 bytes vs R2 baseline (WORSE).
- Status: ❌ over 7,000-byte Iron Law ceiling. Iron Law fires.

### Why R3 grew the binary

The bumpalo migration successfully collapsed the 7-Vec monomorphization
problem (function count fell from 150 in R2 to 90 in R3, the 7 distinct
`RawVec::*` impls collapsed into one). However, two new contributions
overwhelmed that win:

1. **Bumpalo's panic paths pull in `core::str` Debug formatting and the
   Unicode `printable.rs` tables.** Inspection of the data section shows
   `library/core/src/unicode/printable.rs` and the full
   `byte index ... is not a char boundary; it is inside ... (bytes ...) of`
   panic message infrastructure are present, plus the `0x00..99` ASCII
   pair table and the Unicode property tables (~4 KB binary data).
   These were NOT in the R2 binary. The R2 binary used only `core::alloc`
   panics which have static, format-free messages.

2. **`bumpalo` crate code itself** (alloc.rs, raw_vec.rs, lib.rs) adds
   ~1 KB of grow/realloc/Layout-validation logic on top of `dlmalloc`.
   This is a fixed per-binary cost.

Function-count won (150 → 90), data-section lost (~5 KB of new strings
+ Unicode tables). Net is a binary that's 2 KB gz larger than R2.

### Verdict

R3 verdict: **OVER (Iron Law fires)** — see `docs/investigation-r3-bumpalo-panic-bloat.md`.
The Vec-monomorphization analysis was correct (and the 60-function
reduction proves it), but the assumption that bumpalo would be a
near-zero-overhead drop-in was wrong. Bumpalo's panic-formatting paths
are heavier than the Vec monomorphization they replaced.

Surfaced to user as BLOCKED. Candidate next moves listed in the
investigation doc.

## R5 (Option B — span-indexed flat arrays) — 2026-05-03

- rustc: `rustc 1.89.0 (29483883e 2025-08-04)`
- wasm-opt: `wasm-opt version 108`
- Build command:
  ```
  cargo build -p magna-gqlmin --target wasm32-unknown-unknown \
    --no-default-features --features "ops,wasm" --profile release-wasm
  wasm-opt -Oz --strip-debug --vacuum --enable-bulk-memory --enable-sign-ext \
    target/wasm32-unknown-unknown/release-wasm/magna_gqlmin.wasm \
    -o /tmp/gqlmin.opt.wasm
  ```

- Phase 1 baseline (revert bumpalo, restore Document<'src>, plain Vec<T>):
  | Stage | Bytes |
  |---|---|
  | Raw `.wasm` | 37254 |
  | Post `wasm-opt -Oz --strip-debug --vacuum` | 32221 |
  | Post `gzip -9` | **15298** |

- Phase 3 (span-indexed AST: ONE `Vec<Node>` arena per Document):
  | Stage | Bytes |
  |---|---|
  | Raw `.wasm` | 36017 |
  | Post `wasm-opt -Oz --strip-debug --vacuum` | 31273 |
  | Post `gzip -9` | **14895** |

- Function count:
  - R2 baseline: 150
  - R3 (bumpalo): 90
  - R5 phase 1 baseline: ~150 (Vec<T> back)
  - R5 phase 3 (span-indexed): **77**

- Budget: 5120 bytes gz
- Iron Law ceiling: 7000 bytes gz
- Status: ⚠️ PARTIAL — gz=14895 is BELOW R2 baseline (15375) AND below
  R3 (17490) but ABOVE the 7000-byte Iron Law ceiling and the 5120-byte
  budget.

### What R5 achieved

The structural fix (collapsing the seven distinct `Vec<T>` AST
collections into ONE shared `Vec<Node<'src>>` arena per `Document`)
landed cleanly:

- All 38 R1 tests still pass (18 lex + 20 corpus).
- All 5 pretty tests still pass.
- All 12 validation tests still pass.
- Wasm smoke ABI test still passes (tag=0 / tag=1+kind=34 unchanged).
- Function count fell from 150 (R2) → 77 (R5), confirming the
  monomorphization analysis was correct.
- No new external runtime deps (bumpalo removed; only `dlmalloc`
  remains under `feature = "wasm"`).
- Public API stays single-lifetime (`Document<'src>`).

### Why gz didn't drop further

The Vec-monomorphization collapse saved ~480 bytes gz vs the R5 phase 1
baseline (15298 → 14895). The remaining bloat lives in the data section,
NOT in the function table:

Data-section contents (extracted from `wasm-dis /tmp/gqlmin.opt.wasm`):

- Filename literals: `crates/magna-gqlmin/src/lex.rs`,
  `library/core/src/unicode/printable.rs`,
  `library/core/src/str/mod.rs`,
  `crates/magna-gqlmin/src/parse/mod.rs`,
  `library/alloc/src/raw_vec/mod.rs`,
  `library/alloc/src/vec/mod.rs`,
  `dlmalloc-0.2.13/src/dlmalloc.rs`,
  `library/alloc/src/alloc.rs`.
- Slice / str panic messages:
  `byte index ... is not a char boundary; it is inside ... (bytes ...) of`,
  `index out of bounds: the len is ... but the index is ...`,
  `slice index starts at ... but ends at ...`,
  `range end index ... out of range for slice of length ...`,
  `begin <= end ( <= ) when slicing`,
  `... is out of bounds of`.
- Allocation panics: `capacity overflow`, `memory allocation of ... bytes failed`.
- Hex / decimal lookup tables: `..0123456789abcdef`,
  the `0x00010203...99` two-digit ASCII pair table.
- Unicode property tables (`core::unicode::printable`):
  ~4 KB of binary data encoding `is_printable` /
  `is_printable_in_supplementary_planes`.
- dlmalloc internal asserts: `assertion failed: psize >= size + min_overhead`,
  etc.
- GraphQL keyword pool: `querymutationsubscriptionfragmenton`,
  `truefalsenull` (these are PARSER content — not bloat).

This is the SAME bloat pattern that killed R3's bumpalo migration —
panic strings + Unicode tables transitively reachable from any
slice-bounds-check or vec-grow panic site. The span-indexed rewrite
didn't introduce them; they're a baseline cost of using safe slice and
`Vec` operations on stable Rust without `build-std`.

### Next-largest bloat candidates (for R6+)

Ranked by estimated savings:

1. **Unicode `printable.rs` tables (~3-4 KB gz).** Reachable from a
   `core::str` Debug-formatting site that the parser body indirectly
   triggers. Mitigation: replace `self.src[s..e]` style indexing in
   `parse/mod.rs` and `lex.rs` with `self.src.get(s..e).unwrap_or("")`
   (or `.unwrap_unchecked()` under `#[cfg(feature = "wasm")]`); audit
   `Vec::extend(self.scratch.drain(...))` for similar reachability.
   These changes keep `core::str::is_char_boundary` reachable but stop
   `core::str::Debug` from dragging in the printable tables.
2. **dlmalloc → wee_alloc swap (~1.4 KB gz, R2 measured).** Already
   tested; would land us at gz≈13.5 KB.
3. **`core::str::from_utf8_unchecked` audit.** The wasm shim already
   uses `from_utf8_unchecked`; verify no other UTF-8 validation path
   in the parser/lexer is reachable.
4. **Custom panic-handler with `core::fmt::Write` shim.** Stub the
   format machinery so panic messages reduce to `unreachable`. Only
   helps if the format strings themselves are unreferenced after dead-
   stripping (likely partially effective on stable).
5. **`build-std` (Option F).** Requires user approval for nightly
   toolchain split; estimated to land near the 5,120 budget.

### Iron-Law check

R5 did NOT regress vs R2 or R3, so the Iron Law does not fire on R5.
The structural-fix counter advances to 2/5 (R3 was 1/5). Three
attempts remain on this defect class.

R5 verdict: **PARTIAL** — improvement landed and is durable on stable
Rust, but the absolute gz figure is still above both the budget and
the Iron Law ceiling. Surface to user with the next-largest bloat
identified; user picks the next rung (allocator swap, parser-body
panic-elimination audit, or Option F nightly build-std).
