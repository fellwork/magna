# Director R8 — magna-gqlmin

Round: R8 director note (SURFACE TO USER)
Branch HEAD reviewed: `fcaa5fe`
Verdict: **PARTIAL — close.** gz=6,254, 1,134 over budget, in the
"5,121 ≤ gz ≤ 6,500 close" verdict band per Director R7 §4. Counter
advances 1/5 → 2/5 in the build-std-nightly defect class. Iron Law
does NOT fire. **Surface to user per R7 §7 surface plan.**

---

## 1. On-thesis assessment

R8 Builder was **on-thesis end-to-end.**

- **Followed the R7 brief precisely.** Custom bump allocator replaced
  dlmalloc under `feature = "wasm"`; carry-forward of Path β
  (`-Z build-std=core,alloc` + `-Cpanic=immediate-abort` via RUSTFLAGS)
  preserved unchanged. Workspace `rust-toolchain.toml` stays on stable.
  Bans honored (no `format!`, no `String`, no `regex`, no Unicode
  tables). Public ABI preserved (`gqlmin_alloc`, `gqlmin_free`,
  `gqlmin_parse`, `gqlmin_result_free`).
- **Lifecycle reasoning correct.** Builder identified that
  `gqlmin_parse` resetting the bump pointer at entry would corrupt
  the input bytes the caller wrote via `gqlmin_alloc`, and instead
  chose fill-then-rollover semantics with a 256 KiB arena. Documented
  the assumption (single-threaded wasm, parse-once-then-drop, fresh
  instance per long-running session). The R7 brief's offhand "reset
  at parse entry" was wrong; Builder caught it and adapted with
  judgment.
- **Cargo.toml cleanup landed.** `dlmalloc` optional dep removed
  from `[dependencies]`; `dep:dlmalloc` entry removed from the
  `wasm` feature dep list. **The wasm build now has zero runtime
  crate deps.** This is a durable architectural fact worth
  capturing — the wasm distribution is now a pure-Rust + core/alloc
  build with no external crate surface.
- **Empirical resolution of the R7 dlmalloc lesson.** R7's "build-std
  rebuilds core+alloc but not external `[dependencies]`" finding
  predicted that removing dlmalloc as a `[dependencies]` entry
  (rather than tuning it) was the right move. R8 measurement
  validates: removing dlmalloc dropped 31 functions (R7: 64 → R8: 33)
  and ~2 KB raw of malloc/free/sbrk/chunk-merging code. The
  bump allocator's `alloc` is fully inlined into callers; `dealloc`
  is a no-op and gets fully eliminated. No new functions appeared.
- **Measured honestly.** Reported gz=6,254 against R7's projected
  ~6,800 ceiling; the win exceeded projection by ~550 bytes. No
  reframing, no padding the savings, no hidden regressions.

### Architectural picture after R8

The R7 framing — "future rounds attack code, not data" — is now
empirically the only path forward:

| Region | wat lines | Notes |
|---|---|---|
| `$27` `gqlmin_parse` (top-level export) | 2,256 | Parser entry. |
| `$22` (parser internal) | 1,704 | Recursive-descent body. |
| `$15` (parser internal) | 1,476 | Recursive-descent body. |
| `$20` (parser internal) | 1,260 | Recursive-descent body. |
| **Top 4 total** | **6,696** | **~85% of `.text`.** |
| Data section | ~55 bytes | Only the keyword pool literal. |

R8 verifies that the build-std + immediate-abort + bump-allocator
combination is the right architectural posture. **Only the parser
code itself remains as the dominant bloat source.** Every non-parser
axis has been exhausted: Vec monomorphization (R5), Unicode tables
(R6), panic strings (R7), allocator (R8). What's left is a hand-
written LL(1) recursive-descent parser whose top 4 functions account
for 85% of the binary.

### Is the ≤5,120 thesis still achievable?

**Plausible but requires a tradeoff the user has not yet been asked
to make.** Math reality after R8:

| Action | Projected R8→landing gz | Confidence | Tradeoff |
|---|---|---|---|
| 9a-state-table parser refactor | 4,000–5,500 | low | high risk (rewrite, regression-prone) |
| 9a-inline (helper merging only) | 5,750–6,050 | medium | lower risk, smaller win |
| 9b-block-string drop | 5,250–5,750 | medium | **user-visible API change** |
| 9a-inline + 9b combined | ~5,000 | medium | API change + low-risk inline |
| 9c-combined micro | 5,550–5,950 | medium | likely won't hit budget |
| 9d-accept | 6,254 | n/a | document and ship |
| 9e-revise | budget reset | n/a | no further rounds |

