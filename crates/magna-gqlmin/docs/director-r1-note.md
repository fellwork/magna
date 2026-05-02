# Director R1 — magna-gqlmin

Round: R1 director note
Builder commits reviewed: `e4ccccb`..`d1fa28b`
Verifier report: `bb1507e` (verifier-r1-report.md)
Verdict input: PASS-WITH-FINDINGS (all MINOR/NIT)

## 1. On-thesis assessment

R1 is **on-thesis**. The work advances exactly steps 1–4 of the locked
12-step build order in `docs/topic-summaries/gqlmin-summary.md`
(skeleton + workspace registration, feature-flag matrix, DFA lexer, ops
parser + corpus). No drift into SDL, validation, napi, pretty errors,
or any post-step-4 work.

Strongest evidence — Verifier Section D (sample correctness):

- D1: 5/5 spec inputs (covering query, mutation w/ object+list args,
  fragment def + spread, variables with non-null-list types, alias +
  numeric values incl. `1.5e2` and `-0`) all parse without error.
- D2: 3/3 invalid inputs are rejected with the correct
  `ParseErrorKind` discriminants and span ranges:
    - `{ }` → `EmptySelectionSet @ 0..3`
    - `{ field(arg: ) }` → `ExpectedValue @ 13..14`
    - `query Q($v: ) { x }` → `ExpectedType @ 12..13`
- D3: `inline_fragment_no_type` is implemented as the spec-correct
  typeless `... Directives SelectionSet` form (Oct-2021 spec § 2.8.2),
  resolving Builder's open question — answer is **yes, correct**.

The bidirectional rejection signal is the load-bearing one: the parser
not only accepts valid grammar but also *refuses* invalid grammar with
the right error kind. That pre-empts the most common failure mode for
hand-rolled parsers (over-permissive accept-all-but-the-corpus).

Bans honored end-to-end: no `String` / `format!` / `unwrap` / `regex` /
unicode tables / `serde` / `wasm-bindgen` / `napi` in `src/`. The few
matches all live under `#[cfg(test)]`. Workspace stays clean
(`cargo check --workspace` exit 0). No file edits outside scope.

R1 advances the topic. Iteration counter for the build-out defect
class advances 0/5 → 1/5.

## 2. Routing for synthesis

Two items are durable enough to belong in the topic summary; one is
not. Recommend the Team Lead dispatch the Synthesizer with this
limited diff:

- **Add to summary — durable design decision.** The `ParseErrorKind`
  discriminant partition (lex = 1..=5, parse = 32..=43) is a
  cross-component contract: the JS-side decoder in the future
  `@magna/gqlmin-wasm` package needs to know it, and Builder R2 needs
  to keep the partition stable when adding new kinds. Suggested:
  Synthesizer adds a short subsection under "Public API" (or a new
  "Error code wire format" stub) saying "lex errors occupy 1..=5,
  parse errors occupy 32..=43; preserve the gap so JS decoders can
  branch on the high bit."

- **Add to summary — verified-against-spec note.** A one-line entry
  under "Status" (or a new "Round log" subsection) recording that R1
  delivered steps 1–4 with 5/5 valid + 3/3 invalid spec probes
  passing. This protects against later rounds silently regressing the
  ops parser without re-running the bidirectional probe.

- **Keep in verifier report only — not summary-worthy.** The two
  deferred `check-features.sh` combos and the span-clamp duplication
  NIT are R2 housekeeping; they live cleanly in the brief and the
  verifier findings without polluting the living plan.

If the Team Lead skips Synthesis this round, the Builder R2 brief in
section 5 below is self-sufficient — the discriminant partition note
is repeated as a carry-forward action item.

## 3. Priority for R2

I agree with the proposed order; one carve-out and one re-ordering:

1. **Highest:** wasm shim — `#[cfg(feature = "wasm")]` allocator
   (dlmalloc) + `#[panic_handler]` + `extern "C"` exports
   (`gqlmin_alloc`, `gqlmin_free`, `gqlmin_parse`, `gqlmin_result_free`).
   This is the unblocker for everything else in R2.
2. `release-wasm` profile in workspace `Cargo.toml`
   (`opt-level = "z"`, `lto = "fat"`, `codegen-units = 1`,
   `panic = "abort"`, `strip = "symbols"`). Architect plan already
   specifies this; locking it in now removes a measurement variable.
3. Measure baseline gz size; record in
   `crates/magna-gqlmin/SIZE.md` with rustc version, wasm-opt version,
   pipeline stages (raw → wasm-opt → gzip) and per-stage byte counts.
