# Director R5 — magna-gqlmin

Round: R5 director note
Branch HEAD reviewed: `031d4f5`
Builder commits this cycle: `039edbb` (phase 1 — revert bumpalo, restore
`Document<'src>`), `18048ea` (phase 2 — design note for span-indexed AST),
`e1fb0f1` (phase 3 — implement Option B span-indexed AST), `031d4f5`
(phase 4 — measurement + SIZE.md update).
Verdict: **PARTIAL — structural fix landed, durable, but still over budget
and over Iron Law ceiling.** Continue to R6 with a tuning rung; user-surface
deferred one round to land an empirical data point before forcing the budget
decision.

---

## 1. On-thesis assessment

R5 is on-thesis. The Builder followed the Option B brief precisely:

- **Phase 1** cleanly reverted the bumpalo migration (single-lifetime
  `Document<'src>` restored, `dep:bumpalo` removed from `ops`). Phase-1
  baseline measured at gz=15,298 — within rounding of the R2 baseline
  (15,375), confirming the revert was clean.
- **Phase 2** produced a written design note (`investigation-r5-span-indexed-design.md`)
  that evaluated three concrete arena shapes (2A type-erased byte arena,
  2B typed `Node` arena, 2C custom inline bump) and picked 2B with
  defensible reasoning. The choice of 2B over 2A is correct: data-section
  layout doesn't appear in `.text` size, so the enum-padding cost is
  irrelevant for the wasm budget while the API ergonomics gain is real.
- **Phase 3** implemented the typed-Node arena. Function count fell from
  150 → **77**, a 51% reduction — even better than the bumpalo arena's
  150 → 90 collapse, confirming the structural insight is sound and that
  Option B captures it without bumpalo's panic-path overhead.
- **Phase 4** measured honestly: gz=14,895, recorded in SIZE.md side-by-
  side with R2 and R3, with an unflinching "PARTIAL — below R2 (15,375)
  but ABOVE budget (5,120) AND above Iron Law ceiling (7,000)" verdict.
  The data-section bloat-source enumeration is concrete (filename
  literals, slice-bounds panic strings, dlmalloc asserts, Unicode
  printable tables) and points at testable next rungs.
- **No regression.** All 38 R1 + 5 pretty + 12 validation tests pass;
  wasm smoke ABI unchanged (tag=0 / tag=1+kind=34). Iron Law does NOT
  fire on R5 (Δ=−480 vs R2, well below the +0 regression line and far
  below the 7,000-byte ceiling overshoot threshold that R3 tripped).

The smaller-than-architect-estimated win (−480 gz versus the implied
Architect estimate that span-indexed alone would land in the 3–5 KB
range) is itself a useful finding: LTO had already partially collapsed
the seven Vec monomorphizations on R2's binary, so the *measurable*
delta from doing it explicitly via `Vec<Node>` is much smaller than the
*counterfactual* size delta (R2's 150 functions vs R5's 77). The bulk
of the residual gz lives in data-section panic strings + Unicode tables,
not in `.text`.

**Is the ≤5 KB gz thesis still achievable?** Stable-toolchain only:
**unlikely** with the rungs available. R5's own analysis estimates rungs
1+2+3 sum to ~6.4 KB savings → ~8.5 KB gz, still 3.4 KB over budget.
That gap is real and should be telegraphed early (see §3, §5).

---

## 2. Routing for synthesis

The following findings from R5 should enter the living summary:

**Add to summary:**

- **R5 measurement:** gz=**14,895** (Δ=−480 vs R2 baseline 15,375;
  Δ=−2,595 vs R3 17,490). PARTIAL — improvement is durable on stable
  Rust but still 9.7 KB over budget, 7.9 KB over Iron Law ceiling.
- **Function-count signal:** R2 baseline 150 → R3 (bumpalo) 90 → R5
  (span-indexed) **77.** Confirms the Option B structural collapse
  exceeds bumpalo's collapse, and confirms the residual bloat is in
  the data section, not in monomorphized code.
- **Stable-toolchain rung budget (math reality):** Even if the four
  next-rung candidates (Unicode/slice-panic elimination, wee_alloc swap,
  panic-handler/fmt::Write shim, manual core function elimination) all
  land their best estimates, projected gz lands ~8.5 KB — still 3.4 KB
  over the 5,120 budget. The 5,120 target on stable Rust without
  `build-std` is empirically unlikely.
- **Locked decision update:** Option B is implemented and durable. The
  next decision the user faces is "stable-toolchain rungs to ~8 KB +
  budget revision" vs "Option F nightly build-std to ~5 KB + toolchain
  split" vs "lexer-only fallback at <3 KB." Defer this to user surface
  after one more empirical rung lands (see §3).
