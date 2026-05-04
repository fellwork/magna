# gqlmin — topic summary (living document)

Living document the Synthesizer updates after each Director routes findings.
Read this *first* before briefing any Researcher.

## Thesis

A new workspace crate `magna-gqlmin` that parses GraphQL with three
distribution modes from a single Rust source:

1. **wasm32-unknown-unknown** — operations-only, MUST be ≤5 KB gz post
   `wasm-opt -Oz` and `gzip -9`. This is the binding constraint.
2. **napi-rs** — same operations parser, called from Node/Bun.
3. **Native Rust** dependency — full SDL + validation, no size constraint,
   for build-time tooling (eventual SFC compiler consumer).

Integration is NEW AST + NEW consumers. The parser does NOT replace
`async-graphql-parser` inside `magna-serv`.

## Status

Session 1, round 0. Not yet started. Builder R1 to be dispatched.

## Crate spec (locked)

- **Name**: `magna-gqlmin`
- **Path**: `crates/magna-gqlmin/`
- **Stability**: `experimental`
- **Deps for default `ops + wasm`**: zero runtime deps. `dlmalloc` for the
  wasm allocator (chosen over `wee_alloc` for maintenance).
- **Crate-type**: `["rlib", "cdylib"]`
- **MSRV**: workspace pinned (`1.89.0`)

## Feature flag layout

```
default = ["ops", "std"]
ops          # always-on
sdl          # type-system definitions
validate     # requires sdl + std
pretty       # caret/line error rendering (std)
serde        # AST derives, used by napi
napi         # napi-rs binding
wasm         # pure no_std + extern "C" (NO wasm-bindgen)
wasm-bindgen # opt-in fatter wasm escape hatch
```

## Size budget (the constraint)

Target ≤5120 bytes gzipped. Architect estimate ~4450 bytes:

| Component | Bytes (gz) |
|---|---|
| Lexer | ~700 |
| Parser | ~1500 |
| AST | ~400 |
| Span helpers | ~150 |
| Error type | ~250 |
| Wasm glue | ~300 |
| dlmalloc | ~700 |
| Panic handler | ~50 |
| LLVM memcpy/memset | ~400 |
| **Subtotal** | **~4450** |

Build profile `release-wasm`: `opt-level = "z"`, `lto = "fat"`,
`codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`.

**Bans (enforced via review and clippy denies):** no `HashMap`, no
`format!`, no `regex`, no `serde` in runtime, no Unicode tables, no
`String` in AST, `#![no_std]` + `extern crate alloc` for wasm.

**Risk ladder (if 5 KB gz proves infeasible):**
1. Best: 4.5 KB with dlmalloc, full ops feature
2. Tight: switch to `wee_alloc`, prune AST `kind` enums
3. Over budget at 5.5–6.5 KB: drop block-string parsing
4. Hard ceiling 7 KB: ship at 7 KB, document in README; offer lexer-only
   build at <3 KB
5. Floor: lexer-only at <5 KB, parsing on the JS side

## AST shape (operations-only)

Span-based, zero-copy. `&'src str` for identifiers; numeric values stored
as unparsed lexemes (caller decodes). Public types: `Document`,
`Definition`, `OperationDefinition`, `FragmentDefinition`,
`OperationKind`, `Name`, `VariableDefinition`, `Directive`, `Argument`,
`SelectionSet`, `Selection` (Field|FragmentSpread|InlineFragment), `Field`,
`Type`, `NamedType`, `Value`, `StringValue`, `ObjectField`, `Span`.

## Public API

- `parse_executable_document(&str) -> Result<Document<'_>, ParseError>`
- `parse_schema(&str) -> Result<SchemaDocument<'_>, ParseError>` (sdl)
- `validate(doc, schema) -> Result<(), Vec<ValidationError>>` (validate)
- `ParseError { span: Span, kind: ParseErrorKind }` — `#[non_exhaustive]
  #[repr(u8)]` enum, static-string messages

