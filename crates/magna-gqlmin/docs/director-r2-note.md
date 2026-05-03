# Director R2 — magna-gqlmin

Round: R2 director note
Builder commits reviewed: R2 build (post-Iron-Law investigation)
Verifier input: R2 SIZE.md + investigation-r2-wasm-size.md
Verdict: PARTIAL — all deliverables complete except the size budget; surface condition active

---

## 1. On-thesis assessment

Builder R2 delivered every deliverable within R2 scope: the wasm shim is correct
(`gqlmin_alloc`, `gqlmin_free`, `gqlmin_parse`, `gqlmin_result_free` all present),
the smoke test passes (simple_query → success tag=0; empty_selection_error → parse
error kind=34), all 38 R1 tests continue to pass, SIZE.md is written with honest
measurements, and the CI gate workflow is in place. The Iron Law was correctly
invoked: instead of blind tuning, Builder authored a full root-cause document
(`investigation-r2-wasm-size.md`) and exhausted the documented risk ladder before
surfacing. This is the right behavior.

**The root cause is architectural, not a Builder error.** The Architect's ~1,500-byte
parser estimate implicitly assumed a non-Vec design (bump-allocated or span-indexed
AST). The R1 implementation used `Vec<T>` for all seven list types in the AST. That
divergence between plan and implementation is what produced the 3x overshoot — not a
mistake in the wasm shim, the build profile, or the allocator selection.

**Is the ≤5 KB gz thesis still achievable?** Yes, conditionally. Fix A (bumpalo
arena) has a high probability of landing in the 4,000–7,000 byte range, with the
lower end achievable if the single-monomorphization gain is as large as the Vec
analysis implies. Fix B (span-indexed parser rewrite) has an even stronger expected
outcome (3,000–5,000 bytes) but requires substantially more implementation work.
Neither estimate is measured — both require a Builder R3 pass to confirm. The thesis
is **achievable but contingent on a design change**; revising the target to 8 KB
would be the conservative path if the user needs a time-bounded guarantee.

R2 is **PARTIAL, not FAILING**. The build-out defect class iteration counter
advances to 2/5. The surface condition "Wasm size budget cannot be met with the
documented fallback ladder" is active.

---

## 2. Routing for synthesis

The following findings from R2 are durable enough to enter the living summary:

**Add to summary:**

- **Vec monomorphization as the structural size root cause.** The seven `Vec<T>`
  types in the ops parser generate ~10 KB gz of monomorphized grow/drop/realloc
  code. This is the primary reason the risk ladder was exhausted. Any future
  maintainer adding a new list type to the AST needs to know this constraint exists
  and why.

- **Smoke test passing — wasm ABI confirmed working.** The four wasm exports are
  wired correctly. The binary-level ABI (tag=0 success, tag=1 + kind byte for
  error) is established by the smoke test. This fact is durable: future rounds that
  modify the parser must not break it.

- **Risk ladder results table.** The six rungs tried (with gz measurements) should
  be preserved in the summary so future rounds do not repeat them. Reference to
  SIZE.md is sufficient; the Synthesizer may embed the table directly.

- **The three fix paths and their tradeoffs** (Fix A / B / C) belong in the summary
  as the "open decision" block until one is chosen and acted on.

**Keep in investigation doc only (not summary-worthy):**

- The specific function-count analysis from `wasm-dis` (150 functions in
  release-wasm) — too implementation-detail-specific for the living plan.
- The `from_utf8_unchecked` change rationale — already captured as a code comment.

---

## 3. Surface note

**— RELAY THIS SECTION TO THE USER VERBATIM —**

---

**magna-gqlmin R2 — wasm size surface**

Here is the current state and a decision you need to make.

**What was achieved in R2:**

The wasm build works. All four exports (`gqlmin_alloc`, `gqlmin_free`,
`gqlmin_parse`, `gqlmin_result_free`) are present and correctly wired. The smoke
test confirms the wasm ABI is sound: `simple_query.graphql` returns success (tag=0),
`empty_selection_error.graphql` returns a parse error (tag=1, kind=34). All 38
tests from R1 continue to pass. SIZE.md is written with honest, reproducible
measurements. The CI size gate workflow is in place and will fail PRs that exceed
5,120 bytes gz. Every R2 deliverable is complete except the size budget itself.

**Why the budget cannot be met without a design change:**

The operations parser uses seven distinct `Vec<T>` types (`Vec<Definition>`,
`Vec<VariableDefinition>`, `Vec<Directive>`, `Vec<Argument>`, `Vec<Selection>`,
`Vec<ObjectField>`, `Vec<Value>`). In the wasm32 target, each distinct `Vec<T>`
generates its own monomorphized copy of the grow, drop, and realloc machinery —
approximately 100+ functions in the release binary. This produces ~10 KB gz of
allocator-adjacent code that the Architect's estimate did not anticipate, because
that estimate assumed a bump-allocated or span-indexed design where list operations
share a single monomorphization. The full documented risk ladder (allocator swap,
Debug/Display gating, unchecked UTF-8) was exhausted: best achieved is 13,978 bytes
gz with `wee_alloc`, which is still 2.7x over the 5,120-byte budget and nearly 2x
over the 7,000-byte Iron Law ceiling. The overshoot is structural.

