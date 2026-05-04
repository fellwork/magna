# Investigation R9 — `#[inline(always)]` regression on parser helpers

Round: R9 phase 3 (sub-investigation). Triggered by phase-3 measurement
showing the full 11-helper `#[inline(always)]` set produced
**gz=6,847** (Δ=+593 vs R8 6,254) — Iron Law fires.

This note documents the bisection, reaches a conclusion, and explains
why the round still landed PARTIAL with gz=6,155.

## Sub-experiments (single-helper bisection)

Starting from "all 11 helpers `#[inline(always)]`" → gz=6,847, the
brief's investigation guidance ("over-inlining usually hurts; back off
and re-measure") was applied. I ran a bisection where the conservative
baseline keeps only one-line helpers (`Span::new`, `Lexer::peek_byte`)
with `#[inline(always)]`, then added one mid-size helper at a time.

| Variant | Set | gz | Δ vs R8 | Δ vs conservative |
|---|---|---|---|---|
| R8 | (baseline, no R9 changes) | 6,254 | 0 | +99 |
| R9-aggressive | all 11 helpers `inline(always)` | 6,847 | +593 | +692 |
| R9b-conservative | `Span::new` + `Lexer::peek_byte` only + `Document::slice` + `open_list` + `close_list` (mostly former `#[inline]` promotions) | 6,155 | −99 | 0 |
| R9c-minimal | `Span::new` + `Lexer::peek_byte` only (others reverted to `#[inline]`) | 6,155 | −99 | 0 |
| R9d (+ `Parser::peek`) | minimal + `peek` | 6,588 | +334 | +433 |
| R9e (+ `Parser::slice`) | minimal + `slice` | 6,223 | −31 | +68 |
| R9f (+ `Parser::bump_tok`) | minimal + `bump_tok` | 6,359 | +105 | +204 |

**Interpretation.** Forcing `inline(always)` on the larger hot helpers
(`peek`, `bump_tok`, `slice`) at every call site grows code, not shrinks
it, on this workload + this compiler. The only positive contributors
are the true one-liners (`Span::new`, `Lexer::peek_byte`), and even
those are essentially indistinguishable between `#[inline]` and
`#[inline(always)]` on the conservative variant.

## Why over-inlining hurt

The brief's hypothesis ("LLVM may currently be sharing a tail of
`expect`/`peek` across multiple sites via outlining; `inline(always)`
disables that") is consistent with the data. In particular:

- **`Parser::peek` has a 5-line body that returns `Result<Token, _>`.**
  Inlining it at ~12 call sites duplicates the `if let Some(t) =
  self.peeked` test + the `self.lexer.next_token()?` dispatch. The
  `?` propagation is itself non-trivial code (carries an enum
  discriminant + early-return scaffold). Each duplication adds ~30–50
  bytes raw → +400–600 bytes compressed across all duplications.
- **`Parser::bump_tok` has the same shape** (5-line body, hot, multi-
  call) and a similar regression signature. The `Option::take()` +
  early-return + `next_token()` dispatch is already small enough that
  LLVM was making the right call (out-of-line) and our directive
  forced the wrong one.
- **`Parser::slice` is a 4-line body** but its 5 call sites all share
  the exact same fallback (`unwrap_or("")`). Inlining duplicates the
  fallback constant address logic at each site. Net regression but
  smaller (+68 bytes).
- **`Document::slice`, `Parser::open_list`, `Parser::close_list`** —
  all promoted from `#[inline]` to `#[inline(always)]` were neutral
  (no measurable change between R9b and R9c). LLVM was already
  inlining these via `#[inline]` because the bodies are tiny.

## What stayed

Final R9 inline-always set (kept):

- `Span::new` (1-line constructor; many callers including hot paths in
  `lex.rs` and `parse/mod.rs`).
- `Lexer::peek_byte` (1-line `self.bytes.get(self.pos).copied()`).

Both are pure one-liners. The `#[inline(always)]` directive on these
acts as insurance against future caller growth without measurable cost
today.

Two `#[inline]` annotations remain on slightly larger helpers
(`Document::slice`, `open_list`, `close_list`) — these were already
present pre-R9 and unchanged post-R9.

## Durable lesson

`#[inline(always)]` is NOT free for size-on-wasm even on small (5-line)
hot helpers. The R8 binary's 33-function structure was already a
near-Pareto-frontier: LLVM had picked which helpers to inline and
which to share via outlining, and forcing the inline direction at the
medium helpers (`peek`, `bump_tok`, `slice`, `expect`) breaks the
outlining without giving enough specialisation/CSE wins to compensate.

The win, when it exists, is at the one-liner end of the spectrum. For
the larger hot helpers, **trust LLVM**. The `#[inline]` hint (without
`always`) is the right level of guidance.

## Counter / Iron-Law disposition

The phase-3 aggressive variant (gz=6,847) tripped the Iron Law literally
— it was a regression vs R8 (6,254). Per the R9 brief's investigation
guidance, the response is "back off and re-measure," NOT to surface the
aggressive variant. Backing off to the conservative variant produced
gz=6,155 — a 99-byte improvement and explicitly NOT a regression.

The final committed R9 result is the conservative variant. Iron Law
does NOT fire on the committed state.

R9 verdict: **PARTIAL — underwhelming** (close-miss but only by 99
bytes). gz=6,155 lands in the brief's "6,051 ≤ gz < 6,254" band:
"PARTIAL but underwhelming — inlining didn't yield as expected."
Director R9 should surface for ship-or-iterate decision.
