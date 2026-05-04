# Director R9 — magna-gqlmin

Round: R9 director note (SURFACE TO USER)
Branch HEAD reviewed: `1d094a6`
Verdict: **PARTIAL — underwhelming.** gz=6,155, 1,035 bytes over budget,
in the "6,051 ≤ gz < 6,254" band per the R9 brief. Counter advances
2/5 → 3/5 in the build-std-nightly defect class. Iron Law does NOT
fire on the committed conservative variant. **Surface to user.**

The substantive new finding this round is not the −99 bytes — it is
the empirical confirmation that **the inlining lever is exhausted on
the parser code path.** That is durable knowledge regardless of which
option the user picks next.

---

## 1. On-thesis assessment

R9 Builder was **on-thesis end-to-end**, including the way they handled
the regression.

- **Followed 9a-inline brief.** Audited 11 candidate parser/lexer
  helpers, applied `#[inline(always)]` aggressively, measured. Path
  β toolchain (nightly + `-Z build-std=core,alloc` +
  `-Cpanic=immediate-abort`) and the R8 bump allocator carried forward
  unchanged. Workspace `rust-toolchain.toml` stays on stable. Bans
  honored. ABI preserved.
- **Bisection methodology was correct.** When the aggressive 11-helper
  variant measured gz=6,847 (Δ=+593 vs R8) — which would have tripped
  the Iron Law if shipped — Builder did not paper over the regression
  or reframe the comparison. They invoked the brief's "back off and
  re-measure" guidance, ran a single-helper bisection, and identified
  exactly which directives caused the regression
  (`Parser::peek` +433, `Parser::bump_tok` +204, `Parser::slice` +68).
  That bisection is itself the substantive output of R9.
- **Reported honestly.** The committed result (gz=6,155, Δ=−99) is in
  the "underwhelming" band, well short of the 5,750–6,050 R8
  projection for 9a-inline. Builder did not hide the projection miss
  or pad the savings. Phase 4 verification ran in full (38 corpus + 5
  pretty + 12 validation tests; wasm smoke tag=0 / tag=1+kind=34
  unchanged); `block_string_arg` corpus test specifically confirmed
  preserved.
- **Final committed inline-always set is the empirically-correct
  set, not the brief-suggested set.** Only `Span::new` (1-line
  constructor) and `Lexer::peek_byte` (1-line `self.bytes.get(pos).
  copied()`) gained `#[inline(always)]`. The other 9 candidates were
  reverted or kept at their pre-R9 `#[inline]`. This is the right
  call: bisection data shows the wins live at the one-liner end of
  the spectrum and that LLVM was already making correct outline
  choices on the medium helpers.

### Architectural picture after R9 (the durable finding)

The Pareto-frontier observation is **the new durable architectural
fact** for this topic:

> The R8 binary was already at a near-Pareto frontier on the
> inline/outline tradeoff. LLVM had picked optimal choices across
> the 5-line helpers via its standard cost model, and our
> `#[inline(always)]` directives at those medium helpers mostly
> displaced LLVM's outlining of duplicated tail patterns
> (`Result<Token, _>` propagation, `Option::take()` early-return,
> `unwrap_or("")` fallback). Forcing the inline direction at those
> sites grew the binary; it did not shrink it.

Top 4 functions after R9:

| Function | wat lines (R8) | wat lines (R9) | Δ |
|---|---|---|---|
| `gqlmin_parse` top-level | 2,256 | 2,257 | +1 |
| `$22` parser internal | 1,704 | 1,642 | −62 |
| `$15` parser internal | 1,476 | 1,477 | +1 |
| `$20` parser internal | 1,260 | 1,261 | +1 |
| **Top 4 total** | **6,696** | **6,637** | **−59** |

The top-4 wat-line delta (−59 lines) is consistent with the gz delta
(−99 bytes). The structural picture is unchanged from R8: parser body
dominates, ~85% of `.text` lives in the top 4. R9 trimmed a sliver
off `$22` and produced a microscopic net win.

### What this means for the budget thesis

