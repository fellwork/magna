# Director R3 — magna-gqlmin

Round: R3 director note
Branch HEAD reviewed: `2b5eb33`
Builder commits: `c27b0af` (bumpalo migration), `10b6fbb` (Cargo/lib/wasm wiring),
`f851043` (borrow-checker fix), `2b5eb33` (R3 measurement + investigation).
Parallel R4 commits: `18f9a61` (serde scaffold), `4954ee7` (napi scaffold),
`4e23624` (5 ops-only validation rules).
Verdict: **BLOCKED — Iron Law fires.** Option A (bumpalo arena) is empirically refuted.
Surface condition active.

---

## 1. On-thesis assessment

R3 was on-thesis. Builder followed the locked plan (Option A — bumpalo arena), executed
it correctly, measured honestly, and invoked the Iron Law when the measurement showed
the migration made the binary worse instead of better. The migration is technically
sound: all 7 `Vec<T>` types collapsed into `bumpalo::collections::Vec<'bump, T>`, the
public type became `Document<'src, 'bump>`, the function count fell from 150 to 90
(confirming the R2 root-cause analysis was correct on the Vec axis), the smoke test
still passes (tag=0 / tag=1+kind=34, ABI unchanged), and all 38 R1 tests still pass.

The failure is **empirical evidence, not a Builder error.** Both the Architect's
plan and the R2 director recommendation assumed bumpalo would be a net win on the size
axis. The measurement disproved that assumption: bumpalo's panic paths transitively
pull in `core::str` Debug formatting and the Unicode `printable.rs` tables (~3 KB gz
of new strings + tables), and the bumpalo crate itself adds ~1 KB gz. Those new costs
exceed the ~2 KB the Vec-monomorphization collapse saved. Net delta: **+2,115 bytes
gz worse than R2** (gz=17,490 vs R2 baseline 15,375). The 7,000-byte Iron Law ceiling
is now exceeded by 2.5x.

This is what experiments are for. R3 produced a durable lesson with a concrete
measurement. The plan-of-record needs to be updated with the disproof, not retried.

**Is the ≤5 KB gz thesis still achievable?** Conditionally yes, but only via paths
that do NOT route through bumpalo. The Vec-collapse insight is still valid (60-function
reduction is real), so a panic-path-free arena (Option B span-indexed flat arrays, or
a hand-rolled bump arena combined with panic-string elimination) remains plausible.
The conservative honest answer is "achievable on stable only via parser rewrite (B),
or achievable on nightly via build-std (F); otherwise revise the target."

---

## 2. Routing for synthesis

The following findings from R3 are durable enough to enter the living summary:

**Add to summary:**

- **R3 measurement:** gz=17,490 (post Option A migration), Δ=+2,115 bytes vs R2
  baseline of 15,375. Option A empirically refuted on the size axis. Iron Law fires.

- **Bumpalo panic-path/Unicode-table bloat (DURABLE LESSON):** `bumpalo`'s `RawVec`
  and `Bump::alloc_layout_*` panic paths reach `panic!`/`format_args!` call sites
  that transitively pull in `core::str` Debug formatting and the Unicode
  `printable.rs` property tables (~4 KB binary tables in rodata) plus the
  `byte index ... is not a char boundary` family of slice-bound messages. Even
  `panic = "abort"` does NOT strip these — abort only skips unwind/cleanup, it does
  not eliminate the format strings or unicode tables that the panic call site
  references. **Bumpalo is therefore NOT a free win on the wasm-size axis even when
  it correctly collapses Vec monomorphizations.** Future maintainers considering an
  arena-backed AST need to know this gotcha and prefer either span-indexed designs or
  a hand-rolled arena with `core::fmt`-free error paths.

- **Function-count signal:** 150 (R2) → 90 (R3), a 40% reduction. This confirms the
  R2 Vec-monomorphization analysis was correct. The structural insight is durable;
  it's only the choice of `bumpalo` as the implementation vehicle that fails.