The honest framing: **5,120 is reachable with R9 only if the user
accepts the block-string-parsing API drop OR commits to a multi-
round state-table parser refactor with regression risk.** Otherwise
the achievable floor is ~5,750–6,050 (9a-inline alone) or ~6,254
(accept).

---

## 2. Verdict ruling — PARTIAL (close)

**Ruling: PARTIAL — close.**

Per Director R7 §4 (R8 verdict bands):

- gz ≤ 5,120 → PASS — **NOT MET** (6,254 > 5,120).
- 5,121 ≤ gz ≤ 6,500 → **PARTIAL — close: THIS BAND.**
- 6,501 ≤ gz ≤ 8,604 → PARTIAL — disappointing: not in band.
- gz ≥ 8,605 → BLOCKED (regression vs R7): not in band (R8 is well
  below R7 8,605).

Iron Law triggers (R7 §8):

- **Regression vs R7 (8,605):** R8 = 6,254. Did not fire (R8 is
  2,351 below R7).
- **Sub-threshold saving (< 500 bytes vs R7):** R8 saved 2,351 bytes.
  Did not fire.
- **ABI break:** smoke tag=0 / tag=1 kind=34 unchanged across
  R2/R3/R5/R6/R7/R8. Did not fire.
- **Test regression:** 38 + 5 + 12 native tests pass; napi + serde
  features compile; workspace `cargo check` clean. Did not fire.

R8 lands cleanly in the "PARTIAL — close" band. Counter advances
1/5 → 2/5 in the build-std-nightly defect class. **Three attempts
remain.**

---

## 3. Routing for synthesis

The following findings should enter the living summary
(`docs/topic-summaries/gqlmin-summary.md`):

**Add to summary:**

- **R8 measurement:** gz=**6,254** (Δ=−2,351 vs R7). PARTIAL — close.
  Three-data-point progression: R6 stable 10,006 / R7 build-std 8,605 /
  R8 build-std + bump 6,254. Reduction from R2 baseline 15,375 →
  6,254 = **−9,121 bytes (59% reduction).**
- **Custom bump allocator + zero-runtime-deps wasm build (durable
  architectural fact).** The `[dependencies]` block has zero entries
  for the wasm feature path; the wasm distribution is now a pure-Rust
  + core/alloc build with no external crate surface. 256 KiB static
  arena, fill-then-rollover semantics, single-threaded wasm32, no-op
  dealloc, panic-free overflow returns null. ~50 lines of `unsafe`
  Rust under `#[cfg(target_arch = "wasm32")]`.
- **Parser code is now the dominant bloat (~85% of `.text` in top 4
  functions).** Every non-parser axis exhausted: Vec monomorphization
  (R5), Unicode tables (R6), panic strings (R7), allocator (R8). Top
  4 = 6,696 wat lines. Future rounds attack the parser body itself
  or accept the achieved ceiling.
- **R9 options matrix.** Five candidates ranked: 9a-state-table
  (high risk, high yield), 9a-inline (low risk, low yield),
  9b-block-string drop (medium yield, **API change**), 9c-combined
  micro (low yield, likely close-miss), 9d-accept-and-ship,
  9e-revise-budget. See §5 surface note for the user-facing version.
- **Counter:** build-std-nightly defect class at **2/5** after R8.
  Three attempts remain.

**Keep in investigation/SIZE.md only:**

- The exact `wasm-dis` symbol enumeration and function-table diff
  (R7 vs R8) — already in `SIZE.md` R8 section.
- The fill-then-rollover lifecycle reasoning — already in `SIZE.md`.

---

## 4. Surface decision

**Surface to user — YES.** Per Director R7 §7 surface plan:

> "Surface to user after R8 with three-data-point evidence in hand."

R7 was explicit that R8 was the gather-evidence round and that the
user agreed to surface after R8 regardless of outcome (PASS, PARTIAL,
or BLOCKED). The verdict bands were structured so that:

- PASS → surface a clean win.
- PARTIAL — close → surface options matrix; user picks.
- PARTIAL — disappointing → surface re-evaluation.
- BLOCKED → surface immediately.

R8 landed in PARTIAL — close. The user-facing decision is now
informed by three honest measurements rather than one. **The relay
in §5 is the surface.**

---

## 5. Surface note (relay verbatim to user)

---

