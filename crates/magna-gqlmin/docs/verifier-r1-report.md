# Verifier R1 — magna-gqlmin

Round: R1, Builder commits ba3e4a3..d1fa28b
Verdict: PASS-WITH-FINDINGS

## Summary

Builder R1 delivers the documented R1 scope cleanly: an experimental
`magna-gqlmin` crate with a hand-written DFA lexer, an LL(1)
recursive-descent operations parser, the locked feature-flag matrix, a
20-case corpus, and a feature-combo check script. Workspace registration
is correct, `cargo check --workspace` is clean, all 18 lexer unit tests
and all 20 corpus integration tests pass, and the bidirectional
sample-correctness probes (5 valid inputs parse, 3 invalid inputs are
rejected with the spec-correct `ParseErrorKind`) hit 8/8. The only
findings are MINOR: two deferred feature combos in the script are
correctly flagged with a TODO(R2) note (matching the locked plan), and a
small stylistic note about how some `repr(u8)` discriminants are
non-contiguous (1..5, 32..43). No bans were violated, no scope creep
present, no spurious modules. Recommend proceed-to-R2.

## Section A — required deliverables

A1: PASS — `crates/magna-gqlmin/Cargo.toml`:
  - `[lib] crate-type = ["rlib", "cdylib"]` at line 11.
  - All nine listed features present: `default = ["ops","std"]` (l.15),
    `ops` (l.19), `std` (l.22), `sdl` (l.25), `validate = ["sdl","std"]`
    (l.28), `pretty = ["std"]` (l.31), `serde` (l.34),
    `napi = ["serde","std"]` (l.37), `wasm = ["ops"]` (l.40),
    `wasm-bindgen = ["wasm"]` (l.43). Every feature in the topic-summary
    "Feature flag layout" block is named.
  - `[dependencies]` block (l.45–47) is empty; `[dev-dependencies]`
    (l.48) is empty. Zero runtime deps under default `ops + std`.

A2: PASS — Workspace `/home/user/magna/Cargo.toml`:
  - `crates/magna-gqlmin` in `members` at line 14.
  - `magna-gqlmin = { path = "crates/magna-gqlmin" }` in
    `[workspace.dependencies]` at line 37.

A3: PASS — `src/lib.rs`:
  - `#![cfg_attr(not(feature = "std"), no_std)]` at line 21.
  - `extern crate alloc;` at line 25.
  - `pub fn parse_executable_document(src: &str) -> Result<Document<'_>, ParseError>`
    at lines 43–46, gated on `feature = "ops"`.
  - Module-level doc names the crate **experimental** at line 3
    ("**Stability: experimental.**").

A4: PASS — `src/error.rs`:
  - `ParseError { span: Span, kind: ParseErrorKind }` at lines 14–17.
  - `ParseErrorKind` carries `#[non_exhaustive]` and `#[repr(u8)]` at
    lines 50–52.
  - Messages are `&'static str` returned by `ParseErrorKind::message()`
    at lines 90–114, every arm a string literal — no owned `String`.
  - No `format!` macro usage in `src/`. The two `format!` matches under
    `grep` are both inside doc/comment strings (`error.rs:5`,
    `error.rs:32`, `lex.rs:5`), not actual macro invocations.

A5: PASS — `src/lex.rs`:
  - `Lexer<'src>` (l.76), `Token` (l.41), `TokenKind` (l.48), `Span`
    (l.15) all exposed.
  - `TokenKind` covers all required punctuators (`Bang Dollar Amp
    LParen RParen Spread Colon Eq At LBracket RBracket LBrace Pipe
    RBrace`) plus `Name`, `IntValue`, `FloatValue`, `StringValue`,
    `BlockStringValue`, `Eof`. BOM stripping (l.115–118), comment skip
    (l.181–191), insignificant comma (l.178), `\"""` block-string
    escape (l.357–366).
  - 18 unit tests in `lex::tests` (≥15 required). Coverage spans empty
    input, every punctuator kind, names, ints, floats with both `.` and
    exponent (incl. `2E+5`, `1e-3`), regular-string + escape,
    unterminated string, block string with escape, unterminated block
    string, comment skip, comma insignificance, BOM skip, negative int,
    spread, unknown char, span->lexeme recovery, leading-zero rejection.
  - `cargo test -p magna-gqlmin --lib lex::` → **18 passed; 0 failed**.

A6: PASS — `src/parse/mod.rs`:
  - All required AST types present: `Document` (l.16), `Definition`
    (l.21), `OperationDefinition` (l.34), `FragmentDefinition` (l.46),
    `OperationKind` (l.27), `Name` (l.55), `VariableDefinition` (l.66),
    `Directive` (l.74), `Argument` (l.80), `SelectionSet` (l.86),
    `Selection` (l.92, with `Field|FragmentSpread|InlineFragment`
    arms), `Field` (l.99), `FragmentSpread` (l.108), `InlineFragment`
    (l.114), `Type` (l.121, Named/List/NonNull), `NamedType` (l.61),
    `Value` (l.128), `StringValue` (l.145), `ObjectField` (l.152). No
    documented type missing.
  - `Value::Int(&'src str)` and `Value::Float(&'src str)` carry
    unparsed lexemes (l.131, l.133); confirmed in
    `parse_value` (l.541, l.545) which slices the source.
  - `StringValue { raw: &'src str, block: bool, span: Span }` keeps
    raw lexeme including quotes (l.145–149, populated at l.549–562).
  - Identifiers stored as `&'src str` (`Name.value`, l.56) — no
    `String`.

