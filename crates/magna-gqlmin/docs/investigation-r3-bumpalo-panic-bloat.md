# Investigation R3 — bumpalo panic bloat

## Symptom

Measured gz size after the R3 bumpalo arena migration: **17,490 bytes** —
WORSE than the R2 baseline of 15,375 bytes. The 5,120-byte budget is
3.4x exceeded; the 7,000-byte Iron Law ceiling is 2.5x exceeded.

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
# raw=43342 opt=37152 gz=17490
```

Toolchain: rustc 1.89.0 (29483883e 2025-08-04), wasm-opt version 108,
bumpalo 3.20.2.

---

## What the migration did right

Function count fell from **150 (R2) to 90 (R3)** — a 40% reduction. The
seven distinct `Vec<T>` monomorphizations identified in R2 collapsed
into a single bumpalo `RawVec` set. This confirms the R2 root-cause
analysis was correct: the per-`T` `RawVec::grow_one` /
`try_reserve_for_push` / drop glue accounted for ~60 functions, all of
which are gone.

Smoke test still passes (tag=0 for valid documents, tag=1+kind=34 for
empty selection sets). All 38 native tests (18 lexer + 20 corpus) pass.
The wasm ABI is unchanged.

---

## Root-cause diagnosis

### Cause 1: `bumpalo` panic strings pull in `core::str` Debug + Unicode tables (PRIMARY — ~5 KB raw / ~3 KB gz)

Examination of the binary's data section via `wasm-dis` shows the
following large strings/tables that were NOT present in the R2 binary:

- `library/core/src/unicode/printable.rs` (filename literal)
- `library/core/src/str/mod.rs` (filename literal)
- The `byte index ... is not a char boundary; it is inside ... (bytes ...) of`
  panic message
- The `0x00010203...99` two-digit-pair lookup table for `Debug`
  formatting `&[u8]` slices
- The Unicode property tables embedded in
  `core::unicode::printable::{is_printable, is_printable_in_supplementary_planes}`
  (~4 KB of binary tables)
- `slice index starts at ... but ends at ...` and similar slice-bound
  panic messages

The chain: `bumpalo`'s `RawVec` (and the inner `Bump::alloc_layout_*`
fallback paths) panic with messages like `"capacity overflow"` and
`"requested allocation size overflowed"` formatted via `panic!` /
`format_args!`. These call sites reference `&str` literals in static
data and embed line/column information from the call site. The compiler
includes the Unicode `printable` tables because somewhere in the
bumpalo code path a `Debug`-formatted `&str` is reachable from a panic
site.

Even with `panic = "abort"`, the formatting code is reachable because
the panic infrastructure must format the message before calling the
panic handler. `panic = "abort"` only aborts the unwind/cleanup phase;
it does not strip `format_args!` from panic call sites.

### Cause 2: `bumpalo` library code (~1 KB gz)

The bumpalo crate itself contributes:
- `bumpalo::Bump::new`, `Bump::alloc_layout_*`, chunk-list traversal
- `RawVec::grow_amortized`, `try_reserve_for_push`, etc.
- `Layout::from_size_align` validation paths

Net: roughly +1 KB gz of new code that is shared across all 7 list
types (which is a win for monomorphization but a loss in absolute terms
because R1's hand-written parser stored most state on the stack).

### Cause 3: `Bump::alloc::<T>` for `Type` nodes (NEGLIGIBLE)

The change `Box<Type>` → `&'bump Type` collapses to a single bumpalo
allocation per `Type::List`/`Type::NonNull` wrapper. This is a tiny win
and not measurably significant in either direction.

### What was assumed in the brief vs. what's true

The Director's R2 brief estimated **4,000–7,000 bytes gz** for the
bumpalo migration with "medium confidence — the bumpalo overhead and
residual allocator size need measurement". The estimate was off by 2.5x
on the high end. The error: the brief modeled bumpalo as a Vec-shaped
collection with a small fixed-cost allocator, but did not account for
bumpalo's `panic!`-with-format paths which transitively pull in
`core::fmt` machinery + Unicode tables. This is a known wasm-size
gotcha but was not captured in the R2 risk ladder.