**Wasm exports (no wasm-bindgen):** `gqlmin_alloc`, `gqlmin_free`,
`gqlmin_parse`, `gqlmin_result_free`. Custom binary AST encoding; JS-side
decoder is sibling `@magna/gqlmin-wasm` package, NOT in the wasm budget.

### Error code wire format

`ParseErrorKind` discriminants are partitioned by range so the JS-side
decoder (`@magna/gqlmin-wasm`) can branch without importing the Rust
enum:

- Lexer errors: `1..=5`
- Parser errors: `32..=43`

The gap is intentional. Rule for adding new kinds: append within the
correct range; do not fill the gap. JS decoders branch on `kind < 32`
to dispatch lex-vs-parse error rendering.

**Napi:** `parseExecutableDocument(src: string)` returning JSON via
`serde_json`. No size constraint.

## Parser strategy

Hand-written DFA lexer + LL(1) recursive-descent. Rejected lalrpop/pest
(runtime size) and nom/winnow (size variance under compiler updates).
Implements GraphQL Oct-2021 spec sections 2.1–2.12 (operations) and 3
(SDL, gated).

## Build pipeline

The wasm size-gate path uses **nightly** with `-Z build-std` (workspace
`rust-toolchain.toml` stays on stable; nightly is invoked only for the
wasm-only target):

```bash
RUSTFLAGS="-Zunstable-options -Cpanic=immediate-abort" \
cargo +nightly build -p magna-gqlmin --target wasm32-unknown-unknown \
  --no-default-features --features "ops,wasm" \
  --profile release-wasm \
  -Z build-std=core,alloc
wasm-opt -Oz --strip-debug --vacuum --enable-bulk-memory --enable-sign-ext \
  <in> -o <out>
gzip -9 -c <out> | wc -c   # MUST be < 5120
```

Nightly toolchain prerequisite: `rustup toolchain install nightly
--component rust-src` plus `rustup +nightly target add
wasm32-unknown-unknown`.

**Note (post-2026-05 nightly):** the older `-Z
build-std-features=panic_immediate_abort` invocation no longer compiles;
core now emits a `compile_error!` directing consumers to
`-Cpanic=immediate-abort` via RUSTFLAGS (which we use, to keep the
workspace `Cargo.toml` profile shared with stable native builds).

CI gate `gqlmin-size`: fails the PR if gzipped wasm exceeds 5120 bytes.

## Test corpus (20 named cases)

`simple_query`, `named_query`, `query_with_variables`, `mutation`,
`subscription`, `fragment_definition`, `fragment_spread`,
`inline_fragment_with_type`, `inline_fragment_no_type`,
`nested_directives`, `field_alias`, `arguments_all_value_kinds`,
`non_null_list_type`, `default_value`, `multiple_operations`,
`block_string_arg`, `comments_and_commas`, `unicode_in_strings`,
`empty_selection_error`, `unterminated_string_error`.

Each test compares AST against a snapshot. Use a hand-written `Debug` impl
or a minimal serializer; do not add `insta` as a dependency.

## Build-order (12 steps; step 11 deferred)

1. Skeleton + workspace registration
2. Feature flag matrix + `check-features.sh`
3. Lexer
4. Ops parser + corpus
5. Wasm build + SIZE.md baseline
6. CI size gate
7. napi binding
8. SDL parser
9. Validation rules (10 starter)
10. Pretty errors + serde derives
11. **DEFERRED** — SFC compiler doesn't exist
12. Publish prep — README, CHANGELOG, GOVERNANCE row

## Locked decisions (autonomous mode)