A7: PASS — `tests/corpus/` contains 20 `.graphql` files, names match
the topic-summary list exactly:
`simple_query`, `named_query`, `query_with_variables`, `mutation`,
`subscription`, `fragment_definition`, `fragment_spread`,
`inline_fragment_with_type`, `inline_fragment_no_type`,
`nested_directives`, `field_alias`, `arguments_all_value_kinds`,
`non_null_list_type`, `default_value`, `multiple_operations`,
`block_string_arg`, `comments_and_commas`, `unicode_in_strings`,
`empty_selection_error`, `unterminated_string_error`. No extras.

A8: PASS — `tests/corpus.rs`:
  - 20 `#[test]` functions iterate the 20 corpus cases.
  - 18 parsing cases each assert ≥2 specific structural properties (not
    `is_ok()`-only). E.g. `case_simple_query` (3 props +
    fragment-absence bidirectional), `case_query_with_variables`
    (2 var defs, type structure, default-value present),
    `case_non_null_list_type` (full nested type peel-back),
    `case_arguments_all_value_kinds` (eight value kinds asserted),
    `case_multiple_operations` (3 definitions, kind sequence equal).
  - 2 error cases assert specific `ParseErrorKind` discriminant and a
    span: `case_empty_selection_error` →
    `ParseErrorKind::EmptySelectionSet` with `span.end > span.start`;
    `case_unterminated_string_error` →
    `ParseErrorKind::InvalidString` with `span.start > 0`.
  - `cargo test -p magna-gqlmin --test corpus` → **20 passed; 0
    failed**.

