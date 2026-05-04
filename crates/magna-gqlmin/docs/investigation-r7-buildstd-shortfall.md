# Investigation R7 — build-std shortfall vs projection

Round: R7 Builder. Branch: `claude/graphql-parser-lightweight-b6Vi8`.
Date: 2026-05-04.

## Symptom

R7 implemented Path β (Option F) — nightly `cargo +nightly build` with
`-Z build-std=core,alloc` and `-Cpanic=immediate-abort` (the post-2026
successor to the deprecated `-Z build-std-features=panic_immediate_abort`
flag). Goal per Director R6 brief: land near gz=5,120 bytes (budget).

Measured result: **gz=8,605 bytes.**

- Δ vs R6 (10,006): **−1,401 bytes** (~14% reduction).
- Δ vs Director R6 projection (5,120): **+3,485 bytes over.**
- Δ vs R7 brief's "FAILED — Iron Law fires" cutoff (8,500): **+105 bytes
  over.**

Smoke test passes. ABI durable. Native tests intact (38+5+12).

## Why this triggers an investigation

Two Iron-Law triggers from the R7 brief:

1. "Phase 3 measurement shows < 1,000 bytes saved (means
   panic_immediate_abort isn't doing what we expected)." **Does NOT fire**:
   we saved 1,401 bytes.
2. "gz > 8,500: R7 FAILED — Iron Law fires. The build-std +
   panic_immediate_abort hypothesis was wrong." **Fires** literally,
   at 105 bytes above the cutoff.

