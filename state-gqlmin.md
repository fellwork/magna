# state-gqlmin

Single-pointer state for the `magna-gqlmin` track. Human-reviewable. Authored
by Team Lead and updated by Historian at end of each session.

## Topic

Build a lightweight GraphQL parser crate `magna-gqlmin` with three
distribution modes: wasm32 runtime (≤5 KB gz), napi-rs binding for Node/Bun,
and a native Rust dependency for build-time tooling.

Plan-of-record: see `docs/topic-summaries/gqlmin-summary.md`.

## Mode

Mode 2 — Build / refactor (L-scope). Iteration budget: hard-stop at 5
Builder ↔ Verifier rounds per defect class.

## Branch

`claude/graphql-parser-lightweight-b6Vi8` (single repo, no cross-repo work).

## Locked decisions (autonomous-mode defaults)

- **Crate name**: `magna-gqlmin`
- **Stability**: `experimental`
- **AST stability**: free-to-break in 0.x minors per GOVERNANCE
- **Wasm target**: generic `wasm32-unknown-unknown`, optimize for browsers
- **Validation rules (initial)**: 10 starter rules per spec
- **SFC compiler integration**: out-of-scope this session (compiler does
  not yet exist in the workspace)

## Surface conditions (autonomous mode)

Surface to user when any of:
- Builder reports BLOCKED with no path forward
- Verifier reports BLOCKED with no path forward
- 5-round Builder ↔ Verifier non-convergence on the same defect class
- Wasm size budget cannot be met with the documented fallback ladder
- Workspace breakage (`cargo check --workspace` fails on existing crates)
- License/legal question outside prior guidance
- Scope shift requiring user judgment

## Status

Session 1 — kicking off. No rounds run yet.

## Iteration counter

Defect class: build-out (initial implementation). Rounds: 0/5.

## Open work (from Architect plan, 12 steps)

1. Empty crate skeleton + workspace registration
2. Feature flag matrix
3. Lexer
4. Operations parser + 20-case corpus
5. First wasm build + baseline size in SIZE.md
6. CI size gate
7. napi binding
8. SDL parser
9. Validation rules (10 starter)
10. Pretty errors + serde derives
11. Wire into SFC compiler — DEFERRED (compiler does not exist)
12. Publish prep

## Definition of done (this session)

Stretch goal: steps 1–10 complete, wasm gz size measured under 5120 bytes
(or ladder-fallback documented), workspace `cargo check` clean, all corpus
tests pass.

Minimum: steps 1–5 (parser builds for wasm + size measured) plus step 6
(CI gate enforcing the budget).