- **Locked decision update:** the summary's "Wasm AST: bumpalo-arena" locked decision
  must be **unlocked** by the Synthesizer. The decision now reverts to "open — pick
  among Options B / C / D / E / F."

**Keep in investigation doc only (not summary-worthy):**

- The specific data-section string list from `wasm-dis` — captured in
  `investigation-r3-bumpalo-panic-bloat.md`, sufficient for any future re-attempt.
- The borrow-checker fix in `wasm.rs` (let-binding the parse result) — already in
  the commit message at `f851043`.

---

## 3. Updated risk-ladder + next-path recommendation

Option A is empirically ruled out. The user now faces a NEW decision among the
remaining options. The evidence has shifted: we now know bumpalo costs ~2 KB gz
beyond what its monomorphization collapse saves, and we now have a sharper picture
of where panic-path bloat lives.

### Option B — span-indexed flat arrays (parser rewrite, no new dep)

Replace `Vec<T>` with index ranges into one or two flat backing buffers (`Vec<u8>`
arena holding type-erased nodes, plus a small index table per node kind). Each AST
node holds `(start, len)` integer pairs.

- **Estimated gz: 3–5 KB.** Confidence: medium-high. This is the design the
  Architect's original 1.5 KB parser estimate implicitly assumed.
- No external dep. No `core::fmt` reachability from list operations. No unicode
  tables (panic paths can be made format-free by using `unsafe { get_unchecked }`
  for hot indexing, after invariants are proved by the parser).
- Public API can stay `Document<'src>` (single lifetime) — the flat arena lives
  inside `Document` itself.
- **Risk:** if the index access helpers retain bounds-check panic paths (e.g., due
  to `Range`-based slicing that the optimizer cannot prove safe), we may end up in
  a similar bind. Mitigated by writing helpers as `#[inline] unsafe fn` on top of
  `get_unchecked`, with debug-only assertions.
- **Cost:** full parser rewrite. Highest implementation effort of the live options,
  but the recursive-descent productions are stable in `src/parse/mod.rs` — only the
  AST representation and storage change. Test corpus ports forward unchanged
  (corpus asserts structural properties of the AST, which still hold).

### Option C — revise the size target

Accept that 5,120 bytes gz is infeasible for a full ops parser on stable Rust without
`build-std`. Reset the budget honestly:

- **Full ops parser:** revise to ≤14 KB gz (achievable today on R2-with-wee_alloc;
  realistic ≤8 KB only after additional structural work, which means we still need
  Option B or D anyway).
- **Lexer-only build:** offer at ≤3 KB gz for size-strict consumers (parsing on the
  JS side).

This trades architectural purity for shipping speed and avoids further structural
work. It's the honest version of "we tried, the Iron Law ceiling was real."

### Option D (NEW) — eliminate panic-path strings via panic_immediate_abort or rust_eh_personality stub

R3's investigation makes one path newly attractive: the bloat is dominated by
panic-path `&str` literals + Unicode tables. If we eliminate those at the link
level, bumpalo becomes affordable.