**R8 result: gz = 6,254. 1,134 bytes over the 5,120 budget. Surfacing
per the post-R8 plan.**

### a. What R8 achieved

Custom 256 KiB bump allocator replaces dlmalloc inline in the wasm
shim. The wasm distribution now has **zero runtime crate dependencies**
(the `[dependencies]` block in Cargo.toml has no entries for the wasm
feature path). 38 corpus + 5 pretty + 12 validation tests pass. Wasm
ABI durable (smoke tag=0 success / tag=1 kind=34 unchanged).

Three-data-point progression for the size axis:

| Round | Approach | gz bytes | Δ |
|---|---|---|---|
| R6 | stable + Unicode/slice-panic eliminated | 10,006 | — |
| R7 | + nightly build-std + immediate-abort | 8,605 | −1,401 |
| R8 | + custom bump allocator (dlmalloc removed) | **6,254** | **−2,351** |
| Budget | — | 5,120 | gap: 1,134 |

**Reduction journey:** R2 baseline 15,375 → R8 6,254 = **−9,121 bytes
(59% reduction).** Function count R2 150 → R8 33 (−78%).

### b. Where the remaining bloat lives

The R8 binary is essentially all parser code now. Top 4 functions =
6,696 wat lines = ~85% of `.text`:

- `$27 gqlmin_parse` top-level export — 2,256 wat lines
- `$22` parser internal — 1,704 wat lines
- `$15` parser internal — 1,476 wat lines
- `$20` parser internal — 1,260 wat lines

Data section is structurally clean (~55 bytes, just the parser
keyword pool literal). Every non-parser bloat axis is exhausted:
Vec monomorphization (R5), Unicode tables (R6), panic strings (R7),
allocator (R8). What's left is the LL(1) recursive-descent parser
itself.

### c. The five options for R9

| Option | Approach | Est. gz | Risk | Rounds | API impact |
|---|---|---|---|---|---|
| **9a-state-table** | Replace recursive-descent fn calls with a state machine table | 4,000–5,500 | **high** (rewrite, regression-prone) | 2–3 | none |
| **9a-inline** | Merge small parser helpers, reduce monomorphization overhead | 5,750–6,050 | low | 1 | none |
| **9b-block-string drop** | Remove `BlockStringValue` token + parser path | 5,250–5,750 | medium | 1 | **YES** — `"""..."""` literals reject |
| **9c-combined-micro** | 9a-inline + literal sharing + small wins | 5,500–5,950 | low | 1 | none |
| **9d-accept-and-ship** | Document 6,254 as achieved ceiling; update CI gate | 6,254 | none | 0 | none |
| **9e-revise-budget** | Set new target ~6,500; ship | 6,254 | none | 0 | none |

Combined option for completeness:

- **9a-inline + 9b** combined: gz ≈ 5,000. Likely under budget.
  Requires API change.

### d. Director's recommendation

You chose Path β at R6 to hit 5,120, not "close to 5,120." That
preference for a budget-hit signal matters. But 6,254 is honest, the
59% reduction is real, and the remaining work is either a high-risk
parser rewrite (9a-state-table) or a user-visible API change
(9b-block-string drop).

**My recommendation, ranked:**

1. **First choice: 9a-inline + 9b-block-string-drop combined,** if
   you accept the API change. Projects to ~5,000 gz. One round of
   work. Low-medium risk. Lands the budget. Block-string parsing is
   not commonly used for executable documents (it's mostly an SDL
   description-comment construct); the operations parser is the
   constrained build. You said at R5 surface that the API change
   would be a user-decision point — this is that point.
2. **Second choice: 9d-accept-and-ship.** 6,254 is honest and the
   59% reduction is the real story. Update the CI gate to ~6,500,
   document the achieved size as "6.1 KB gz on nightly build-std +
   custom bump allocator," and unblock SDL/validation/serde-derives/
   napi-real-body work (post-budget per R4). Topic closes on the
   size axis with a documented residual gap.
3. **Avoid: 9a-state-table.** Highest risk and most rounds. The
   parser is hand-written, well-tested (38 corpus cases), and any
   regression introduced by a rewrite would cost more iteration
   counter than we have left (2/5 → could exhaust). This is the
   path most likely to consume the remaining 3 attempts without
   landing budget.
4. **Skip: 9a-inline alone or 9c-combined-micro.** Both project to
   close-miss territory (5,750–6,050). One round of work for a
   non-budget-hitting result is the worst combination.
