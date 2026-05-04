# R9 Inline / Merge Candidate Audit (9a-inline)

Round: R9 phase 1 audit, scope `9a-inline` per Director R8 §5(c).

Goal: identify small parser helpers in `src/parse/mod.rs` and `src/lex.rs`
where adding `#[inline(always)]` or merging into the single call site
gives LLVM enough information to deduplicate / CSE / specialize and shrink
the wasm `.text` section. **No API change.** Block-string parsing
preserved.

Counts are approximate (function bodies measured by line count, not
tokens). Call-site counts are static call-count from the source —
projectors used as `fn`-pointers are noted.

## Conventions

- "inline-always" = add `#[inline(always)]`. Prefer for small (≤10 lines)
  hot helpers called from 2+ sites.
- "merge" = paste the body into the single call site, delete the helper.
  Prefer when there's exactly one call site and the merge is mechanical.
- "leave-alone" = the function is large, public-API, used as a function
  pointer, or already correctly attributed.

## src/parse/mod.rs

| Symbol | Lines | Call sites | Existing attr | Recommendation | Notes |
|---|---|---|---|---|---|
| `NodeRange::is_empty` | 1 | external | `#[inline]` | leave | tiny, public API |
| `NodeRange::len` | 1 | external | `#[inline]` | leave | tiny, public API |
| `NodeSlice::len` | 1 | external | `#[inline]` | leave | trait-impl-like |
| `NodeSlice::is_empty` | 1 | external | `#[inline]` | leave | trait-impl-like |
| `NodeSlice::get` | 1 | external | `#[inline]` | leave | trait-impl-like |
| `NodeSlice::iter` | 4 | external | `#[inline]` | leave | trait-impl-like |
| `NodeSliceIter::next/size_hint/len` | 1 each | external | `#[inline]` | leave | iterator impl |
| `project_definition` (+6 siblings) | 5 each | 1 each (as fn-ptr) | `#[inline]` | **leave-alone** | Used as `for<'a> fn(...)` argument to `Document::slice`. Inlining at the consumer site is impossible while they're addressed-as-data fn pointers. Forcing `#[inline(always)]` is at best a no-op, at worst risks the compiler taking different addresses. |
| `panic_invariant` | 3 | 8 | `#[cold] #[inline(never)]` | **leave-alone** | Intentionally not inlined: it's the central abort point and inlining its body would re-emit the abort instruction at every call. Single-emission is the size-optimal posture. |
| `Document::definitions` / 6 siblings | 3 each | external API | `#[inline]` | leave | public API; already `#[inline]`. |
| `Document::slice` | ~7 | 7 (from accessors) | `#[inline]` | **inline-always** | Called from 7 accessors; each accessor is itself only `#[inline]`. Promoting `slice` to `inline(always)` lets LLVM observe the constant `project` fn-pointer at each accessor site and (potentially) devirtualise. |
| `parse_executable_document` | 2 | external entry | none | leave | public API; the body is two statements but the function is the public entry point. Tiny; inlining-into-caller is a caller decision. |
| `Parser::new` | 7 | 1 (`parse_executable_document`) | none | **inline-always** | Called from one site. Adding `#[inline(always)]` is equivalent to merging without textual deletion (preserves readability). |
| `Parser::peek` | 7 | ~12 inside parser | none | **inline-always** | Hot path, small, multi-call. Highest-value target. |
| `Parser::bump_tok` | 5 | ~14 inside parser | none | **inline-always** | Hot path, very small, multi-call. Highest-value target. |
| `Parser::slice` | 4 | 5 inside parser | none | **inline-always** | Small helper, multi-call. The `unwrap_or("")` body is identical at every site → CSE opportunity. |
| `Parser::expect` | 6 | 7 inside parser | none | **inline-always** | Hot path, multi-call, body is a peek+kind-compare+bump_tok or err. Inlining lets LLVM specialise per `(kind, err)` pair. |
| `Parser::open_list` | 1 | 7 inside parser | `#[inline]` | **inline-always** | Trivial body (`self.scratch.len()`). Promotion is essentially free. |
| `Parser::close_list` | 7 | 7 inside parser | `#[inline]` | **inline-always** | Already attributed `#[inline]`. Promote to `inline(always)`; small and multi-call. |
| `Parser::parse_document` / `parse_definition` / ... | 10–80 | recursive | none | leave | Production functions; large; recursion exists; inlining recursive functions can blow up size. |