- **Mechanism:** the unstable `panic_immediate_abort` feature (requires nightly +
  build-std) inserts `core::intrinsics::abort()` at every panic site, allowing the
  format strings and unicode tables to be dead-stripped. A stable workaround is to
  override the panic handler with `extern "C" fn rust_panic_handler(_: &PanicInfo) -> !`
  that calls `core::arch::wasm32::unreachable()` directly, combined with disabling
  formatting via a `core::fmt::Write` shim — but this only helps if the `&str`
  literals themselves are unreferenced after dead-stripping, which on stable
  without build-std they typically are NOT (they live in the panic site's caller).
- **Estimated gz: could save 3–5 KB from the current 17,490 figure → ~12–14 KB.**
  Confidence: low-medium on stable; medium on nightly (where it converges with
  Option F).
- **Combination potential:** Option D combined with Option B (span-indexed) might
  land under the 5 KB budget where neither alone does.
- **Risk:** stable-toolchain efficacy is uncertain. Likely insufficient as a
  standalone fix on stable.

### Option E — revert R3, ship R2-with-wee_alloc at gz≈14 KB and document

Restore the R2 state (single-lifetime `Document<'src>`, `Vec<T>` AST), apply the
wee_alloc swap that R2 measured at gz=13,978, document the ceiling honestly in
README. Same effective tradeoff as Option C but without R3's API change.

- **Cost:** revert ~4 commits, swap allocator, update CI gate threshold.
- **Risk:** none on the size axis (already measured); the work nature shifts back
  to "ship at honest ceiling."

### Option F — `build-std` nightly + LTO of core/alloc (Fix C from R2)

`cargo +nightly build -Z build-std=core,alloc ...` rebuilds the standard library
with the same LTO settings, eliminating panic strings, unicode tables, and
unreached fmt machinery via dead-stripping.

- **Estimated gz: could land near 5 KB** if structural design is reasonable.
- **Cost:** scope shift (toolchain change). Requires `rust-src` component or
  nightly. May land in budget without additional rewrite.
- **Risk:** binds the project to nightly for wasm builds, which conflicts with the
  stable-MSRV stance elsewhere in the workspace. Acceptable only if user explicitly
  approves the toolchain split.

### Director's recommendation: Option B.

Option B has the strongest combination of:
- High confidence of landing in the 3–5 KB range (it is the design the Architect
  estimate implicitly assumed; all the new evidence from R3 is consistent with it).
- Stable toolchain (no scope shift to nightly).
- No new external runtime dep (eliminates the bumpalo panic-path failure mode and
  any equivalent failure modes from other arena crates).
- Acceptable API: can stay `Document<'src>` single-lifetime; even if it gains a
  second lifetime it's the same API change R3 already made and documented.

**Confidence:** medium-high that B lands ≤5 KB; high that B lands ≤8 KB. Risk is
implementation effort (full parser-internals rewrite, but corpus and ABI are stable).

If the user prefers shorter time-to-ship over reaching 5 KB, Option E is the honest
fallback — revert R3, ship at ~14 KB with wee_alloc, document the ceiling. Option F
(nightly build-std) is the right escape hatch only if the user is willing to bind
wasm builds to nightly.

I do **not** recommend retrying any further bumpalo-shaped path (Option D-as-bumpalo-rescue,
or hand-rolled arena variants). The R3 evidence shows the panic-path bloat is paid by
any arena whose error path reaches `core::fmt` — the only durable fix is to keep
`core::fmt` unreachable, which is what Option B's flat-array design does naturally.

---

## 4. Scope signal

**SURFACE.**

The user already chose Option A explicitly post-R2 (it was a locked decision in the
summary). That choice is now empirically refuted. Picking among B / C / D / E / F is
a decision the user owns, not one the Director should make autonomously: each option
shifts something the user previously locked (toolchain, target, API stability, or
delivery timeline). Section 5 below is the relay.

---

## 5. Surface note (relay-to-user content)

**— RELAY THIS SECTION TO THE USER VERBATIM —**

---

**magna-gqlmin R3 — Option A empirically refuted**

Here is what happened and the decision you need to make next.

**What R3 achieved (the migration is technically correct):**

The bumpalo arena migration is implemented and works. All 7 `Vec<T>` types in the
parser collapsed into `bumpalo::collections::Vec<'bump, T>`, the public type became
`Document<'src, 'bump>`, all 38 R1 tests still pass, and the wasm smoke test is
unchanged (tag=0 success / tag=1+kind=34 EmptySelectionSet — ABI is durable).
Function count fell from 150 to 90, confirming the R2 Vec-monomorphization analysis
was correct. R4 parallel work also succeeded against the new API: napi scaffold,
pretty-error tests, serde feature scaffold, and 5 of 10 ops-only validation rules
all compile under their feature gates.

**What R3 disproved (Option A was wrong on the size axis):**

R3 baseline gz: **17,490 bytes.** R2 baseline gz: 15,375 bytes. Delta: **+2,115
bytes worse, not better.** The 7,000-byte Iron Law ceiling is now exceeded by 2.5x;
the original 5,120-byte budget by 3.4x.

Root cause: `bumpalo`'s `RawVec` and `Bump::alloc_layout_*` panic paths reach
`panic!` / `format_args!` call sites that transitively pull in `core::str` Debug
formatting and the Unicode `printable.rs` property tables (~3 KB gz of new strings
+ tables), plus the bumpalo crate itself adds ~1 KB gz. Those new costs exceed the
~2 KB the Vec-monomorphization collapse saved. `panic = "abort"` does not strip
these — abort only skips unwind, it does not eliminate format strings or unicode
tables. **Bumpalo is not a free win on the wasm-size axis even when it correctly
collapses Vec monomorphizations.** This is the durable lesson from R3.

**Current numbers vs targets:**

| | gz bytes |
|---|---|
| R3 (bumpalo, Option A) | 17,490 |
| R2 baseline (Vec, dlmalloc, Debug gated) | 15,375 |
| R2 with wee_alloc swap | 13,978 |
| Iron Law ceiling | 7,000 |
| Original budget | 5,120 |

**Five remaining options — choose one:**

**Option B — span-indexed flat arrays (parser rewrite, no new dep).** Replace
`Vec<T>` with index ranges into a flat backing buffer. Estimated gz: 3–5 KB.
Stable toolchain. No new runtime dep. No `core::fmt` reachability from list
operations. Public API can stay `Document<'src>` single-lifetime. Cost: full
parser-internals rewrite (corpus and ABI port forward unchanged).

**Option C — revise the size target.** Accept ≤8 KB (or ≤14 KB) for the full ops
parser as the realistic stable-Rust ceiling; offer a ≤3 KB lexer-only build for
size-strict consumers. No further structural work needed beyond updating the CI
gate. Trades architectural purity for shipping speed.

**Option D — eliminate panic strings via `panic_immediate_abort` or panic-handler
override.** Aimed at killing the format strings and unicode tables that dominate
R3's bloat. Stable-toolchain efficacy is uncertain (the unstable feature requires
nightly+build-std and converges with Option F). Estimated saving: 3–5 KB from the
current 17,490 → ~12–14 KB. Likely insufficient alone for 5 KB; might combine with
Option B.

