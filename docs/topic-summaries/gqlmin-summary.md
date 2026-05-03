# gqlmin — topic summary (living document)

Living document the Synthesizer updates after each Director routes findings.
Read this *first* before briefing any Researcher.

## Thesis

A new workspace crate `magna-gqlmin` that parses GraphQL with three
distribution modes from a single Rust source:

1. **wasm32-unknown-unknown** — operations-only, MUST be ≤5 KB gz post
   `wasm-opt -Oz` and `gzip -9`. This is the binding constraint.
2. **napi-rs** — same operations parser, called from Node/Bun.
3. **Native Rust** dependency — full SDL + validation, no size constraint,
   for build-time tooling (eventual SFC compiler consumer).

Integration is NEW AST + NEW consumers. The parser does NOT replace
`async-graphql-parser` inside `magna-serv`.

## Status

Session 1, round 0. Not yet started. Builder R1 to be dispatched.

## Crate spec (locked)

- **Name**: `magna-gqlmin`
- **Path**: `crates/magna-gqlmin/`
- **Stability**: `experimental`
- **Deps for default `ops + wasm`**: zero runtime deps. `dlmalloc` for the
  wasm allocator (chosen over `wee_alloc` for maintenance).
- **Crate-type**: `["rlib", "cdylib"]`
- **MSRV**: workspace pinned (`1.89.0`)

## Feature flag layout

```
default = ["ops", "std"]
ops          # always-on
sdl          # type-system definitions
validate     # requires sdl + std
pretty       # caret/line error rendering (std)
serde        # AST derives, used by napi
napi         # napi-rs binding
wasm         # pure no_std + extern "C" (NO wasm-bindgen)
wasm-bindgen # opt-in fatter wasm escape hatch
```

## Size budget (the constraint)

Target ≤5120 bytes gzipped. Architect estimate ~4450 bytes:

| Component | Bytes (gz) |
|---|---|
| Lexer | ~700 |
| Parser | ~1500 |
| AST | ~400 |
| Span helpers | ~150 |
| Error type | ~250 |
| Wasm glue | ~300 |
| dlmalloc | ~700 |
| Panic handler | ~50 |
| LLVM memcpy/memset | ~400 |
| **Subtotal** | **~4450** |

Build profile `release-wasm`: `opt-level = "z"`, `lto = "fat"`,
`codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`.

**Bans (enforced via review and clippy denies):** no `HashMap`, no
`format!`, no `regex`, no `serde` in runtime, no Unicode tables, no
`String` in AST, `#![no_std]` + `extern crate alloc` for wasm.

**Risk ladder (if 5 KB gz proves infeasible):**
1. Best: 4.5 KB with dlmalloc, full ops feature
2. Tight: switch to `wee_alloc`, prune AST `kind` enums
3. Over budget at 5.5–6.5 KB: drop block-string parsing
4. Hard ceiling 7 KB: ship at 7 KB, document in README; offer lexer-only
   build at <3 KB
5. Floor: lexer-only at <5 KB, parsing on the JS side

## AST shape (operations-only)

Span-based, zero-copy. `&'src str` for identifiers; numeric values stored
as unparsed lexemes (caller decodes). Public types: `Document`,
`Definition`, `OperationDefinition`, `FragmentDefinition`,
`OperationKind`, `Name`, `VariableDefinition`, `Directive`, `Argument`,
`SelectionSet`, `Selection` (Field|FragmentSpread|InlineFragment), `Field`,
`Type`, `NamedType`, `Value`, `StringValue`, `ObjectField`, `Span`.

## Public API

- `parse_executable_document(&str) -> Result<Document<'_>, ParseError>`
- `parse_schema(&str) -> Result<SchemaDocument<'_>, ParseError>` (sdl)
- `validate(doc, schema) -> Result<(), Vec<ValidationError>>` (validate)
- `ParseError { span: Span, kind: ParseErrorKind }` — `#[non_exhaustive]
  #[repr(u8)]` enum, static-string messages

**Wasm exports (no wasm-bindgen):** `gqlmin_alloc`, `gqlmin_free`,
`gqlmin_parse`, `gqlmin_result_free`. Custom binary AST encoding; JS-side
decoder is sibling `@magna/gqlmin-wasm` package, NOT in the wasm budget.

### Error code wire format

`ParseErrorKind` discriminants are partitioned by range so the JS-side
decoder (`@magna/gqlmin-wasm`) can branch without importing the Rust
enum:

- Lexer errors: `1..=5`
- Parser errors: `32..=43`

The gap is intentional. Rule for adding new kinds: append within the
correct range; do not fill the gap. JS decoders branch on `kind < 32`
to dispatch lex-vs-parse error rendering.

**Napi:** `parseExecutableDocument(src: string)` returning JSON via
`serde_json`. No size constraint.

## Parser strategy