**The size-tuning lever is empirically exhausted on the parser code
path within the current architectural posture.** Every non-parser
axis was already exhausted at R8 (Vec monomorphization R5; Unicode
tables R6; panic strings R7; allocator R8). R9 attacked the
parser-code axis the only way that doesn't change the API
(inlining-merge of helpers) and it yielded 99 bytes after careful
bisection. The remaining options for closing the 1,035-byte gap are:

- A user-visible API change (drop block-string parsing → 9b).
- A multi-round parser rewrite (state-table) — already declined by
  user at R8 surface, and the R9 bisection finding makes it even
  less attractive (LLVM is already doing the work).
- Accept the achieved size.

There is no fourth option that we have not measured.

---

## 2. Verdict ruling — PARTIAL (underwhelming)

**Ruling: PARTIAL — underwhelming.**

Per the R9 brief verdict bands:

- gz ≤ 5,120 → PASS — **NOT MET** (6,155 > 5,120).
- 5,121 ≤ gz ≤ 6,050 → PARTIAL — close-miss: not in band.
- **6,051 ≤ gz < 6,254 → PARTIAL — underwhelming: THIS BAND.**
- gz ≥ 6,254 → BLOCKED (regression vs R8): not in band on the
  committed conservative variant. (The aggressive variant tripped
  this and was correctly reverted.)

Iron Law triggers (R9 brief):

- **Regression vs R8 (6,254):** committed R9 = 6,155 (−99). Did not
  fire on the committed state. The aggressive intermediate variant
  (gz=6,847, Δ=+593) tripped it; per brief guidance, Builder backed
  off and re-measured. Correct response.
- **Build / smoke / test failures:** not triggered. 38 + 5 + 12
  native tests pass; wasm smoke ABI durable across R2/R3/R5/R6/R7/R8/R9.
- **Sub-threshold saving signal:** the brief did not encode a
  sub-threshold trigger for R9; the verdict bands handle it. The
  "underwhelming" band is the explicit name for what we landed in.

R9 lands cleanly in the "PARTIAL — underwhelming" band. Counter
advances 2/5 → 3/5 in the build-std-nightly defect class. **Two
attempts remain.**

State the headline finding clearly: **inlining is empirically
exhausted as a lever on the parser code path.** This is the durable
addition R9 makes to the topic's knowledge base; the −99-byte
measurement is secondary.

---

## 3. Routing for synthesis

The following findings should enter the living summary
(`docs/topic-summaries/gqlmin-summary.md`):

**Add to summary:**

- **R9 measurement:** gz=**6,155** (Δ=−99 vs R8). PARTIAL —
  underwhelming. Four-data-point progression: R6 stable 10,006 / R7
  build-std 8,605 / R8 build-std + bump 6,254 / R9 + 9a-inline 6,155.
  Reduction from R2 baseline 15,375 → 6,155 = **−9,220 bytes (60%
  reduction).**
- **The Pareto-frontier finding (durable lesson).** The R8 binary was
  already at a near-Pareto frontier on inline/outline. Forcing
  `#[inline(always)]` at medium-size hot helpers (5-line bodies with
  `Result<Token,_>` propagation: `Parser::peek`, `bump_tok`, `slice`,
  `expect`) grows the binary, not shrinks it, because it displaces
  LLVM's existing outlining of shared tail patterns. The
  bisection data is in `docs/investigation-r9-inline-regression.md`.
  **Future rounds attacking the parser code itself must acknowledge
  this constraint; do not apply `#[inline(always)]` blanket to
  parser helpers without measuring.** The wins are confined to true
  one-liners (`Span::new`, `Lexer::peek_byte`).