**Option E — revert R3, ship R2-with-wee_alloc at ~14 KB, document.** Restore the
R2 state (single-lifetime `Document<'src>`), swap to wee_alloc, document the
ceiling honestly in README. Same effective tradeoff as Option C but without R3's
API change. Lowest implementation effort.

**Option F — `build-std` nightly path.** Rebuild core/alloc with project LTO to
dead-strip panic strings/unicode tables. Estimated gz: near 5 KB. Cost: binds
wasm builds to nightly toolchain. Acceptable only if you approve a stable/nightly
split.

**Director's recommendation: Option B.** Highest confidence of landing in budget
on stable Rust without a toolchain split, no new dep, no further unknowns about
arena-crate panic paths. Cost is implementation effort, but corpus tests and the
wasm ABI port forward unchanged. If shipping speed matters more than 5 KB, Option E
is the honest fallback.

**What happens to other open work:** R4 parallel work has already shipped (napi
scaffold, pretty-error tests, serde scaffold, 5 ops-only validation rules — all
under feature gates). Steps 7–10 of the Architect plan are partially in flight via
R4 and not blocked by the size decision. SDL parser (step 8) and the remaining 5
validation rules + full validation (step 9 completion) are deferred to R5/R6 and
do not depend on which Option you pick — they live behind the `sdl` and `validate`
feature flags, neither of which is in the wasm size budget.

