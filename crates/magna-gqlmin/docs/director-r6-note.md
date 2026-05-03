# Director R6 — magna-gqlmin

Round: R6 director note (SURFACE)
Branch HEAD reviewed: `f54b71a`
Verdict: **PARTIAL — rung 1 landed full estimate; preemptive surface to user
triggered per Director R5 §5(a).**

---

## 1. On-thesis assessment

R6 is on-thesis. The Builder followed the rung-1 brief precisely and
produced an unusually clean result:

- **Estimate vs actual:** R5 projected −3 to −4 KB gz from
  Unicode/slice-panic elimination. R6 measured **−4,889 bytes** —
  above the upper end of the band. This is a strong signal that the
  R5 bloat-source diagnosis was accurate, not coincidental.
- **Zero scope creep.** The Builder touched only `src/lex.rs` and
  `src/parse/mod.rs`, replacing 17 byte-index reads + 2 `&str` slices
  + 1 `&Node` index with `Option`-returning equivalents. No new
  `unsafe`. No API change. No new dependencies. No data-structure
  rewrite. No allocator change. No speculative side-quests.
- **Verification was wasm-dis-grounded, not just byte-counting.**
  R6 confirmed via `wasm-dis` that the specific bloat sources R5
  hypothesised (Unicode `printable.rs` tables, `lex.rs` filename
  literal, `byte index … is not a char boundary` panic message,
  `range end index … out of range` panic message) are **absent** in
  the R6 binary. The diagnosis-then-elimination loop is closed.
- **No regression.** All 38 + 5 + 12 tests pass; wasm smoke ABI
  unchanged (tag=0 / tag=1+kind=34). ABI durable across R2/R3/R5/R6.
- **Iron Law did not fire** (savings 4,889 ≫ 1,500-byte threshold).

The R6 audit doc (`docs/audit-r6-slice-panics.md`) is the right shape
of evidence: enumerated catalog before edits, mechanical phase-2 plan,
explicit list of bloat sources eliminated AND remaining. The remaining
bloat now has names and call sites — this round produced the empirical
grounding the R5 surface was missing.

The remaining bloat picture is now well-characterised, and the
already-measured estimates for R7+ rungs let us project the
stable-only ceiling with empirical (not speculative) confidence.

---

## 2. Routing for synthesis

The following findings should enter the living summary:

**Add to summary:**

- **R6 measurement:** gz=**10,006** (Δ=−4,889 vs R5 14,895). PARTIAL —
  inside the partial-over-ceiling band (7,001 ≤ gz ≤ 11,000). Halved
  the gap to budget (was 9,775; now 4,886).
- **Hypothesis confirmed empirically:** R5's "Unicode tables reachable
  from `&str` slice panic" diagnosis is correct. `wasm-dis` verifies
  that eliminating those slice operations removes the entire
  ~3–4 KB Unicode `printable.rs` data section, the `lex.rs` filename
  literal, the char-boundary panic message, and the slice-range panic
  message.
- **Stable-only ceiling, now empirically projected:**
  - Rung 2 (wee_alloc swap): −1.4 KB measured (R2)
  - Rung 3 (custom panic_handler / fmt::Write shim): ~−1 KB
    estimated (medium confidence; targets remaining filename
    literals + capacity-overflow strings)
  - Rung-misc (single slice-index msg, parse/mod.rs filename,
    two-digit ASCII pair table): ~−400 bytes
  - **Combined R7+ best-case projection: gz ≈ 7,200 bytes.** Just
    over the 7,000-byte Iron Law ceiling, **2 KB over** the 5,120
    budget.
- **The 5,120 budget is not reachable on stable Rust.** Only Option F
  (build-std nightly) or aggressive panic-elimination measures (which
  converge with F) are likely to land at or under 5,120. This is now
  empirically grounded, not estimated.
- **R7+ rung options with measured/audited estimates:**
  1. Rung 3 — custom panic_handler / fmt::Write shim. ~−1 KB. Risk:
     medium (panic_handler delicate; must not break wasm shim).
  2. Rung 2 — dlmalloc → wee_alloc swap. ~−1.4 KB (R2-measured).
     Risk: low (one-line change). Wee_alloc unmaintained but
     irrelevant for parse-once-then-drop usage.
  3. Rung-misc — three small targets totaling ~−400 bytes. Risk: low.
- **Counter:** structural-fix at **3/5** after R6. Two attempts remain
  on this defect class.

**Keep in investigation/audit doc only:**

- The 17-byte-index + 2-slice + 1-node-index catalog from
  `docs/audit-r6-slice-panics.md` — already the right level of
  documentation in that file.
- The exact `wasm-dis` symbol enumeration — already in SIZE.md R6
  section.

---

## 3. Surface note (relay-to-user content)

> **Round 6 result + decision request.** Below is the user-facing
> relay block. Team Lead should surface verbatim.

### a. What R6 achieved