**Three options — choose one:**

**Option A — bumpalo arena (recommended)**

Replace the seven `Vec<T>` fields in the parser with
`bumpalo::collections::Vec<'bump, T>`. Because all bumpalo vecs share a single
allocator monomorphization, the ~10 KB of per-type grow/drop code collapses to one
copy. Estimated gz: 4,000–7,000 bytes (unconfirmed; requires Builder R3 to measure).
Confidence: medium — the monomorphization analysis is solid, but the bumpalo
overhead and residual allocator size need measurement.

Concrete changes: one new optional runtime dep (`bumpalo`, ~500 bytes gz); the
public API type `Document<'src>` gains a second lifetime parameter, becoming
`Document<'src, 'bump>`. This is a breaking change in the 0.x experimental API,
which is explicitly allowed under the current stability policy ("free-to-break in
0.x minors"). All callers of `parse_executable_document` would need to be updated.
Estimated Builder effort: medium — existing parser logic is preserved, only the
collection type changes and the arena is threaded through.

**Option B — span-indexed parser rewrite**

Replace all `Vec<T>` fields with a flat backing array of type-erased bytes and
index-based references into it. Each AST node holds integer indices instead of
owned Vecs. Expected gz: 3,000–5,000 bytes. No new runtime dep. No public API
lifetime change (the public types still expose slices; the internal representation
changes). Confidence: medium — this is the design the Architect's estimate implicitly
assumed, so the target is plausible, but the full parser must be rewritten.
Estimated Builder effort: high — this is a complete implementation replacement, not
an incremental change. Carries more schedule risk than Option A.

**Option C — revise the size target**

Accept that a full operations parser with dynamic Vec allocation cannot hit 5,120
bytes gz on a stable toolchain without `build-std` (nightly). Proposed revised
targets: use `wee_alloc`, aim for ≤8,192 bytes gz for the full ops parser wasm
build; provide a lexer-only build feature at ≤3,072 bytes gz for the strictest
size-budget consumers (parsing is then done on the JS side). The existing CI gate
threshold would be updated to 8,192 bytes, and a second gate for the lexer-only
build would be added at 3,072 bytes. This option trades architectural purity for
shipping speed and avoids any API change.

**Director's recommendation: Option A.**

Option A gives a high probability of meeting the original 5,120-byte budget with
one Builder round (R3), and the API change it introduces (adding a second lifetime
to `Document`) is acceptable under the 0.x stability policy. The bumpalo approach
is a well-understood pattern for arena-backed ASTs in Rust and does not require
rewriting the parser's logic — only its collection type. Option B is technically
cleaner (no new dep, no lifetime change) but the implementation cost is substantially
higher and the schedule risk is real; I would recommend it only if Option A comes
back from R3 still over budget. Option C is not wrong — if the 5 KB budget is a
soft heuristic rather than a hard deployment constraint, relaxing it saves time —
but Option A is likely to succeed in one focused round and should be tried first.

**What happens to steps 7–10 (napi, SDL, validation, pretty/serde) while this is
resolved:**

Steps 7–10 are not blocked by the wasm size issue. The wasm ABI is correct and
passing smoke tests; the only open question is binary size. A parallel team could
proceed with the napi binding (step 7) and SDL parser (step 8) immediately — neither
touches the wasm size path. I recommend proceeding with steps 7–10 in parallel
while the user chooses between Options A, B, and C. If Option A is chosen, Builder
R3 (arena rewrite) and Builder R3-parallel (napi/SDL) can run concurrently. If the
user wants a single-thread plan, hold steps 7–10 until Option A/B is resolved —
this is the safe choice but delays the overall build by one round.

**What I need from you:** a decision on Option A, B, or C. If you choose A or B,
also confirm whether you want steps 7–10 to proceed in parallel.

---
*End of user-facing surface note.*

---

## 4. Continuity check

- **Iteration counter:** 2/5 (R1 Verifier + R2 Verifier). Healthy. Three rounds
  remain before the 5-round hard stop on this defect class. If Option A is chosen,
  R3 should resolve the size issue in one round, leaving two rounds as buffer for
  any R3 partial result.

- **Work nature shift:** The work has shifted from "build to spec" (steps 1–6 of the
  12-step plan) to "fix structural mismatch" (the Vec vs. non-Vec design gap). This
  warrants a **budget reset** if we proceed with Fix A or B: the R3 brief should
  explicitly scope to the arena/rewrite change only, not attempt to also advance
  steps 7–10. Mixing the structural fix with new feature work in the same Builder
  round increases verifier complexity and risks partial attribution when something
  goes wrong.

- **No other anti-patterns detected.** Builder did not revise targets silently.
  The Iron Law was correctly invoked. The smoke test exercises the actual `.wasm`
  artifact (not the Rust-native parse path). The risk ladder was tried in order and
  documented honestly. Workspace compiles clean.

- **Next Director note trigger:** after Builder R3 Verifier completes (if Option A
  or B is chosen) or after the CI gate is updated (if Option C is chosen). No
  further surface is expected unless R3 comes back with gz still > 7,000 bytes.