## src/lex.rs

| Symbol | Lines | Call sites | Existing attr | Recommendation | Notes |
|---|---|---|---|---|---|
| `Span::new` | 1 | many (parser + lex) | `pub const` | **inline-always** | One-liner constructor. Already `const`, but no `#[inline]` attribute — adding `#[inline(always)]` ensures it never appears as an out-of-line function. |
| `Span::empty` | 1 | rare | `pub const` | leave | not hot |
| `Span::len` | 1 | rare | `pub const` | leave | not hot |
| `Span::is_empty` | 1 | rare | `pub const` | leave | not hot |
| `Lexer::new` | 6 | 1 (`Parser::new`) | none | leave | already on the path that's getting `inline(always)` via Parser::new; further attribute redundant. |
| `Lexer::source` | 1 | 0 in this crate | none | leave | public API surface |
| `Lexer::slice` | 4 | 0 internal | none | leave | public API surface; parser uses its own `slice`. |
| `Lexer::next_token` | 47 | hot | none | leave | the dispatch loop; it's already at the right size for LLVM to leave out-of-line. |
| `Lexer::single` | 7 | 13 inside `next_token` | none | **inline-always** | Tiny (advance pos by 1 + return Ok(Token{...})), called from 13 punctuator arms. Inlining lets LLVM see the kind constant at each site. Highest-value target in lex.rs. |
| `Lexer::skip_insignificant` | 25 | 1 (`next_token`) | none | leave | One call site; the body has a loop with two non-trivial inner cases (whitespace + comment). Inlining-into-caller may not help; leaving as a clean separate function avoids accidental code duplication. |
| `Lexer::lex_spread` | 14 | 1 (`next_token`) | none | **inline-always** | Single call site, small, simple branching → inline-always merges it without textual deletion. |
| `Lexer::lex_name` | 14 | 1 (`next_token`) | none | **inline-always** | Single call site, small, hot. |
| `Lexer::lex_number` | 100 | 1 | none | leave | Large; do NOT inline. (Forcing inline-always could grow `next_token` past a code-size threshold and hurt overall.) |
| `Lexer::lex_string` | 115 | 1 | none | leave | Large; do NOT inline. Block-string handling is in here — must stay functional. |
| `Lexer::peek_byte` | 1 | ~12 inside lex_number | none | **inline-always** | One-liner (`self.bytes.get(self.pos).copied()`). Already trivially inlined by LLVM, but explicit `#[inline(always)]` documents intent and locks behavior. |

## Summary counts

- **inline-always candidates: 11** (parse: 7 — `Document::slice`, `Parser::new`, `Parser::peek`, `Parser::bump_tok`, `Parser::slice`, `Parser::expect`, `Parser::open_list`/`close_list` (promotion); lex: 4 — `Span::new`, `Lexer::single`, `Lexer::lex_spread`, `Lexer::lex_name`, `Lexer::peek_byte`).
- **merge candidates: 0** (single-call-site helpers are kept as `#[inline(always)]` for readability per brief guidance).
- **leave-alone: rest** (large, public API, fn-pointer-addressed, or already correctly attributed).

## Risk notes

- **`#[inline(always)]` on an already-inlined function is a no-op.** Compiler may already be inlining the small ones; the directive is partially insurance against regressions when the caller grows.
- **Over-inlining can hurt.** If LLVM is currently sharing a tail of `expect` / `peek` across multiple sites via outlining, `inline(always)` could disable that. The R9 brief flags this: gz regression vs R8 → BLOCKED. We will measure.
- **`lex_number` and `lex_string` are deliberately NOT inlined.** Their size is the reason — forcing them into `next_token` would balloon a single function that LLVM may already be keeping tight via tail-merging across panic exits.
- **Block-string parsing preserved.** No code path under `BlockStringValue` / `lex_string` block branch is touched. The corpus test `block_string_arg.graphql` remains in scope and must pass post-R9.