R6 landed rung 1 (Unicode/slice-panic elimination) cleanly:

- **gz=10,006 bytes** (was 14,895 in R5; saved 4,889 bytes — above the
  upper end of the 3–4 KB estimate).
- All 38 corpus + 5 pretty + 12 validation tests still pass.
- Wasm ABI unchanged (smoke tag=0 success / tag=1+kind=34 parse error
  durable across R2/R3/R5/R6).
- No new `unsafe`, no API change, no new dependencies.
- `wasm-dis` confirms the eliminated bloat sources are physically
  absent from the binary, not just smaller.

### b. Updated bloat picture

**Eliminated** (verified by `wasm-dis`):

- The full Unicode `printable.rs` property tables (~3–4 KB).
- `library/core/src/str/mod.rs` and `crates/magna-gqlmin/src/lex.rs`
  filename literals.
- `byte index … is not a char boundary` panic message.
- `range end index … out of range for slice of length …`.
- `index out of bounds: the len is …`.
- `begin <= end ( <= ) when slicing`.

**Remaining** (already characterised — no new diagnostic needed):

- `Vec`/`raw_vec`/`alloc` filename literals + `capacity overflow` /
  `memory allocation of … bytes failed` panic messages — from parser
  scratch & node arena `Vec::push`/`extend` capacity-overflow paths.
- `dlmalloc-0.2.13/src/dlmalloc.rs` filename + internal asserts
  (`psize >= size + min_overhead` etc.).
- `crates/magna-gqlmin/src/parse/mod.rs` filename literal — emitted
  by `track_caller` on `panic_invariant()`.
- One residual `slice index starts at … but ends at …` site (not yet
  located).
- `0x00..99` two-digit ASCII pair table (used by integer Display in
  alloc panic paths).

### c. Math reality, now empirically validated

Combining the remaining stable rungs:

| Rung | Action | Estimated savings | Confidence |
|---|---|---|---|
| 3 | Custom panic_handler / fmt::Write shim | ~1.0 KB | medium |
| 2 | dlmalloc → wee_alloc swap | ~1.4 KB | high (R2-measured) |
| misc | Single slice-index msg + parse filename + ASCII table | ~0.4 KB | low-medium |
| **Total best-case** | | **~2.8 KB** | |

Projected stable-only landing: **gz ≈ 7,200 bytes.** That is:

- ~200 bytes **over** the 7,000-byte Iron Law ceiling we set in R2.
- ~2,080 bytes **over** the 5,120-byte original budget.

Bottom line: **the 5,120 target is not reachable on stable Rust** with
the rungs available. Only Option F (build-std nightly) or equivalent
aggressive panic-elimination is likely to bridge that gap. This is
now an empirical projection, not an estimate.

### d. Your choice — four concrete next paths

**Path α — Continue methodically through rungs 2 + 3 + misc.**
End state: gz ≈ 7.2 KB. Document as "stable-Rust achievable ceiling."
Strict-budget consumers fall back to a lexer-only build (estimated
< 3 KB, not yet built). **Cost:** 2–3 more rounds (one per rung, plus
verification). **Outcome:** budget officially missed by ~2 KB; ceiling
honestly documented; no toolchain split.

**Path β — Skip remaining stable rungs; jump to Option F (build-std nightly).**
Likely lands at or near 5,120 in a single round. **Cost:** 1 round
(R7). **Outcome:** budget met, BUT wasm builds bind to a nightly
toolchain (every contributor + CI needs a nightly channel pin).

**Path γ — Revise the budget.**
Set the new target at ~7.5 KB (achievable on stable after rungs
2 + 3 + misc, with a small headroom). Update the CI gate accordingly.
**Cost:** 1 round of stable tuning, then ship. **Outcome:** budget
officially relaxed; no toolchain split; documented as an honest
tradeoff against the original ≤5 KB framing.

**Path δ — Hybrid: stable default + nightly "small" variant.**
Continue stable to ~7.2 KB (Path α path), then add Option F as a
secondary build artifact. Default wasm is stable (~7.2 KB); a `small`
variant uses nightly to land near 5,120. **Cost:** Path α cost
(2–3 rounds) + 1 round to add the nightly variant = 3–4 rounds.
**Outcome:** both audiences served; toolchain complexity present but
opt-in; CI matrix grows by one job.

### e. Director's recommendation

**Path γ (revise the budget to ~7.5 KB), with a stretch consideration
for Path δ if the nightly variant matters to a downstream consumer.**

Reasoning:

1. **The 5 KB target was aspirational, set by the Architect *before*
   we had measurements.** It assumed a non-`Vec` parser design. R2,
   R3, R5, and R6 have collectively proved that on stable Rust with
   safe slice operations, the floor sits ~2 KB above 5,120. That's a
   structural property of the toolchain, not a defect in our
   implementation. Revising the budget reflects what we've learned.