- SFC compiler: out-of-scope this session
- Crate name: `magna-gqlmin`
- AST stability: free-to-break in 0.x minors
- Wasm target: generic `wasm32-unknown-unknown`, browser-tuned
- Validation: 10 starter rules
- Wasm AST: span-indexed flat arrays, `Document<'src>` single lifetime (decided post-R3 after Option A refuted; R5 implements)
- Wasm build toolchain: stable-default + **Path β nightly build-std for wasm-only**
  (decided post-R6). Goal: land ≤5,120 bytes gz via `-Z build-std=core,alloc`
  + `-Z build-std-features=panic_immediate_abort`. Workspace `rust-toolchain.toml`
  remains stable; nightly is invoked only for the wasm size-gate target.
- R9 path: 9a-inline (parser helper merging, no API change)
  (decided post-R8). Block-string parsing preserved. Estimated gz
  5,750–6,050. State-table rewrite explicitly avoided due to risk
  vs counter-budget tradeoff.

## Structural size constraint (R2 finding)

Empirical measurement in R2 found the original 5,120-byte budget cannot be
met with `Vec<T>`-based AST collections on stable Rust. The 7 distinct
`Vec<T>` types in the parser (`Vec<Definition>`, `Vec<VariableDefinition>`,
`Vec<Directive>`, `Vec<Argument>`, `Vec<Selection>`, `Vec<ObjectField>`,
`Vec<Value>`) generate ~10 KB gz of monomorphized grow/drop/realloc code.
The Architect's 1.5 KB parser estimate implicitly assumed a non-Vec design.

Risk ladder exhausted in R2:

| Rung | Action | Measured gz |
|---|---|---|
| 0 baseline | dlmalloc, full ops parser | 15,783 |
| 1 | Gate Debug derives + Display behind cfg(std) | 15,375 |
| 2 | from_utf8_unchecked in wasm shim | (in rung 1) |
| 3 | Switch to wee_alloc | ~13,978 |
| 4 | Drop block-string parsing | not tried (API change) |
| 5 | Accept 7 KB ceiling | exceeded — N/A |

**Decision (locked by user, post-R2):** Option A — bumpalo arena. Replace
the 7 `Vec<T>` fields with `bumpalo::collections::Vec<'bump, T>`.
Document API gains a second lifetime: `Document<'src, 'bump>`. New optional
runtime dep: `bumpalo`. Estimated gz: 4,000–7,000 bytes (R3 will measure).

Wasm ABI confirmed working in R2 via Node smoke test:
- `simple_query.graphql` → tag=0 (success)
- `empty_selection_error.graphql` → tag=1, kind=34 (parse error)

This ABI is durable — future rounds must not break it.

## R3 outcome — bumpalo refuted

R3 measured the bumpalo migration (Option A): gz=17,490 bytes — **+2,115 bytes
worse than R2 baseline.** Iron Law fires (>7,000 byte ceiling).

**Durable lesson:** bumpalo is NOT a free win on the wasm-size axis even when
it correctly collapses Vec<T> monomorphizations. Bumpalo's `RawVec` and
`Bump::alloc_layout_*` panic paths reach `panic!`/`format_args!` call sites
that transitively pull in `core::str` Debug formatting and Unicode `printable.rs`
property tables (~3 KB gz of new strings + tables), plus the bumpalo crate
itself adds ~1 KB gz. `panic = "abort"` does not strip these — it skips
unwind, not format-string emission.

**Function count signal:** R3 confirmed the R2 Vec-monomorphization analysis
was correct (150 → 90 functions). The size regression came from new code
introduced by bumpalo, not from the Vec collapse failing.

**Rule for future rounds:** any arena-allocator candidate must be measured
for panic-path bloat BEFORE committing to migration. The size axis is
sensitive to transitive `core::fmt` reachability, not just data-structure
choice.

| Round | Approach | gz bytes |
|---|---|---|
| R2 baseline (Vec, dlmalloc, Debug-gated) | original | 15,375 |
| R2 + wee_alloc swap | tuning | 13,978 |
| R3 (bumpalo arena, Option A) | refuted | 17,490 |
| Iron Law ceiling | — | 7,000 |
| Original budget | — | 5,120 |