---

## Candidate next moves (ranked)

### Candidate 1: Fix B from the original ladder (span-indexed flat arrays) — RECOMMENDED

Implement the parser-rewrite path the Architect's estimate originally
assumed. No external dep; AST holds index ranges into a flat backing
buffer; no `core::fmt` reachability from list operations.

**Estimated gz: 3,000–5,000 bytes** (per R2 director note). Confidence:
medium-high — this is the design the original 1,500-byte parser estimate
implicitly used.

**Cost:** full parser rewrite. Higher Builder effort than R3 was, but
the parser logic is known (the recursive-descent productions are stable
in `crates/magna-gqlmin/src/parse/mod.rs`); only the AST representation
and the storage need to change. Testing is unchanged — corpus tests
assert structural properties of the AST and would port forward with the
same `&'src str` borrows.

**Risk:** if the flat-array design itself has bookkeeping panic paths
(e.g. Range/index bounds checks that aren't statically eliminable), we
may end up in a similar bind. Mitigated by writing the index access
helpers with `unsafe { get_unchecked }` after invariants are verified
by the parser.

### Candidate 2: Replace `bumpalo` with a hand-rolled bump arena — UNCERTAIN

Write a 50-line bump arena with `panic_handler`-friendly error paths
(no formatted panics, no `Debug`-reachable code). This recovers the
monomorphization win of bumpalo without its formatting overhead.

**Estimated gz: 5,000–8,000 bytes.** Confidence: low. The Vec
monomorphization is an inherent property of having a `RawVec` type at
all — even a hand-rolled bump arena would still produce one set of
grow/drop/realloc functions per element type unless we also flatten the
collection layout. So this only saves the bumpalo panic-format
contribution (~3 KB) which would put us in the 14 KB ballpark, still
over budget.

### Candidate 3: Revise the budget per Option C — ESCAPE HATCH

The 5,120-byte target may be infeasible without nightly `build-std`. R2
already proposed:
- 8,192-byte ceiling for the full ops parser
- 3,072-byte lexer-only build for the strictest consumers

This trades architectural purity for shipping speed. If the user's real
constraint is "small enough to ship in a CDN-cached static script", an
8 KB target is still acceptable for most deployments.

### Candidate 4: `build-std` (Fix C from R2) — SCOPE SHIFT

`cargo build -Z build-std=core,alloc` rebuilds the standard library
with the same LTO settings as the application crate, allowing the
unicode tables and the format machinery to be dead-stripped if no
non-test panic site references them. Requires nightly toolchain or
`rust-src` component.

This is a known scope shift trigger. Worth raising with the user as the
fastest path to "5 KB on stable was a soft constraint; we'll allow
nightly".

---

## Decision point for Director / user

This R3 attempt at Fix A produced a binary that's WORSE than the R2
baseline. The structural fix (collapsing Vec monomorphization) was
correctly identified and successfully applied — it's just that bumpalo
brings its own bloat that more than offsets the win.

Ranked recommendation:

1. **Most likely to land budget:** Fix B (span-indexed flat arrays).
   No new dep, no external crate's panic paths, full control over
   what's in the binary.

2. **Most likely to ship today:** Option C (revise budget to 8 KB).
   Accept R2's 15,375-byte baseline minus the wee_alloc swap (~14 KB)
   or invest one more allocator/strip pass to land near 8 KB.

3. **Worth trying if user accepts nightly:** Fix C (`build-std`).
   May reach 5 KB with no rewrite.

4. **NOT recommended:** further bumpalo tuning. The panic-format paths
   are inherent to bumpalo's API.

R3 outcome: **BLOCKED — Iron Law fires.** The defect class iteration
counter is now 1/5 (R3 was the first round on the structural-fix
class). Four rounds remain.