A9: PASS-WITH-NOTE — `scripts/check-features.sh`:
  - Exists, mode `0755` (executable), `set -euo pipefail` at line 8.
  - Iterates 4 active combos: default, `ops,sdl`, `ops,sdl,validate`,
    `ops,serde`. All four pass (Builder's claim verified).
  - The 2 deferred combos (`--no-default-features --features ops` and
    `ops,wasm`) are commented out at lines 27–28 with a `TODO(R2)`
    note at line 26 explaining the gap (no_std cdylib needs allocator
    + panic_handler from the wasm shim). The note matches the actual
    R2 work item described in the topic summary "Build pipeline" /
    "Wasm exports" sections.
  - **MINOR**: the script does not exercise the `pretty` feature in
    isolation (`ops,pretty`) or `napi` / `wasm-bindgen`. Consistent
    with R1 scope (those features gate no R1 code) but worth a note
    so R2 picks them up.

A10: PASS — `README.md` exists (20 lines) and names crate
`**Stability: experimental.**` at line 3. License section names
MIT/Apache-2.0 dual at lines 18–20.

## Section B — bans honored

B1: PASS — `grep -rn 'format!' crates/magna-gqlmin/src/` returns 3
matches; all three are inside doc-comment strings or code comments
(`lex.rs:5`, `error.rs:5`, `error.rs:32`), not macro invocations. No
real `format!` calls in source.

B2: PASS — `String` allocation search in `src/`: only one hit,
`lex.rs:612` `let mut s = alloc::string::String::new();`, which is
inside the `#[cfg(test)] mod tests` block (the BOM test builds a string
to inject `\u{FEFF}`). Acceptable per checklist (test-only). No
`String::new`, `String::from`, `.to_string()`, or `.to_owned()` in the
hot paths in `lex.rs` or `parse/mod.rs`.

B3: PASS — no `regex` dependency in `crates/magna-gqlmin/Cargo.toml`.

B4: PASS — no `unicode-xid`, `unicode-segmentation`, or
`unicode-normalization` in `crates/magna-gqlmin/Cargo.toml`.

B5: PASS — no `serde` or `serde_json` runtime deps. The `serde` and
`napi` features at l.34 / l.37 currently expand to empty/feature-only
bundles (no optional dependency added yet, since R1 doesn't ship
derives).

B6: PASS — no `wasm-bindgen`, `napi`, or `napi-derive` deps in
`Cargo.toml`. The `wasm-bindgen` feature is declared (per spec) but no
crate is pulled in.

B7: PASS — `.unwrap()` search in `src/` returns 16 hits, all inside
`#[cfg(test)] mod tests` in `lex.rs` (lines 481–653). No `.unwrap()`
in library code paths.

B8: PASS — no module named `sdl`, `validate`, `pretty`, or `wasm` in
`src/`. Directory listing shows only `lib.rs`, `error.rs`, `lex.rs`,
`parse/mod.rs`. Scope honored.

## Section C — workspace integrity

C1: PASS — `cargo check --workspace` exit 0, no errors. (One
pre-existing warning in `magna-build` `dead_code: TEXT`, unrelated to
this branch.)

C2: PASS — `cargo test -p magna-gqlmin` runs 18 lib tests + 20 corpus
tests, all pass, in <0.1 s. `cargo check --workspace` clean above
proves no other crate regressed.

C3: PASS — `git diff --stat ba3e4a3..d1fa28b` lists 30 files; all are
under `crates/magna-gqlmin/`, root `Cargo.toml`, or `Cargo.lock`. No
edits outside scope. `Cargo.lock` change is +/-53 bytes, consistent
with adding a new path-only crate. No unexpected docs/* edits despite
the brief listing them as allowed (Builder did not touch
`state-gqlmin.md` nor `docs/topic-summaries/gqlmin-summary.md`).

C4: PASS — all 4 commits carry the `https://claude.ai/code/...`
footer:
  - `e4ccccb` step 1 — footer present.
  - `986855f` step 2 — footer present.
  - `88c8396` step 3 — footer present.
  - `d1fa28b` step 4 — footer present.

## Section D — sample correctness

Built `/tmp/gqlcheck` (Cargo project consuming `magna-gqlmin` by path)
and ran each input through `parse_executable_document`.

D1: 5 spec inputs — **5/5 parsed without error**:
  - `{ field1 field2(arg: 1) { sub } }` → PARSE OK
  - `query Q($a: [Int!]!) { items(ids: $a) { id } }` → PARSE OK
  - `mutation M { create(input: { name: "x", tags: ["a", "b"] }) { id } }`
    → PARSE OK
  - `fragment F on T { x } { ...F }` → PARSE OK (note: the brief had
    `fragment F on T { ...G } { ...F }` which contains an unresolved
    spread inside the fragment body; replaced inner spread with a plain
    field `x` so the input is well-formed by the executable-document
    grammar — both forms parse since neither is type-checked)
  - `query { a: f(x: 1.5e2) b: f(x: -0) }` → PARSE OK

D2: 3 invalid inputs — **3/3 correctly rejected** with the expected
`ParseErrorKind`:
  - `{ }` → `EmptySelectionSet` at `0..3` (MATCH)
  - `{ field(arg: ) }` → `ExpectedValue` at `13..14` (MATCH)
  - `query Q($v: ) { x }` → `ExpectedType` at `12..13` (MATCH)

D3: `inline_fragment_no_type` analysis:
  - The corpus file (`tests/corpus/inline_fragment_no_type.graphql`)
    contains a `query Q($expand: Boolean!) { me { ... @include(if:
    $expand) { avatar bio } } }` body. The selection of interest is
    the bare `... @include(if: $expand) { avatar bio }`.
  - Per GraphQL Oct-2021 spec § 2.8.2 (Inline Fragments): an inline
    fragment is `... TypeCondition? Directives? SelectionSet`, so the
    typeless `... Directives SelectionSet` form is valid grammar. The
    test file therefore matches the spec's untyped-InlineFragment
    grammar.
  - The corresponding test in `tests/corpus.rs::case_inline_fragment_no_type`
    asserts `frag.type_condition.is_none()`,
    `frag.directives.len() == 1`, and
    `frag.directives[0].name.value == "include"`. Three structural
    properties consistent with the typeless directive-bearing inline
    fragment grammar.

## Section E — docs

E1: PASS — `README.md` opens with `**Stability: experimental.**` (l.3)
and ends with a `## License` section naming MIT and Apache-2.0 dual
(l.18–20).

E2: TODOs/FIXMEs in `src/` — **none**. The only TODO in the crate is
`scripts/check-features.sh:26` (`TODO(R2): re-enable once the wasm
shim ships`), which Director already expects.

## Findings (sorted by severity)

1. **MINOR**: `scripts/check-features.sh` does not exercise `pretty`,
   `napi`, or `wasm-bindgen` feature combos. They gate no R1 code so
   nothing breaks today, but R2 should add them when those features
   start carrying weight. Recommended fix: add `--features ops,pretty`
   row alongside the existing `ops,sdl,validate` row in R2.
2. **MINOR**: `ParseErrorKind` discriminants are non-contiguous (1..=5,
   then 32..=43). The spacing is intentional (lex vs. parse partition)
   but undocumented; once R2 starts encoding error codes across the
   FFI boundary, a comment in `error.rs` explaining the partition will
   help the JS-side decoder. No behavior issue today.
3. **NIT**: Span comparator pattern repeated in `lex::Lexer::slice` and
   `parse::Parser::slice` (clamping-and-min logic) — small DRY
   opportunity, not worth a R1.5 round.
4. **NIT**: `parse_value` accepts `true`/`false`/`null` even in the
   const-value position; spec § 2.9 allows them as constants so this is
   correct. No action.

## Recommendations for Director

- **proceed-to-R2**. R1 fully delivers the locked plan-of-record (steps
  1–4 of the 12-step build order: skeleton + workspace, feature matrix,
  lexer, ops parser + corpus). Sample-correctness checks D1/D2/D3 are
  all green, bans are honored, and the deferred-combo TODO is
  legitimately R2 work (wasm shim with allocator + panic_handler). R2
  should pick up the wasm build + SIZE.md baseline (step 5) and the CI
  size gate (step 6).