**Decision (locked by user, post-R3):** Option B — span-indexed flat arrays.
Replace `Vec<T>` collections with index ranges into shared backing buffers.
Public API stays `Document<'src>` (single lifetime). No new runtime deps.
No bumpalo. Implementation is parser-internals rewrite. Estimated gz: 3–5 KB
(per Director R3 analysis). R5 implements; tuning rounds R6+ as needed.

**Bumpalo dep status:** R5 must remove the `bumpalo` dep from Cargo.toml
along with the AST migration revert. The `dep:bumpalo` entry under the `ops`
feature must be removed.

## R5 outcome — Option B partial (structural collapse confirmed, budget gap remains)

R5 implemented span-indexed flat arrays (design 2B: single typed Node arena
in Document). All 7 distinct `Vec<T>` collections collapsed to ONE `Vec<Node>`,
plus a parser-internal scratch stack of the same type. Result: function count
150 (R2) → 77 (R5), confirming the Vec-monomorphization root cause. Public
API stays `Document<'src>` (single lifetime). All 38 corpus + 5 pretty + 12
validation tests pass. Wasm smoke test unchanged: tag=0 success, tag=1
kind=34 EmptySelectionSet — ABI durable across R2/R3/R5.

| Round | Approach | gz bytes | Δ vs prev |
|---|---|---|---|
| R2 baseline | Vec + dlmalloc + Debug-gated | 15,375 | — |
| R3 | bumpalo arena (refuted) | 17,490 | +2,115 |
| R5 | span-indexed Node arena | 14,895 | −2,595 vs R3, −480 vs R2 |

**Why the per-byte gain was modest:** LTO had already collapsed some of the
Vec monomorphizations on the R2 baseline. The 150→77 function-count signal
proves the structural change worked; the ~480-byte savings reflects the
incremental wins after LTO was already doing partial deduplication.

### Math reality after R5

Current: 14,895 bytes gz. Budget: 5,120. Gap: **9,775 bytes**.

Identified next-rung candidates from R5 (`docs/investigation-r5-span-indexed-design.md`
section "Next-largest bloat"):

| Rung | Mitigation | Estimated savings | Risk |
|---|---|---|---|
| 1 | Unicode/slice-panic elimination — replace `.src[s..e]` with `.src.get(s..e).unwrap_or("")` to break `core::str` Debug reachability | 3,000–4,000 bytes gz | Low (mechanical) |
| 2 | dlmalloc → wee_alloc swap | ~1,400 bytes gz | Low (one-line) |
| 3 | Custom panic-handler / fmt::Write shim | ~1,000 bytes gz (med-confidence) | Medium |
| 4 | Option F (build-std nightly, requires user approval) | likely lands ≤5,120 | Toolchain split |

Combined rungs 1+2+3 best-case: **−6,400 → ~8,495 bytes gz**. Still ~3.4 KB
over budget. Stable-toolchain rungs alone are unlikely to hit 5,120.

**Decision (per Director R5):** continue methodically through rung 1 in R6,
then re-assess with empirical data before the budget-vs-Option-F surface.
Counter advanced to **2/5** after R5 (3 attempts remain in the structural-fix
defect class).

## R6 outcome + Path β decision (build-std nightly)

R6 implemented rung 1 (Unicode/slice-panic elimination): replaced 17
byte-index panic-on-bounds reads + 1 `&str` slice in `lex.rs`, plus
1 slice + 1 `&Node` index in `parse/mod.rs`, with `.get(...).unwrap_or(...)`
patterns. Zero new `unsafe`, zero API changes, zero new deps.

| Round | Approach | gz bytes | Δ vs prev |
|---|---|---|---|
| R5 | span-indexed Node arena | 14,895 | — |
| R6 | + Unicode/slice-panic elimination | **10,006** | **−4,889** |