**What I need from you:** a decision on Option B / C / D / E / F. If you pick B,
also confirm you accept the rewrite scope for R4 (or R5, depending on R4 status).
If you pick E, confirm you want R3's commits reverted (vs. landed-and-documented).

---
*End of user-facing surface note.*

---

## 6. Iteration discipline

The structural-fix defect class is at **1/5 — R3 burned the first slot on Option A.
Four attempts remain before hard-stop on this class.**

Recommendation per option:

- **If user picks Option B:** stay within this defect class (continue on "fix
  structural mismatch"). R4-or-R5 brief scope = parser rewrite to span-indexed
  flat arrays, ABI preservation, corpus-tests-pass acceptance. Counter advances to
  2/5 on completion.

- **If user picks Option D:** stay within this defect class. Counter advances to
  2/5. Note that Option D is the lower-confidence path and may chain into a 3/5
  attempt if it does not land alone.

- **If user picks Option C or E:** **signal a budget reset.** The work nature
  shifts from "fix structural mismatch" back to "build to spec" with a relaxed
  budget. The structural-fix counter is closed at 1/5 (one attempt, refuted by
  measurement, fallback accepted). A new "build-out — relaxed-budget delivery"
  class begins at 0/5 covering the wee_alloc swap, CI-gate threshold update, and
  README documentation. This is consistent with the playbook's "work nature
  shifted" trigger.

- **If user picks Option F:** **flag scope shift.** Toolchain change requires user
  judgment beyond Director autonomy (stable/nightly split affects MSRV stance and
  CI). Counter advances within structural-fix class to 2/5 only after user accepts
  the toolchain split.

The 1/5 ceiling is healthy. R3 produced a real disproof (not a stalled attempt),
which is the highest-information outcome possible on a single round.

---

## 7. Continuity check

- **Did Builder revise targets silently?** No. R3 measured the post-migration gz
  honestly (17,490 bytes), reported BLOCKED, wrote a full root-cause investigation
  in `investigation-r3-bumpalo-panic-bloat.md`, and did not paper over the
  regression. SIZE.md was updated with both the R2 baseline and the R3 measurement
  side-by-side, including the "WORSE not better" delta. This is the correct
  behavior under the Iron Law.

- **Sample-level failures hidden?** No. The smoke test passes (tag=0 / tag=1+kind=34,
  ABI unchanged). All 38 R1 tests still pass. The failure is purely on the size
  axis; functional correctness is intact.

- **Acceptance items deferred?** No. R3 deferred nothing inside its own scope. The
  items still deferred (real napi binding body, AST serde derives, SDL parser,
  full validation rule set) are properly assigned to later rounds and tracked in
  R4's partial work. SFC compiler integration remains explicitly out-of-scope.

- **Work nature shift?** Possibly — depends on user choice in §5. Option B / D
  stay within the structural-fix class. Option C / E shift back to "build to spec"
  with a relaxed budget (counter reset). Option F shifts toolchain scope and
  requires explicit user approval.

- **Iteration ceiling?** 1/5 on structural-fix. Healthy. Four rounds remain.

- **Coordination concern (parallel agent activity):** R3 flagged write-retry
  friction during the session from concurrent R4 work. R4's commits all landed
  cleanly (one fix-up commit handled the `Document<'src, 'bump>` lifetime change
  in R4's code). R3's commits also all landed (`c27b0af`, `10b6fbb`, `f851043`,
  `2b5eb33`). No leftover merge conflicts or test breakage observed at HEAD
  `2b5eb33`. **Note for future rounds:** when parallel-agent work touches the same
  file (here: the AST type definitions), prefer serializing the structural change
  before launching parallel agents, or pre-declare the API contract so parallel
  agents can target it independently. Verifier R3+R4 should re-confirm no
  cross-contamination at next checkpoint.

- **Next Director note trigger:** after the user picks among B / C / D / E / F,
  and after the next Builder round completes its acceptance pass.