4. **Decision gate** based on size:
    - ≤ 5120 bytes → R2 passes the budget criterion.
    - 5121–7000 bytes → walk the risk ladder from the topic summary
      in order (dlmalloc tweaks → wee_alloc → drop block-string →
      accept 7 KB ceiling). Record each rung tried.
    - \> 7000 bytes → trigger Iron-Law investigation (see 5e); do not
      attempt blind fixes.
5. CI size gate workflow under `.github/workflows/` failing the PR
   when gz > 5120 bytes.
6. Re-enable the two deferred `check-features.sh` combos
   (`--no-default-features --features ops` and `ops,wasm`); add the
   three combos Verifier flagged (`ops,pretty`, anything pulling
   `napi`, `wasm-bindgen`) but only at the level R2 actually exercises
   them — a `cargo check` per combo, not full builds.

**Carve-out:** the `gqlmin_result_free` export and the binary-AST
encoding format only need a *minimal* form for R2 — enough that the
smoke test in 5c can call `gqlmin_parse` and observe a non-error
result code. Full encoding lives with the JS-side decoder package and
is deferred to whenever that package starts. This keeps R2 from
expanding into JS-decoder design.

**Re-ordering rationale:** the `release-wasm` profile must be
committed *before* the size measurement, otherwise the SIZE.md
baseline measures something that will move under our feet on the next
profile change.

## 4. Scope signal

**continue.**