`wasm-dis` confirmed elimination of: Unicode `printable.rs` property tables,
filename literals from `library/core/src/str/mod.rs` and `lex.rs`,
"byte index … is not a char boundary", "range end index … out of range",
"index out of bounds: the len is …", "begin <= end ( <= ) when slicing".

### Math reality after R6 (empirically validated)

Combining the remaining stable rungs:

| Rung | Action | Estimated savings |
|---|---|---|
| 3 | Custom panic_handler / fmt::Write shim | ~1.0 KB |
| 2 | dlmalloc → wee_alloc swap | ~1.4 KB (R2-measured) |
| misc | Single slice msg + parse filename + ASCII pair table | ~0.4 KB |
| **Total stable best-case** | | **~2.8 KB** |

Projected stable-only landing: gz ≈ 7,200 bytes — still ~2,080 over the
5,120 budget and ~200 over the 7,000 Iron Law ceiling. **The 5,120 target
is not reachable on stable Rust.**

### Decision (locked by user, post-R6): Path β — Option F build-std nightly

Skip remaining stable rungs. Configure `cargo +nightly build` with
`-Z build-std=core,alloc` and `-Z build-std-features=panic_immediate_abort`
to dead-strip panic strings/Unicode tables/format-string machinery from
core/alloc itself. R7 implements + measures.

**Scope:** nightly is used ONLY for the wasm size-gate build path. The
workspace `rust-toolchain.toml` stays on stable; native test runs and
all other crate builds remain stable. The wasm build invocation in
`scripts/check-features.sh`, the build pipeline docs, and the
`gqlmin-size` CI workflow add a `+nightly -Zbuild-std=...` flag.

**Budget reset:** the work nature shifted (stable-only → stable +
nightly-for-wasm). New defect class: "build-std nightly migration".
Iteration counter resets to **0/5**. Five attempts available before
hard-stop on this class.

**Surface conditions for Path β:**
- Build-std fails on agent's nightly toolchain (rust-src component issue)
- gz still > 5,120 after `-Z build-std-features=panic_immediate_abort` —
  surface to user; further rung-2/rung-3 are still available but are
  this-class attempts not stable-class
- panic_immediate_abort breaks the wasm smoke test (ABI regression)

## R7 outcome — Path β partial (build-std + immediate-abort)

R7 implemented Path β: nightly toolchain (1.97.0-nightly) + `-Z build-std=core,alloc`
+ `-Cpanic=immediate-abort` (passed via RUSTFLAGS, not the original
`-Z build-std-features=panic_immediate_abort` form which was removed upstream).
The wasm-only nightly path is invoked from `.github/workflows/gqlmin-size.yml`
and `crates/magna-gqlmin/scripts/check-features.sh`. The workspace
`rust-toolchain.toml` stays on stable; native crate builds and tests are
unaffected.

| Round | Approach | gz bytes | Δ vs prev |
|---|---|---|---|
| R6 | + Unicode/slice-panic elimination | 10,006 | — |
| R7 | + build-std + immediate-abort (Path β) | **8,605** | **−1,401** |

`wasm-dis` confirmed elimination of: every panic message string, filename
literal, alloc/dlmalloc assertion message, and ASCII pair table. Only ~55
bytes of keyword-pool literal remain in the data section.

### Durable lesson — `-Z build-std=core,alloc` does NOT rebuild external deps

R7 confirmed that `-Z build-std=core,alloc` rebuilds the `core` and `alloc`
crates with the project's `release-wasm` profile + `-Cpanic=immediate-abort`,
but does NOT rebuild crates listed in `[dependencies]`. `dlmalloc` is an
external crate — it inherits the standard release behaviour and continues
to ship its filename + assertion strings until R8 replaces or removes it.

Implication for future rounds: panic-string elimination is incomplete until
the allocator is also replaced with a panic-free implementation. R8 attacks
this by replacing dlmalloc with a custom bump allocator inline in the wasm
shim.

### Flag-name change (adapted)