- **Updated honest estimate for 9b.** The R8 matrix gave 5,250–5,750
  for block-string drop. After R9, the achievable floor for that
  approach (9b stacked on R9's 6,155) is ~5,650–6,000. **Likely a
  close-miss again, NOT a clean budget hit.** The 9b estimate at R8
  surface was over-optimistic; honest update needed.
- **State-table rewrite is off the table.** User declined at R8
  surface. R9 bisection further confirms the parser body is well-
  tuned by LLVM; a rewrite would have to outperform LLVM's
  existing inline/outline choices, which is a tall order given R9's
  evidence. Removed from the options matrix.
- **Narrowed R9-onward options matrix.** Three options remain:
  A (accept-ship, 9d/9e equivalent, 0 rounds), B (9b block-string
  drop, 1 round, may still miss budget at 5,650–6,000),
  C (accept + lexer-only fallback, 1 round, ships two artifacts).
  See §5 surface note for the user-facing version.
- **Counter:** build-std-nightly defect class at **3/5** after R9.
  Two attempts remain.

**Keep in investigation/SIZE.md only:**

- The exact bisection log and per-helper Δ table — already in
  `docs/investigation-r9-inline-regression.md`.
- The R9 phase 4 measurement pipeline rows — already in `SIZE.md`
  R9 section.

---

## 4. Surface decision

**Surface to user — YES.** Strongly recommend.

The user already heard the full options matrix at the R8 surface and
chose 9a-inline explicitly because it was the lowest-risk
budget-attacking option. R9 returned −99 bytes against a 5,750–6,050
projection. That projection miss matters for the next decision: the
honest update is that the next-best option (9b block-string drop) is
also unlikely to hit budget cleanly.

The user must choose explicitly between (a) shipping at 6,155 with
the budget officially missed by 1,035 bytes and 60% reduction
documented honestly; (b) one more round of API-changing work that
probably still misses budget; or (c) shipping at 6,155 plus a separate
lexer-only artifact for sub-5KB consumers.

This is exactly the kind of substance call the user owns. Counter
permits it (3/5 with two slots; option B consumes 1, A and C
consume 0/1 respectively). I am not going to make this call
autonomously.

---

## 5. Surface note (relay verbatim to user)

---

**R9 result: gz = 6,155. 1,035 bytes over the 5,120 budget. The
inlining lever is empirically exhausted. Surfacing.**

### a. What R9 achieved

9a-inline (parser helper merging via `#[inline(always)]`) landed
at gz=**6,155**, a **−99 byte** improvement over R8. ABI durable.
38 corpus + 5 pretty + 12 validation tests pass. The
`block_string_arg` corpus case specifically confirmed running.
Function count R8 33 → R9 32.

Four-data-point progression for the size axis:

| Round | Approach | gz bytes | Δ |
|---|---|---|---|
| R6 | stable + Unicode/slice-panic eliminated | 10,006 | — |
| R7 | + nightly build-std + immediate-abort | 8,605 | −1,401 |
| R8 | + custom bump allocator (zero deps) | 6,254 | −2,351 |
| R9 | + 9a-inline (parser helper merging) | **6,155** | **−99** |
| Budget | — | 5,120 | gap: 1,035 |

**Reduction journey:** R2 baseline 15,375 → R9 6,155 = **−9,220
bytes (60% reduction).** Function count R2 150 → R9 32 (−79%).

### b. The Pareto-frontier finding (the durable result of R9)

Builder ran an aggressive variant first (all 11 candidate helpers
`#[inline(always)]`) and measured gz=6,847 — that's **+593 bytes
worse than R8.** Iron Law would have fired. They bisected
single-helper-at-a-time and found:

| Add-on (vs minimal one-liner-only baseline) | Δ |
|---|---|
| + `Parser::peek` (5-line body, ~12 sites) | **+433** |
| + `Parser::bump_tok` (5-line body, ~14 sites) | **+204** |
| + `Parser::slice` (4-line body, 5 sites) | +68 |

Only `Span::new` and `Lexer::peek_byte` (true one-liners) survived
the bisection.

The honest framing: **the R8 binary was already at a Pareto frontier
on inline/outline.** LLVM had picked optimal inlining choices across
the 5-line hot helpers via its cost model, and was outlining shared
tail patterns (`Result<Token,_>` propagation, `Option::take()` +
early-return, `unwrap_or("")` fallbacks). Our `#[inline(always)]`
directives at the medium helpers displaced that outlining and grew
the binary.

This is durable knowledge for any future round attacking the parser
code itself: **trust LLVM on the medium helpers; don't blanket-apply
`#[inline(always)]` without measuring.** The wins, when they exist,
are at the one-liner end of the spectrum, where the directive is
basically redundant with what LLVM already does.

### c. The narrowed options

Three options remain. State-table rewrite is **off the table** —
you declined at R8 surface, and the R9 bisection further confirms
the parser body is well-tuned by LLVM (a rewrite would have to
outperform LLVM's existing choices, a tall order).

| Option | Approach | Est. gz | Risk | Rounds | API impact |
|---|---|---|---|---|---|
| **A — accept-and-ship** | Update CI gate to ~6,500. Document achieved size. Topic-complete on size axis. Unblock SDL/napi-real-body/serde-derives. | 6,155 | none | 0 | none |
| **B — 9b block-string drop** | Remove `BlockStringValue` token + parser path. | 5,650–6,000 (honest update) | medium | 1 | **YES** — `"""..."""` literals reject |
| **C — accept + lexer-only** | Ship at 6,155 plus a `--features lexer-only` build target at ~3 KB gz for any consumer with a hard <5 KB requirement. | 6,155 + 3K artifact | low | 1 | none (additive) |

### d. Updated honest estimate for option B

The R8 surface estimated 9b at 5,250–5,750. After R9, that
estimate is too optimistic. Realistic update:

- 9b's likely raw saving on R9's 6,155 is ~150–500 gz (block-
  string `lex_block_string` path + parser variant + the
  `Value::String { block: bool }` arm).
- Floor: gz ≈ 5,655 — still **535 over budget.**
- Ceiling (if cleanup is broader than expected): gz ≈ 6,005 —
  still **885 over budget.**

**9b probably will NOT hit 5,120 either.** The remaining ~1,035-byte
gap may be irreducible within the current architectural posture.
Three rounds of close-miss data points (R7/R8/R9) plus this updated
9b estimate point at the same conclusion: the budget is not
reachable without either a parser rewrite (declined) or accepting
both API changes (block-string drop) AND running into one more
close-miss surface afterward.

### e. Director's recommendation (ranked)

You chose Path β at R6 explicitly to hit 5,120, and that preference
for a budget-hit signal still matters. But the data now says:
three close-miss data points, and the most-promising remaining
single round (9b) is honestly likely to be a fourth. Block-string
parsing is a real loss for consumers — some clients (GraphiQL,
generated tooling) format large arguments using `"""..."""`
literals in operation strings. It's not zero-impact.

**My ranked recommendation:**

1. **First choice: A — accept-and-ship.** Honest call given the
   data. 60% reduction shipped. Update CI gate to ~6,500. Document:
   "magna-gqlmin wasm: 6.0 KB gz on nightly build-std + custom
   bump allocator; 5,120-byte budget missed by 1,035 bytes after
   exhausting the inlining lever; 9b block-string drop projects to
   another close-miss." This unblocks SDL / real-napi-body /
   serde-derives / 5 more validation rules immediately. Counter
   stays at 3/5 with two attempts banked. **Zero rounds consumed.**

2. **Second choice: C — accept + lexer-only fallback.** Same as A
   plus ship a `--features lexer-only` artifact at ~3 KB gz. This
   gives any consumer with a hard <5 KB requirement a path
   without requiring you to break the executable parser API.
   Adds one round of work; result is durable (a separate build
   target, not a per-byte tuning round). Two artifacts: full
   parser at 6.2 KB and lexer-only at ~3 KB. **One round consumed,
   no Iron-Law risk.**

3. **Third choice: B — 9b block-string drop.** Highest budget-
   hit chance among the remaining options, but honestly probably
   another close-miss. Real API impact for the small set of
   callers using `"""..."""` literals in executable operation
   strings. **One round consumed; if it's a close-miss (4/5),
   one slot remains and you will face this same decision again
   at gz≈5,650–6,000 — only with the block-string drop already
   shipped (irreversible for that release).**

The substance call: A is the honest exit. C buys you a sub-5K
artifact for the consumers who actually need it without breaking
your executable parser. B is a shot that probably misses and
costs API surface either way.

**My one-line: A first, C second if you have an actual <5K
consumer requirement, B only if you're committed to spending the
counter slot on a probabilistic budget hit and accept the API
loss.**

### f. What's locked regardless

(From prior surfaces, unchanged by R9 outcome.)

- **R4-shipped napi/pretty/serde/validation:** napi scaffold, 5
  pretty error tests, 12 validation rule tests, serde feature
  scaffold — all delivered, all passing.
- **SDL deferred to post-budget:** build-order step 8 not started.
- **Real `#[napi]` body deferred to post-budget:** scaffold only.
- **AST serde derives deferred to post-budget:** feature
  scaffolded; derives not added.
- **5 of 10 validation rules deferred to post-budget:** 5 ops-only
  rules shipped in R4.

These items resume work whichever option you pick. A unblocks them
immediately; B unblocks them after R10 lands; C unblocks them
during the lexer-only round (parallelizable).

### g. Iteration discipline

Counter advances 2/5 → **3/5** after R9. Two attempts remain in
the build-std-nightly defect class.

- **A — accept-and-ship** consumes **0 attempts.** Counter rests
  at 3/5; class effectively retired with two slots in reserve if
  the size axis ever re-opens.
- **B — 9b block-string drop** consumes **1 attempt.** If it
  lands ≤5,120, counter retires at 4/5. If it close-misses (the
  honest expected outcome at 5,650–6,000), counter is at 4/5
  with one slot left, you've shipped an API change, and you're
  facing this same decision again with less buffer.
- **C — accept + lexer-only** consumes **1 attempt** in the
  build-std-nightly class only if the lexer-only round goes
  sideways; if it lands cleanly (low risk — it's an additive
  feature flag, not a tuning attempt), counter could arguably
  stay at 3/5. Conservatively: 1 attempt.

The cleanest counter posture is A (zero attempts, two slots
banked). C is also clean if you have a real consumer for the
sub-5K artifact. B is the path most likely to advance counter
without retiring the class.

---

## 6. Continuity check

- **Did Builder revise targets silently?** No. Builder reported the
  full delta against the brief's projection (5,750–6,050 → actual
  6,155) honestly and called the round "underwhelming" themselves
  in the SIZE.md and investigation note.
- **Hidden sample failures?** No. 38 corpus + 5 pretty + 12
  validation tests pass. `block_string_arg` corpus case specifically
  confirmed running (R9 brief required this check explicitly given
  the 9b option exists in the matrix). Wasm smoke ABI durable
  across R2/R3/R5/R6/R7/R8/R9.
- **Acceptance items deferred?** No new deferrals. Same set as prior
  rounds.
- **Work nature shift?** **Possibly, depending on user choice.**
  - A or C exits the iteration class cleanly ("size axis topic-
    complete at measured size"); work nature shifts to "ship +
    post-budget feature work."
  - B continues within the build-std-nightly class with reduced
    buffer.
- **Anti-patterns?** **None new — and one notable positive
  finding.** The Pareto-frontier observation is not an anti-pattern;
  it is the substantive durable contribution of the round, on par
  with the R3 bumpalo refutation as a "what we learned by trying."
  Builder did not retry an Iron-Law-refuted path (the aggressive
  variant was correctly reverted, not committed). Builder did not
  silently relax acceptance. Builder did not over-claim (the
  "underwhelming" framing is theirs, not externally imposed).
- **Coordination concern (parallel agent activity):** None this
  round. R9 was sequential.

---

## 7. Director's recommendation summary

**Verdict: PARTIAL — underwhelming.** R9 = 6,155 gz, in the
"6,051 ≤ gz < 6,254 underwhelming" band per the R9 brief. Iron Law
does NOT fire on the committed conservative variant.

**Routing:** add R9 measurement, the Pareto-frontier finding (durable
lesson), the updated 9b honest estimate, and the narrowed A/B/C
options matrix to the topic summary.

**Surface decision: SURFACE TO USER.** Four measurements and one
durable architectural finding (inlining-as-lever-exhausted) in
hand. User picks among A/B/C.

**Director's recommendation (one line):** **A — accept-and-ship**
(60% reduction is real; 9b probably close-misses; counter stays
clean at 3/5 with two slots banked). C if you have an actual sub-5K
consumer. B only if you're committed to spending a counter slot on
a probabilistic budget hit with API loss either way.

**Iteration discipline:** build-std-nightly class at **3/5** after
R9. Two attempts remain. A consumes 0; B consumes 1 (with
close-miss risk → 4/5 unconverged); C consumes 1 (low risk).

---

https://claude.ai/code/session_01R5CSNvnAEYc7FCiPPgZspu