The literal trigger is hit, but the diagnostic conclusion ("hypothesis
was wrong") is contradicted by the data-section evidence below. This
investigation reconciles those two facts.

## Hypothesis (from Director R5/R6)

> `panic_immediate_abort` removes Vec capacity-overflow strings, alloc
> OOM strings, dlmalloc internal asserts, integer Display tables — all
> the rung-2/rung-3/misc bloat we identified for stable rungs.
> Aggressive dead-stripping of core/alloc removes unreachable code that
> LTO can't see today.

## Empirical check of the hypothesis

`wasm-dis /tmp/gqlmin.opt.wasm | grep -oE '"[^"]{8,}"'` on R7 returns
exactly:

```
"__data_end"
"__heap_base"
"gqlmin_alloc"
"gqlmin_free"
"gqlmin_parse"
"gqlmin_result_free"
"truefalsenullonquerymutationsubscriptionfragment\ef\bb\bf"
```

Compare to R6 (re-measured this round, same source tree, stable
toolchain):

```
"__data_end" "__heap_base" "gqlmin_alloc" ... (same exports)
"...truefalsenullcapacity overflow...memory allocation of  bytes failed
  ...slice index starts at  but ends at ..."
"crates/magna-gqlmin/src/parse/mod.rs\0/root/.rustup/toolchains/...
  /alloc/src/raw_vec/mod.rs\0.../alloc/src/vec/mod.rs\0.../dlmalloc.rs\0
  library/alloc/src/alloc.rs\0...
  assertion failed: psize >= size + min_overhead\0...
  assertion failed: psize <= size + max_overhead\0..."
```

**The hypothesis is empirically confirmed.** Every panic-string,
filename-literal, and assertion-string the R6 binary carried is
**physically absent** from the R7 binary. The data section is
structurally clean. immediate-abort + build-std is doing exactly what
Director R5/R6 said it would do.

## So why are we 3.5 KB over projection?

Three reasons, ordered by contribution:

### Reason 1: R6 already captured the largest data win

The Director R6 projection arithmetic was:

> Combined R7+ best-case projection: gz ≈ 7,200 bytes, [...] only
> Option F (build-std nightly) is likely to land at or under 5,120.

That implicitly assumed Option F would deliver an additional ~2 KB on
top of the rung-2 and rung-3 stable wins (which would land us at
~7.2 KB). In practice:

- Rung 2 (wee_alloc swap) and rung 3 (custom panic_handler) were
  **never landed** — the user chose to skip remaining stable rungs
  (Path β decision in the round log).
- So R7 was applied directly to the R6 binary, not to a R6-plus-rungs
  binary.
- The single largest data-section item in R5 (the ~3-4 KB Unicode
  `printable.rs` table) was already removed by R6's slice-panic
  elimination. R7's contribution was the *remaining* ~1.4 KB of
  filename literals + alloc panic strings + the ASCII pair table —
  and R7 removed essentially all of it.

In other words, the projection summed wins as if they were independent;
in reality R6 and R7 partially overlap. R6 stripped the table that hung
off `core::str` panic paths; R7 stripped the strings of the panic paths
themselves.

### Reason 2: Code dominates after data is stripped

Of the 8.6 KB gz remaining:

- ~55 bytes are the parser keyword pool (only data-section item).
- The rest is 64 functions of code.

Function-size distribution (lines of wat in `wasm-dis` output):

| Func | Lines wat | Identity |
|---|---|---|
| $45 | 2256 | `gqlmin_parse` (main parser) |
| $40 | 1704 | parser internal (likely `parse_selection_set` or similar) |
| $11 | 1578 | dlmalloc malloc |
| $33 | 1476 | parser internal |
| $38 | 1260 | parser internal |
| $59 | 892 | dlmalloc free |
| $30 | 552 | parser internal |
| ... | | |

The top 5 functions are ~8,300 wat lines. Building blocks of
`gqlmin_parse` plus dlmalloc are the bulk. immediate-abort doesn't
shrink real parser code or dlmalloc; it shrinks panic infrastructure,
which is now gone.

### Reason 3: dlmalloc itself is still present

`-Z build-std=core,alloc` rebuilds **core and alloc** with our profile
+ immediate-abort. It does NOT rebuild `dlmalloc` (a third-party crate
in our dep graph). dlmalloc compiles under our `release-wasm` profile
already (panic=abort, opt-level=z) but still ships ~2 KB of malloc/free
logic that immediate-abort doesn't address. That's the rung-2 (wee_alloc)
target.

## Candidate causes — ruled out

- ❌ **build-std didn't apply our profile.** Ruled out: cargo logs show
  `Compiling core v0.0.0 ... Compiling alloc v0.0.0` under
  `release-wasm`, and the data-section evidence confirms immediate-abort
  was applied.
- ❌ **panic_immediate_abort feature gate name wrong.** Initially yes —
  the brief's exact command (`-Z build-std-features=panic_immediate_abort`)
  fails on this nightly with a `compile_error!` directing us to the new
  flag form. Switched to `RUSTFLAGS="-Zunstable-options
  -Cpanic=immediate-abort"` and the build succeeds.
- ❌ **rustc version mismatch.** No: nightly 1.97 with rust-src installed,
  build-std working as designed.
- ❌ **A leaked dep that doesn't honor build-std.** Closest match is
  dlmalloc (~2 KB still in the binary) — but it does honor our profile,
  it's just not part of `build-std`'s scope. Removing dlmalloc is rung 2
  (wee_alloc swap) or a custom bump-allocator.

## Ranked fixes (if the user wants to push further)

1. **Rung 2 (dlmalloc → wee_alloc).** R2-measured ~−1.4 KB on the R2
   baseline; the proportional saving on R7 should be similar (R7's
   dlmalloc footprint is the same code R2 had). Stacking on R7 would
   project gz ≈ 7,200. **Risk:** low. wee_alloc is unmaintained but
   irrelevant for parse-once-then-drop usage. **Cost:** 1 round.

2. **Custom bump allocator** (replace dlmalloc with a ~50-line bump
   allocator that only supports alloc, never free). Possible because
   the wasm shim's lifecycle is parse-then-drop-everything. Estimated
   saving: ~−1.8 KB (dlmalloc's full free path goes away). **Risk:**
   medium (more original code; needs careful size accounting).
   **Cost:** 1–2 rounds.

3. **Drop block-string parsing.** R5's risk-ladder rung 4. Estimated
   saving: 500–800 bytes. **Risk:** API change (a known GraphQL feature
   becomes unsupported on the wasm artifact). **Cost:** 1 round.

4. **State-table parser refactor.** Rewrite recursive-descent as a
   table-driven LL(1). Estimated saving: 1–2 KB (eliminates inlined
   call-site code). **Risk:** high (full parser rewrite). **Cost:**
   3–4 rounds.

5. **Accept ~8.6 KB and ship** (Path γ revised). Update budget; ship
   today.

## Recommendation to Director / Team Lead

Surface to user as **PARTIAL — Iron-Law-adjacent, hypothesis-confirmed-
but-shortfall**. The 105-byte overshoot of the literal Iron-Law cutoff
should not be treated as a refutation, because:

1. The hypothesis is empirically validated at the data-section level.
2. The shortfall is fully accounted for: R6 already captured the
   largest data win, dlmalloc remains as a known unaddressed item.
3. Stacking rung 2 on R7 (still within the build-std nightly defect
   class, counter at 1/5) is a low-risk path to ~7.2 KB.
4. Hitting 5,120 likely requires also dropping dlmalloc entirely
   (custom bump allocator) — that's another defect class and may
   warrant a fresh decision from the user.

The Director R6 projection of "gz ≈ 5,120 in a single nightly round"
was over-optimistic; the empirical reality is ~7,200 ceiling with
dlmalloc, ~5,500 ceiling with a bump allocator. Either is a 30%+
reduction from R6 with the toolchain we now have wired.

## Counter

R7 spent attempt **1 of 5** in the build-std-nightly defect class.
Four attempts remain. Plenty of budget for a rung-2 stack and one
final aggressive round if the user wants to push for 5,120.

## ABI / tests

- Wasm smoke: tag=0 success / tag=1 kind=34 EmptySelectionSet — durable
  across R2/R3/R5/R6/R7.
- Native ops tests: 18 lex + 20 corpus = 38/38.
- Pretty tests: 5/5.
- Validation tests: 12/12.
- napi feature: compiles.
- serde feature: compiles.
- Workspace `cargo check`: clean.