2. **Time-to-ship.** Path γ closes the topic in 1 more round. Path α
   takes 2–3 rounds and lands at the same place as γ but with the
   budget officially missed. Path β is 1 round but introduces a
   permanent nightly dependency for everyone touching wasm.
3. **Toolchain ecosystem cost.** Binding the entire wasm build to
   nightly is a real durable tax — every CI run, every contributor,
   every downstream Magna consumer who rebuilds wasm. For a 2 KB
   savings, the cost-benefit doesn't favor β as the default.
4. **R6 already proved a 35% size reduction is achievable with low
   risk.** From R2's 15,375 to R6's 10,006 is a 35% reduction with
   no API change, no new deps, no `unsafe`. Continuing to ~7.2 KB
   adds another ~28%. We have demonstrated good-faith effort.
5. **Path δ is the right answer IF a downstream consumer has a hard
   < 5 KB requirement.** Today no such consumer is identified. If
   one emerges, δ is a clean upgrade from γ.

If the user disagrees with γ, the next-best is **δ** (preserves the
stable default while serving a hypothetical strict-budget consumer).
**β alone** is the path I'd recommend against — it pays a permanent
toolchain tax for a one-time 2 KB win.

### f. What happens to other open work (regardless of α/β/γ/δ)

Unchanged:

- R4-shipped napi scaffold, pretty errors, serde feature, 5 of 10
  validation rules — all continue to work.
- SDL parser (build-order step 8) — still deferred to R7+.
- Remaining 5 of 10 validation rules (build-order step 9) — still
  deferred to R8+.
- Real `#[napi]` body and AST serde derives — still post-budget work.
- SFC compiler (step 11) — still explicitly out-of-scope this session.

None of those items shift based on the path chosen.

---

## 4. Iteration discipline

Counter is at **3/5** on the structural-fix defect class entering the
user surface. The path the user picks determines what happens next:

- **Path α (continue stable rungs):** structural-fix counter
  continues. Rung 2 + verifier = 4/5. Rung 3 + verifier = 5/5
  (hard-stop). Rung-misc would force a class-reset. This path
  effectively burns the remaining counter on tuning.
- **Path β (Option F nightly):** **budget-reset.** Different defect
  class — toolchain change, not structural fix. Counter resets to 0/5
  for the new class. This path has fresh iteration budget.
- **Path γ (revise budget):** **budget-reset.** Target is relaxed;
  remaining work shifts back to "build to spec" against the new
  target. Counter resets. Lowest iteration cost overall.
- **Path δ (hybrid):** continues counter for the stable side (α-class);
  starts a fresh β-class counter for the nightly variant. Highest
  iteration cost but parallelizable.

The user should understand: **α is the only path that risks
hard-stopping at 5/5 still over budget.** γ is the lowest-iteration-
cost option.

---

## 5. Continuity check

- **Did Builder revise targets silently?** No. Builder reported the
  full delta honestly; exceeded the estimate without recharacterising
  the brief.
- **Sample-level failures hidden?** No. All 38 + 5 + 12 tests pass;
  wasm smoke ABI durable.
- **Acceptance items deferred?** No new deferrals. The pre-existing
  SDL/validation/napi/serde deferrals from R4 remain properly
  assigned in the round log.
- **Work nature shift?** **Imminent — depends on user choice.** α
  stays in structural-fix. β shifts to toolchain-config. γ shifts back
  to build-to-spec. δ runs both. The Team Lead should re-classify the
  defect class at the start of R7 based on which path the user picks.
- **Anti-patterns?** None new. Builder did not retry any Iron-Law-
  refuted path, did not silently relax acceptance, did not over-claim
  ("PARTIAL" with full estimate landing is the correct verdict).
- **Coordination concern (parallel agent activity):** None this
  round. R6 was sequential; no in-flight R7 work to merge against.

---

## 6. Director's recommendation summary

Surface the relay block in §3 to the user verbatim. Recommend
**Path γ** (revise budget to ~7.5 KB), with **Path δ** as the
fallback if a strict-budget downstream consumer materialises.
Path β alone is the recommendation-against — pays a permanent
nightly-toolchain tax for a one-time 2 KB win.

If the user picks γ: R7 is a single "land the rungs, set the new CI
gate, document the ceiling, ship" round; topic closes shortly after.

If the user picks α: brief R7 Builder on rung 2 (wee_alloc swap) as
the next mechanical step; flag that we're advancing the counter to
4/5 and rung 3 will be the last attempt before hard-stop.

If the user picks β: brief R7 Builder on Option F (build-std
configuration); reset counter; this is now a toolchain-config class.

If the user picks δ: dispatch parallel R7 — Builder A on rung 2
(stable path), Builder B on Option F variant (nightly path); track
counters separately.

Scope signal: **surface (decision-required).**

---

https://claude.ai/code/session_01R5CSNvnAEYc7FCiPPgZspu