The original Path β brief specified `-Z build-std-features=panic_immediate_abort`.
On nightly 1.97.0+ that flag was removed; the successor is `-Cpanic=immediate-abort`
via RUSTFLAGS (with `-Zunstable-options`) or `panic = "immediate-abort"` in the
profile. R7 uses the RUSTFLAGS form so the workspace `Cargo.toml` profile
(shared with stable native builds) stays untouched. This is documented in
`SIZE.md` and in `.github/workflows/gqlmin-size.yml`.

### Top remaining bloat (R7 measurements)

The 8.6 KB binary is now ~80% compiled code, ~20% headroom in data:

| Region | Size (wat lines) |
|---|---|
| `gqlmin_parse` (top-level export) | ~2,256 |
| Parser internal function ($40) | ~1,704 |
| dlmalloc malloc | ~1,578 |
| Parser internal function ($33) | ~1,476 |
| Parser internal function ($38) | ~1,260 |
| dlmalloc free | ~892 |
| **Top 5 ≈** | **~8,300 wat lines** = bulk of binary |

Future rounds attack code, not data. dlmalloc is the single largest
non-parser chunk (~2,470 wat lines for malloc + free).

### Decision (Director R7): R8 = custom bump allocator

R8 replaces dlmalloc with a small inline bump allocator under
`feature = "wasm"`. Lifecycle: parse-once-then-drop, so a no-op `dealloc`
is correct. Estimated saving ~−1,500 to −2,000 raw bytes, ~−700 to −1,000
gz. Combined with R7's 8,605 → projected R8 gz ≈ 6,800–7,800.

**Iron Law for R8:** regression vs R7 (8,605) → BLOCKED.
**Surface decision:** continue autonomously to R8; surface to user after R8
with R6/R7/R8 three measurements in hand.

## Round log

> Defect class: "structural fix — bumpalo arena migration" started after R2 surface. Iteration counter reset to 0/5 per playbook (work nature shifted from "build to spec" to "fix structural mismatch").

| Round | Steps | Status | Evidence |
|---|---|---|---|
| R1 | 1–4 (skeleton, features, lexer, ops parser) | ✅ DONE | 18 lexer tests + 20/20 corpus; 5/5 valid spec probes + 3/3 invalid rejections; all bans honored |
| R2 | 5–6 (wasm shim, SIZE.md, CI gate, smoke) | ⚠️ PARTIAL | wasm builds + smoke passes (tag=0/tag=1+kind=34); 38 R1 tests still pass; gz=15375 vs 5120 budget — surface to user; user chose Option A (bumpalo arena) |
| R3 | Option A: bumpalo arena migration (structural fix) | ❌ BLOCKED (Iron Law) | gz=17,490 (Δ=+2,115 vs R2). Vec collapse worked (150→90 fns) but bumpalo's panic paths pulled in core::str fmt + Unicode tables (~3 KB) + bumpalo crate (~1 KB), exceeding the savings. ABI/tests intact. |
| R4 | Step 7 (napi scaffold) + step 9 partial (5 of 10 ops-only validation rules) + step 10 partial (pretty errors, serde feature scaffold) — parallel with R3 | ✅ DONE | 5 pretty + 12 validation + 1 serde-smoke tests; all feature combos compile; threaded `Document<'src, 'bump>` lifetime via 1 fix-up commit |
| R5 | Option B: span-indexed Node arena (revert bumpalo + rewrite) | ⚠️ PARTIAL | gz=14,895 (Δ=−480 vs R2; −2,595 vs R3). Function count 150→77 confirms structural collapse. ABI/tests intact. Below R2 baseline but ~9.7 KB over budget. |
| R6 | Rung 1: Unicode/slice-panic elimination | ⚠️ PARTIAL | gz=10,006 (Δ=−4,889 vs R5). Eliminated Unicode `printable.rs` tables, char-boundary panic strings, slice-bounds messages (verified by wasm-dis). 38+5+12 tests pass; ABI durable. Beat the 3–4 KB estimate by ~1 KB. |
| R7 | Path β: nightly build-std `core,alloc` + `-Cpanic=immediate-abort` (RUSTFLAGS) | ⚠️ PARTIAL | gz=8,605 (Δ=−1,401 vs R6). All panic strings + Unicode tables + ASCII pair table physically eliminated from data section (verified by wasm-dis). Hypothesis confirmed. Director ruled PARTIAL not BLOCKED — the 8,500 Iron-Law cutoff was a projection-quality check whose conclusion is contradicted by direct evidence. |
| R8 | Custom bump allocator (replaces dlmalloc) | ⚠️ PARTIAL | gz=6,254 (Δ=−2,351 vs R7). Wasm distribution now has zero runtime crate deps. R2→R8 = 59% reduction. Top 4 functions = 85% of .text (parser body). 1,134 over budget. |