Hand-written DFA lexer + LL(1) recursive-descent. Rejected lalrpop/pest
(runtime size) and nom/winnow (size variance under compiler updates).
Implements GraphQL Oct-2021 spec sections 2.1–2.12 (operations) and 3
(SDL, gated).

## Build pipeline

```bash
cargo build -p magna-gqlmin --target wasm32-unknown-unknown \
  --no-default-features --features "ops,wasm" --profile release-wasm
wasm-opt -Oz --strip-debug --vacuum <in> -o <out>
gzip -9 -c <out> | wc -c   # MUST be < 5120
```

CI gate `gqlmin-size`: fails the PR if gzipped wasm exceeds 5120 bytes.

## Test corpus (20 named cases)

`simple_query`, `named_query`, `query_with_variables`, `mutation`,
`subscription`, `fragment_definition`, `fragment_spread`,
`inline_fragment_with_type`, `inline_fragment_no_type`,
`nested_directives`, `field_alias`, `arguments_all_value_kinds`,
`non_null_list_type`, `default_value`, `multiple_operations`,
`block_string_arg`, `comments_and_commas`, `unicode_in_strings`,
`empty_selection_error`, `unterminated_string_error`.

Each test compares AST against a snapshot. Use a hand-written `Debug` impl
or a minimal serializer; do not add `insta` as a dependency.

## Build-order (12 steps; step 11 deferred)

1. Skeleton + workspace registration
2. Feature flag matrix + `check-features.sh`
3. Lexer
4. Ops parser + corpus
5. Wasm build + SIZE.md baseline
6. CI size gate
7. napi binding
8. SDL parser
9. Validation rules (10 starter)
10. Pretty errors + serde derives
11. **DEFERRED** — SFC compiler doesn't exist
12. Publish prep — README, CHANGELOG, GOVERNANCE row

## Locked decisions (autonomous mode)

- SFC compiler: out-of-scope this session
- Crate name: `magna-gqlmin`
- AST stability: free-to-break in 0.x minors
- Wasm target: generic `wasm32-unknown-unknown`, browser-tuned
- Validation: 10 starter rules
- Wasm AST: bumpalo-arena, `Document<'src, 'bump>` (decided post-R2)

## Structural size constraint (R2 finding)

Empirical measurement in R2 found the original 5,120-byte budget cannot be
met with `Vec<T>`-based AST collections on stable Rust. The 7 distinct
`Vec<T>` types in the parser (`Vec<Definition>`, `Vec<VariableDefinition>`,
`Vec<Directive>`, `Vec<Argument>`, `Vec<Selection>`, `Vec<ObjectField>`,
`Vec<Value>`) generate ~10 KB gz of monomorphized grow/drop/realloc code.
The Architect's 1.5 KB parser estimate implicitly assumed a non-Vec design.

Risk ladder exhausted in R2:

| Rung | Action | Measured gz |
|---|---|---|
| 0 baseline | dlmalloc, full ops parser | 15,783 |
| 1 | Gate Debug derives + Display behind cfg(std) | 15,375 |
| 2 | from_utf8_unchecked in wasm shim | (in rung 1) |
| 3 | Switch to wee_alloc | ~13,978 |
| 4 | Drop block-string parsing | not tried (API change) |
| 5 | Accept 7 KB ceiling | exceeded — N/A |

**Decision (locked by user, post-R2):** Option A — bumpalo arena. Replace
the 7 `Vec<T>` fields with `bumpalo::collections::Vec<'bump, T>`.
Document API gains a second lifetime: `Document<'src, 'bump>`. New optional
runtime dep: `bumpalo`. Estimated gz: 4,000–7,000 bytes (R3 will measure).

Wasm ABI confirmed working in R2 via Node smoke test:
- `simple_query.graphql` → tag=0 (success)
- `empty_selection_error.graphql` → tag=1, kind=34 (parse error)

This ABI is durable — future rounds must not break it.

## Round log

> Defect class: "structural fix — bumpalo arena migration" started after R2 surface. Iteration counter reset to 0/5 per playbook (work nature shifted from "build to spec" to "fix structural mismatch").

| Round | Steps | Status | Evidence |
|---|---|---|---|
| R1 | 1–4 (skeleton, features, lexer, ops parser) | ✅ DONE | 18 lexer tests + 20/20 corpus; 5/5 valid spec probes + 3/3 invalid rejections; all bans honored |
| R2 | 5–6 (wasm shim, SIZE.md, CI gate, smoke) | ⚠️ PARTIAL | wasm builds + smoke passes (tag=0/tag=1+kind=34); 38 R1 tests still pass; gz=15375 vs 5120 budget — surface to user; user chose Option A (bumpalo arena) |

## Open questions

(R3 measurement-pending: actual gz post-bumpalo migration; if still > 5,120 bytes, fall back to Option B span-indexed rewrite or accept revised target.)

## Latest director-note

(none yet — round 0)