- **Four next-rung candidates (R5 self-identified, ranked by estimated
  yield):** (1) Unicode `printable.rs` table elimination via
  `self.src.get(s..e).unwrap_or("")` substitution in parser+lexer hot
  paths, est. −3 to −4 KB; (2) dlmalloc → wee_alloc swap, est. −1.4 KB;
  (3) custom panic_handler / fmt::Write shim, est. small partial;
  (4) Option F build-std nightly, est. lands ~5 KB but binds wasm builds
  to nightly — requires user approval.
- **Iron Law status:** structural-fix defect class advances to **2/5**.
  Three rounds remain on this class.

**Keep in investigation doc only (not summary-worthy):**

- The `wasm-dis` data-section enumeration (filenames, panic literals,
  Unicode tables, GraphQL keyword pool) — already in SIZE.md R5 section.
- The 2A/2B/2C arena trade-off discussion — already in
  `investigation-r5-span-indexed-design.md`.
- The phase-1 baseline 15,298 vs R2 15,375 ~rounding-noise comparison —
  noise, not signal.

---

## 3. R6 recommendation

**Pick: Rung 1 (Unicode/slice-panic elimination).**

Rationale:

1. **Highest estimated yield of the stable rungs (−3 to −4 KB).** Lands
   us at ~11 KB if the estimate holds. That's the biggest single delta
   we can earn on stable without touching the toolchain.
2. **Low-risk, mostly mechanical.** The change is `self.src[s..e]` →
   `self.src.get(s..e).unwrap_or("")` (or `.unwrap_unchecked()` under
   `#[cfg(feature = "wasm")]`) in `parse/mod.rs` and `lex.rs`, plus an
   audit of `Vec::extend(self.scratch.drain(...))` and any other
   panic-on-bounds slice access. No API change, no data-structure
   change, no allocator change.
3. **Diagnostic value if it under-delivers.** If rung 1 measures less
   than ~2 KB savings, that's strong evidence the Unicode tables are
   reachable from a different site (e.g., dlmalloc panic, scratch-Vec
   indexing inside the parser) — and we'd know precisely where the next
   rung needs to attack. Either way, R6 produces actionable data.
4. **Sets up the surface decision.** After R6's measurement we'll know
   whether stable-toolchain rungs alone can plausibly land in budget,
   or whether we need to surface the Option F vs budget-revision choice
   with concrete numbers in hand. R5's projection (rungs 1+2+3 → ~8.5 KB)
   is currently a director estimate; one more empirical data point
   strengthens the surface decision considerably.

**Why not rung 2 (wee_alloc) instead?** Lower yield (−1.4 KB) and
already measured back in R2 — rung 2 produces no new information, just
shaves bytes. Better to land it as a follow-on after rung 1 in R7, or
as a combined small-tuning round once we know whether rung 1 hit its
estimate. Allocator swap is not strictly structural so combining is
defensible, but doing rung 1 *first* alone gives a cleaner attribution
of the Unicode-table elimination delta.

**Why not surface immediately (option c)?** The user said methodical
iteration, ONE rung per round, try to land on stable. Surfacing now
forces the budget decision on a director estimate (rungs 1+2+3 → ~8.5 KB)
rather than on measured data. R6 is a single round — one more empirical
data point is cheap and strictly improves the eventual surface
conversation. We're at 2/5 on the counter, with three rounds remaining.

**R6 brief scope (for Builder dispatch):**

- Replace `self.src[s..e]` → `self.src.get(s..e).unwrap_or("")` (or
  `.unwrap_unchecked()` gated on `cfg(feature = "wasm")`) in
  `crates/magna-gqlmin/src/parse/mod.rs` and `crates/magna-gqlmin/src/lex.rs`.
- Audit `Vec::extend(self.scratch.drain(...))` and any other indexing
  pattern (`scratch[i]`, `nodes[i]`, `&buf[..]`) that could be
  panic-on-bounds. Convert to `get`/`get_unchecked` patterns as
  appropriate.
- Verify post-change `wasm-dis` no longer references
  `library/core/src/unicode/printable.rs` strings.
- Re-measure gz; record in SIZE.md as "R6 (rung 1 — slice-panic
  elimination)".
- Acceptance: all 38 + 5 + 12 tests pass; wasm smoke unchanged
  (tag=0 / tag=1+kind=34); gz strictly less than R5's 14,895.
- If gz regresses or holds flat, REPORT and surface — that's a
  diagnostic about where the bloat actually lives.

---

## 4. Scope signal

**continue.**