## R8 outcome + 9a-inline decision (parser helper merging)

R8 replaced dlmalloc with a custom 256 KiB inline bump allocator under
`feature = "wasm"`. Lifecycle: fill-then-rollover (no in-call reset to
avoid clobbering source bytes that `gqlmin_alloc` writes at offset 0
before `gqlmin_parse`). The wasm distribution `[dependencies]` block
is now zero runtime crate deps.

| Round | Approach | gz bytes | Δ vs prev | Cumulative vs R2 |
|---|---|---|---|---|
| R2 baseline | Vec + dlmalloc + Debug-gated | 15,375 | — | 0% |
| R5 | span-indexed Node arena | 14,895 | −480 | −3% |
| R6 | + Unicode/slice-panic eliminated | 10,006 | −4,889 | −35% |
| R7 | + nightly build-std + immediate-abort | 8,605 | −1,401 | −44% |
| R8 | + custom bump allocator (zero deps) | **6,254** | **−2,351** | **−59%** |

Function count: 150 (R2) → 33 (R8) = −78%.

Top remaining bloat (top 4 functions = 85% of .text):

| Function | wat lines |
|---|---|
| `$27 gqlmin_parse` (top-level export) | 2,256 |
| `$22` parser internal | 1,704 |
| `$15` parser internal | 1,476 |
| `$20` parser internal | 1,260 |

Data section: ~55 bytes (parser keyword pool only).

### Decision (locked by user, post-R8): 9a-inline

User chose **9a-inline** from the R9 options matrix: merge small
parser helper functions to reduce monomorphization overhead. **No
API change** (block-string parsing preserved). Estimated gz:
5,750–6,050 — likely close-miss territory but lowest risk among
budget-attacking options.

R9 implements; Director R9 assesses post-measurement and recommends
ship-or-iterate.

**Iron Law for R9:** regression vs R8 (6,254) → BLOCKED.

**Surface plan:** Director R9 surfaces with R6/R7/R8/R9 progression.
If R9 hits ≤5,120 → topic complete. If 5,121–6,050 → close-miss;
user chooses to ship-as-is, attempt 9b (block-string drop, API change),
or revise budget. If >6,050 → inlining didn't yield as expected;
re-evaluate.

Counter: 2/5 → 3/5 after R9 + Verifier. Two attempts remain in the
build-std-nightly defect class.

## Open questions

- R9 measurement-pending: actual gz post-9a-inline. Projected
  5,750–6,050. If under 5,120 → topic complete on size axis (will
  not happen per Director estimate but possible). If 5,121–6,050 →
  surface to user for ship/9b/revise-budget call.
- After R9: surface with R6/R7/R8/R9 four-data-point progression.
- Hard-stop awareness: build-std-nightly class advances to 3/5 after R9.
- Future deferred work (post-budget): SDL parser (step 8), 5 more
  validation rules, real napi #[napi] body, AST serde derives.

## Latest director-note

(none yet — round 0)
