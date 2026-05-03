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