Proceed with R6 (rung 1 — slice-panic elimination). The Team Lead
dispatches Builder R6.

The math reality (rungs 1–3 likely insufficient on stable) is real but
is currently a director estimate. One empirical data point in hand
before forcing the budget-vs-Option-F decision strictly improves the
surface conversation. Counter advances to 3/5 after R6 + Verifier R6;
two rounds remain after that.

---

## 5. Iteration discipline

- **Counter status.** Structural-fix defect class at **2/5** entering R6.
  After R6 + Verifier R6: **3/5**, two attempts remain. If we end at
  5/5 still over budget, hard-stop fires and we MUST surface.
- **Telegraph the math reality in R6 brief.** Yes — the R6 builder brief
  should explicitly call out the ~8.5 KB projection so the Builder knows
  this round's measurement is informative regardless of outcome (it
  either confirms the projection or refutes it — both are useful). The
  Team Lead should also flag in the R6 brief that if rung 1 lands its
  full estimate, the *Director's next note* (post R6 verifier) will
  proactively surface the budget-vs-Option-F decision rather than burn
  a round on rung 2.
- **Preemptive surface plan.** I recommend the *next* director note
  (post R6) surface to the user proactively if rung 1's measurement
  confirms the ~8.5 KB stable-only projection. We do NOT want to ride
  the counter to 5/5 silently and only surface at hard-stop. The user
  said "methodical iteration, ONE rung per round, try to land on
  stable" — that's a stable-first preference, but it doesn't preclude
  surfacing once we have empirical confirmation that stable cannot
  reach budget. Honest mid-course course-correction is preferred to
  silent attempt-counter exhaustion.
- **Surface trigger conditions for R6:** (a) Rung 1 lands its full
  3–4 KB estimate → next director note surfaces (with concrete numbers)
  the budget-vs-Option-F choice, framed as "we're now within striking
  distance of an honest decision; here's what stable can plausibly hit
  vs what nightly can hit." (b) Rung 1 under-delivers (<2 KB) → next
  director note pivots: the bloat lives elsewhere, propose a
  finer-grained investigation (twiggy, wasm-objdump) before another
  rung. (c) Rung 1 regresses → Iron Law fires, surface immediately.

---

## 6. Continuity check

- **Did Builder revise targets silently?** No. R5 measured honestly
  (gz=14,895), reported PARTIAL (not DONE), updated SIZE.md with the
  full R5 section side-by-side with R2/R3, and recorded the
  data-section bloat enumeration. No goalpost movement.
- **Sample-level failures hidden?** No. All 38 + 5 + 12 tests pass;
  wasm smoke ABI unchanged (tag=0 / tag=1+kind=34). Functional
  correctness intact across the structural rewrite — corpus tests caught
  any AST shape regressions.
- **Acceptance items deferred?** Yes, but properly assigned: SDL
  parser (step 8) → R7+; full validation rule set (remaining 5 of 10)
  → R8+; real napi binding body and AST serde derives → post-budget.
  These are documented in the topic summary's round log. SFC compiler
  remains explicitly out-of-scope.
- **Work nature shift?** No. Still in the structural-fix defect class
  (2/5). R6's rung 1 (slice-panic elimination) is a tuning rung within
  the same class, not a class shift. The class will close when either
  (a) gz lands in budget, (b) we surface to user for a budget revision,
  or (c) hard-stop at 5/5 forces user surface.
- **Anti-patterns?** None new. The Builder did not retry an Iron-Law-
  refuted path (no bumpalo redux), did not silently relax acceptance
  criteria, and did not over-claim ("PARTIAL" is the correct verdict).
  The R5 design note is appropriately scoped — picking 2B with stated
  rationale rather than going straight to 2C (highest-risk option).
- **Iteration ceiling?** 2/5 on structural-fix; healthy. Three rounds
  remain.
- **Coordination concern (parallel agent activity):** None this round.
  R5 was sequential (phase 1 → 2 → 3 → 4), no parallel R6/R7/etc.
  agent work in flight. No leftover merge conflicts at HEAD `031d4f5`.

---

## 7. Director's recommendation summary

R6 = Builder dispatched on rung 1 (Unicode/slice-panic elimination).
The R6 brief telegraphs the ~8.5 KB stable-only projection so the
Builder treats the measurement as diagnostic. The director-R6 note
(post R6 + Verifier R6) decides whether to surface the budget-vs-
Option-F choice based on empirical R6 data, OR to continue methodically
with rung 2 (wee_alloc) for one more round. If rung 1 regresses, surface
immediately on Iron Law.

Scope signal: **continue.**

---

https://claude.ai/code/session_01R5CSNvnAEYc7FCiPPgZspu