Defect class is unchanged ("build-out: implement the locked 12-step
plan"). We are still on step 5/6 of the 12-step plan; no pivot.
Iteration counter for this defect class advances to **1/5** on
completion of Verifier R1 → would advance to **2/5** on Verifier R2.
No surface conditions from `state-gqlmin.md` triggered.

## 5. Refined brief for Builder R2

### 5a. Concrete deliverables for R2

Files that MUST exist at end of R2:

- `crates/magna-gqlmin/src/wasm.rs` (or equivalent module wired into
  `lib.rs` under `#[cfg(feature = "wasm")]`) containing:
    - dlmalloc-backed `#[global_allocator]`.
    - `#[panic_handler]` (abort).
    - `extern "C"` exports: `gqlmin_alloc(usize) -> *mut u8`,
      `gqlmin_free(*mut u8, usize)`, `gqlmin_parse(*const u8, usize)
      -> *const u8` (or equivalent error-or-result-pointer ABI),
      `gqlmin_result_free(*const u8)`.
- `crates/magna-gqlmin/Cargo.toml`:
    - `wasm` feature now pulls `dlmalloc` as an optional dep gated on
      `feature = "wasm"`.
    - The empty `[dependencies]` block grows by exactly one entry:
      `dlmalloc = { version = "...", optional = true,
      default-features = false }`. No other runtime deps.
- Workspace `Cargo.toml`:
    - `[profile.release-wasm]` block with `inherits = "release"`,
      `opt-level = "z"`, `lto = "fat"`, `codegen-units = 1`,
      `panic = "abort"`, `strip = "symbols"`.
- `crates/magna-gqlmin/SIZE.md` with:
    - rustc version (`rustc -V`), wasm-opt version
      (`wasm-opt --version`).
    - Build command used.
    - Pipeline byte counts: raw `.wasm`, post `wasm-opt -Oz
      --strip-debug --vacuum`, post `gzip -9`.
    - Architect estimate vs. measured per-component if reasonably
      attributable; otherwise just the totals.
    - Risk-ladder rung reached, if any.
- `.github/workflows/gqlmin-size.yml` (or addition to an existing
  workflow) that:
    - Installs the wasm32 target and `wasm-opt` (binaryen).
    - Runs the build pipeline.
    - Fails when `gzip -9 -c <wasm> | wc -c` > 5120.
- `crates/magna-gqlmin/scripts/check-features.sh` updated to
  re-enable the 2 deferred combos and add the 3 Verifier-flagged ones
  (see 5g).
- `crates/magna-gqlmin/src/error.rs`: comment block above
  `ParseErrorKind` documenting the `1..=5` lex / `32..=43` parse
  partition and the rule for adding new kinds (see 5g).
- A smoke test artifact (see 5c). Location flexible — preferred is
  `crates/magna-gqlmin/tests/wasm_smoke.rs` gated on a presence-check
  for the built `.wasm` file, OR a small Node script under
  `crates/magna-gqlmin/scripts/wasm-smoke.mjs` invoked by the CI
  workflow.

Out of scope for R2 (do NOT create): napi binding, SDL parser,
validation, pretty errors, the JS decoder package, the binary-AST
format spec.

### 5b. Acceptance criteria, runnable

R2 PASSES if every command below exits 0 / produces the stated
artifact:

```bash
# 1. Wasm builds
cargo build -p magna-gqlmin \
  --target wasm32-unknown-unknown \
  --no-default-features --features "ops,wasm" \
  --profile release-wasm
# Produces target/wasm32-unknown-unknown/release-wasm/magna_gqlmin.wasm

# 2. Pipeline runs and records baseline
wasm-opt -Oz --strip-debug --vacuum \
  target/wasm32-unknown-unknown/release-wasm/magna_gqlmin.wasm \
  -o /tmp/gqlmin.opt.wasm
gzip -9 -c /tmp/gqlmin.opt.wasm | wc -c
# Number recorded in crates/magna-gqlmin/SIZE.md

# 3. Workspace still clean
cargo check --workspace

# 4. All R1 tests still pass
cargo test -p magna-gqlmin

# 5. Feature combo script passes (active rows)
crates/magna-gqlmin/scripts/check-features.sh

# 6. CI size gate workflow passes locally (size <= 5120) OR
#    risk-ladder rung is documented in SIZE.md justifying the
#    measured size.
```

Size verdict bands:

- **≤ 5120 bytes gz** → R2 passes the budget criterion outright.
- **5121–7000 bytes gz** → R2 may pass *only if* SIZE.md documents
  which risk-ladder rungs were tried, in order, and which one the
  final number reflects (or notes the 7 KB ceiling decision).
- **\> 7000 bytes gz** → R2 must surface (see 5f).

### 5c. Sample-based assertions

Provide at least one of the following two probes; preferred is (i):

(i) **Node-based wasm smoke test.** A short script (`scripts/
wasm-smoke.mjs` or similar) that:

  - Reads the built+opt'd `magna_gqlmin.wasm`.
  - Instantiates with no imports (or a minimal stub for any required
    intrinsic).
  - Calls `gqlmin_alloc(N)`, writes the bytes of
    `tests/corpus/simple_query.graphql` into linear memory at the
    returned pointer, calls `gqlmin_parse(ptr, N)`, asserts the
    result indicates success (non-error sentinel per the chosen
    minimal ABI), calls `gqlmin_result_free` and `gqlmin_free`.
  - Repeats with `tests/corpus/empty_selection_error.graphql` and
    asserts the result indicates error (any error sentinel — full
    decoding is JS-decoder-package work, out of scope).

(ii) **Rust integration test.** A separate-binary test compiled to
the host target that uses `wasmtime` or `wasmer` *as a dev-dep only*
(NOT a runtime dep) to load the wasm and run the same two probes.
Acceptable but adds a dev-dep; (i) is preferred to keep the smoke
test in the same toolchain CI already needs.

The probe MUST NOT be `cargo test -p magna-gqlmin` running a Rust-
native parse — that proves nothing about the wasm path. It must
exercise the actual `.wasm` artifact.

### 5d. Bans for R2

In addition to all R1 bans:

- No `wasm-bindgen` import. The `wasm-bindgen` feature stays declared
  but unused; it is the escape-hatch behind a separate feature, not
  the default wasm path.
- No `napi`, `napi-derive`, or `napi-build` work — that's R3.
- No SDL parser, no `sdl` module — that's R8.
- No validation work — that's R9.
- No `pretty`/`miette`/`ariadne`-style error rendering — that's R10.
- No JS decoder package work, no `@magna/gqlmin-wasm` directory, no
  TypeScript files outside the smoke-test script.
- No raising the size budget. 5120 bytes is locked. If the measured
  size > 5120, walk the risk ladder; do not silently relax the gate.
- No new runtime deps beyond `dlmalloc` (and only when
  `feature = "wasm"`). The `[dependencies]` block grows by exactly
  one entry.
- No `String` in non-test `src/` (R1 ban carries forward and is
  especially load-bearing for the wasm shim — a stray `format!` in
  the panic handler will torpedo the size budget).

### 5e. Iron-Law trigger

If either of the following conditions holds, Builder MUST author
`crates/magna-gqlmin/docs/investigation-r2-<slug>.md` *before*
attempting any fix:

- The wasm target fails to compile (link error, missing intrinsic,
  panic-handler conflict, allocator wiring error, feature-gate leak
  pulling `std` into the wasm build).
- The measured gzipped size is > 7000 bytes (≥ ~37% over budget).
  This is the threshold above which the cause is almost certainly
  *structural* (LLVM stdlib pull-in, feature-gate leakage, an
  allocator behaving badly, or the panic handler dragging in
  formatter code) rather than tunable.

The investigation document must:

1. State the symptom precisely with reproduce commands.
2. Identify candidate causes (allocator? panic_handler? std leak?
   compiler-builtins? cdylib metadata?).
3. For each candidate, the cheapest disprove-or-confirm experiment.
4. Only then propose a fix.

Iron Law applies because "wasm overshoots budget by 4x" is an
ambiguous defect class spanning at least four very different root
causes; blind tuning will burn iteration budget.

### 5f. Surface-to-user triggers for R2

R2 must surface to the user (stop, write surface note, await
guidance) when any of the following holds:

- Measured gz size > 7000 bytes after the risk-ladder rungs from the
  topic summary have been exhausted (i.e., we cannot meet the 7 KB
  ceiling). This crosses the size-budget surface condition in
  `state-gqlmin.md`.
- `wasm-opt` is unavailable in the agent's environment and cannot be
  installed via the standard package paths (`apt`, `cargo install
  wasm-opt`, etc.). Without it the size measurement is meaningless;
  surface rather than fake the number.
- `rustup target add wasm32-unknown-unknown` fails on the agent's
  toolchain.
- Build pulls in an LLVM intrinsic dependency that requires
  `compiler-builtins-mem`, `build-std`, or another non-trivial
  bootstrap step the architect plan didn't anticipate. This is a
  scope-shift trigger.
- The Iron-Law investigation document concludes the cause requires
  user input (e.g., "drop block-string parsing entirely from the
  wasm build" — that's a user-visible API change).

### 5g. Carry-forward findings from Verifier R1

- **Add `ops,pretty`, `ops,serde,napi`, and `wasm-bindgen` combos to
  `scripts/check-features.sh`.** R2 enables them at whatever level
  the corresponding code actually exercises (`cargo check` is
  acceptable for combos that gate no compiled paths yet).
- **Document the `ParseErrorKind` discriminant partition.** Add a
  comment block above `ParseErrorKind` in `src/error.rs` explaining:
  (a) lex errors occupy `1..=5`; (b) parse errors occupy `32..=43`;
  (c) the gap is intentional so the JS decoder can branch on whether
  the kind is < 32 to dispatch lex- vs. parse-error rendering;
  (d) when adding a new kind, append within its range — do not fill
  the gap.
- **Skip the span-clamp duplication NIT.** Defer to a R5 cleanup pass
  or never; not worth a R2 round-trip and the duplication has zero
  size impact.

### 5h. Out-of-scope for R2 (hard list)

These are explicitly out of scope. Scope creep here triggers
Verifier rejection.

- napi binding (R3 / step 7)
- SDL parser (R4 / step 8)
- Validation rules (R5 / step 9)
- Pretty error rendering + serde derives (R6 / step 10)
- SFC compiler integration (DEFERRED, step 11 — not this session)
- Publish prep, README expansion beyond stability + license,
  CHANGELOG, GOVERNANCE row (R7+ / step 12)
- JS-side decoder package (`@magna/gqlmin-wasm`)
- Binary AST encoding format spec
- Refactoring R1 code unless required to pass R2 acceptance (e.g.,
  adding `#[cfg(feature = "std")]` gates that turn out to be missing
  for the no-default-features build — those are in scope as defects,
  not refactors)

Lessons #4 and #5 (scope creep) apply: be hard about this list.

## 6. Surface-to-user triggers (right-now)

**None — proceed to R2.**

No Verifier-flagged item rises to a user-judgment question. All four
findings are MINOR/NIT and resolve in the R2 brief above. No surface
condition from `state-gqlmin.md` is triggered: Builder did not report
BLOCKED, Verifier did not report BLOCKED, defect class has not hit
5-round non-convergence (we are 1/5), no workspace breakage, no
license/legal question, no scope shift. All locked decisions hold.

## 7. Continuity check (anti-patterns)

- **Did the Builder revise targets?** No. R1 scope was steps 1–4 of
  the locked plan. Builder delivered exactly those steps. No quiet
  target revision; the `release-wasm` profile and 5 KB budget are
  R2's job and remain unchanged in the locked summary.
- **Sample-level failures hidden by aggregate stats?** No. Verifier
  Section D ran direct probes on 5 valid + 3 invalid inputs through
  a separate consumer crate, not the in-tree corpus. 5/5 and 3/3
  green with the correct error kinds. The `inline_fragment_no_type`
  spec-correctness was checked structurally in addition to the test
  assertion. Aggregate "38 tests pass" is *not* the only signal here.
- **Acceptance items silently deferred?** Two `check-features.sh`
  combos (`--no-default-features --features ops` and `ops,wasm`)
  were deferred. The deferral is **acceptable, not slippage**: both
  combos require the wasm shim (allocator + panic_handler) which is
  R2's explicit step-5 work. Builder flagged the deferral with a
  `TODO(R2)` comment in the script. R2 brief carries forward the
  re-enable as a hard deliverable. No other deferrals.
- **Has the work nature shifted?** No. Still in step 4 → step 5 of
  the 12-step build-out plan. Same defect class, same constraints.
- **Same defect class hitting iteration ceiling?** No. Build-out
  defect class is at 1/5 after R1 PASS. Healthy headroom.

No anti-patterns triggered. Continue to R2.