5. **9e-revise-budget** is functionally identical to 9d for our
   purposes; both ship at 6,254. Pick whichever framing is cleaner.

### e. What's locked regardless

(From prior surfaces, unchanged by R8 outcome.)

- **R4-shipped napi/pretty/serde/validation:** napi scaffold, 5
  pretty error tests, 12 validation rule tests, serde feature
  scaffold — all delivered, all passing.
- **SDL deferred to post-budget:** build-order step 8 not started.
- **Real `#[napi]` body deferred to post-budget:** scaffold only;
  the parseExecutableDocument JSON return path is stubbed.
- **AST serde derives deferred to post-budget:** feature scaffolded;
  derives not added.
- **5 of 10 validation rules deferred to post-budget:** 5 ops-only
  rules shipped in R4.

These items resume work whichever R9 option you pick. 9d/9e unblock
them immediately; 9a/9b unblock them after R9 lands.

### f. Iteration discipline note

The build-std-nightly defect class counter advances **2/5 after R8**.
Three attempts remain in this class.

- **9a-state-table** would consume 2–3 of the remaining 3 attempts
  with regression risk on each. If it fails, we have no buffer left
  and we'd surface again at 5/5 hard-stop with no budget hit and
  weeks of churn.
- **9a-inline + 9b combined** consumes 1 attempt with a clear
  hypothesis ("inline + block-string-drop ≈ 5,000 gz"). Predictable
  measurement. Two attempts remain in reserve.
- **9d-accept** and **9e-revise** consume 0 attempts. Counter rests
  at 2/5; class is effectively retired. The remaining attempts stay
  available if a future need re-opens the size axis.

The cleanest exits from the iteration class are 9d/9e (zero attempts)
or 9a-inline+9b (one attempt, clear hypothesis, lands budget if
hypothesis holds).

---

## 6. Continuity check

- **Did Builder revise targets silently?** No. R8 reported the full
  delta against R7 honestly and called out the 1,134-byte gap to
  budget without reframing.
- **Sample-level failures hidden?** No. 38 + 5 + 12 native tests
  pass; napi + serde compile; workspace `cargo check` clean. Wasm
  smoke ABI durable across R2/R3/R5/R6/R7/R8.
- **Acceptance items deferred?** No new deferrals. Same set as
  prior rounds (SDL parser, 5 of 10 validation rules, real
  `#[napi]` body, AST serde derives — all post-budget).
- **Work nature shift?** **Possibly, depending on user choice.**
  - 9d/9e exit the iteration class cleanly (budget reset / accept).
    This is a class change ("size axis topic-complete"), counter
    stays at 2/5 dormant.
  - 9a-inline + 9b continues within the build-std-nightly class.
  - 9a-state-table arguably opens a new class ("parser refactor")
    and would warrant a counter reset by playbook — note for the
    Director if user picks this path.
- **Anti-patterns?** None new.
  - Builder did not retry an Iron-Law-refuted path.
  - Builder did not silently relax acceptance.
  - Builder did not over-claim ("PARTIAL — close" matches the
    measurement; the projection beat is foregrounded honestly but
    not framed as a budget hit).
  - Builder caught and corrected a brief-level error (the R7
    "reset at parse entry" instruction would have corrupted the
    input buffer; Builder chose fill-then-rollover instead and
    documented the reasoning).
- **Coordination concern (parallel agent activity):** None this
  round. R8 was sequential.

---

## 7. Director's recommendation summary

**Verdict: PARTIAL — close.** R8 = 6,254 gz, in the "5,121 ≤ gz ≤
6,500 close" band per Director R7 §4. Iron Law does NOT fire.

**Routing:** add R8 measurement, custom bump allocator + zero-deps
fact, parser-as-dominant-bloat finding, and R9 options matrix to the
topic summary.

**Surface decision: SURFACE TO USER per R7 §7 plan.** Three
measurements in hand (R6 stable / R7 build-std / R8 build-std +
bump). User picks among 9a/9b/9c/9d/9e.

**Director's recommendation (one line):** **9a-inline + 9b-block-
string-drop combined if user accepts API change; otherwise
9d-accept-and-ship.** Avoid 9a-state-table (high risk, multi-round,
counter-burn).

**Iteration discipline:** build-std-nightly class at **2/5** after
R8. Three attempts remain. 9a/9b/9c continue within class; 9d/9e
exit class cleanly.

---

https://claude.ai/code/session_01R5CSNvnAEYc7FCiPPgZspu
