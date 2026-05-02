// SPDX-License-Identifier: MIT OR Apache-2.0
//! Parse error types.
//!
//! Error kinds are static, `repr(u8)` discriminants with const `&'static str`
//! messages. No `format!`, no owned `String`, no allocator usage on the error
//! path — this matters for the wasm size budget (R2+).

use crate::lex::Span;

/// A parse-time error. Carries a span (so callers can render carets without
/// re-lexing) and a static kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseError {
    pub span: Span,
    pub kind: ParseErrorKind,
}

impl ParseError {
    pub const fn new(span: Span, kind: ParseErrorKind) -> Self {
        Self { span, kind }
    }

    /// Static, allocation-free message for the error kind.
    pub const fn message(&self) -> &'static str {
        self.kind.message()
    }
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Avoid `format!`; write the parts directly.
        f.write_str(self.kind.message())?;
        f.write_str(" at byte ")?;
        // u32 -> ASCII without going through std formatting machinery on
        // wasm builds is overkill for round 1; the default Display for u32
        // does not allocate.
        core::fmt::Display::fmt(&self.span.start, f)?;
        f.write_str("..")?;
        core::fmt::Display::fmt(&self.span.end, f)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ParseError {}

/// Discriminants are stable within a 0.x minor; new variants may be added
/// (`#[non_exhaustive]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[repr(u8)]
pub enum ParseErrorKind {
    /// Lexer encountered a byte it cannot start a token with.
    UnexpectedChar = 1,
    /// String literal not closed before EOF or newline.
    InvalidString = 2,
    /// Block string not closed before EOF.
    InvalidBlockString = 3,
    /// Numeric literal malformed (e.g. trailing `.`, missing exponent digits).
    InvalidNumber = 4,
    /// Unknown escape sequence inside a string literal.
    InvalidEscape = 5,

    /// Parser hit EOF before completing a production.
    UnexpectedEof = 32,
    /// Parser saw a token kind it did not expect.
    UnexpectedToken = 33,
    /// Selection set must contain at least one selection.
    EmptySelectionSet = 34,
    /// Operation type keyword (`query`/`mutation`/`subscription`) expected.
    ExpectedOperationKind = 35,
    /// A name (identifier) was required here.
    ExpectedName = 36,
    /// `:` expected (e.g., between argument name and value, or alias and field).
    ExpectedColon = 37,
    /// A type reference was required here.
    ExpectedType = 38,
    /// A value was required here.
    ExpectedValue = 39,
    /// `$` expected (variable reference).
    ExpectedDollar = 40,
    /// `on` keyword expected (in fragment definition or typed inline fragment).
    ExpectedOnKeyword = 41,
    /// Closing punctuator missing (`)`, `]`, `}`).
    UnclosedDelimiter = 42,
    /// Top-level definition not recognized.
    UnknownDefinition = 43,
}

impl ParseErrorKind {
    pub const fn message(&self) -> &'static str {
        match self {
            ParseErrorKind::UnexpectedChar => "unexpected character",
            ParseErrorKind::InvalidString => "invalid or unterminated string literal",
            ParseErrorKind::InvalidBlockString => "invalid or unterminated block string",
            ParseErrorKind::InvalidNumber => "invalid numeric literal",
            ParseErrorKind::InvalidEscape => "invalid escape sequence in string",
            ParseErrorKind::UnexpectedEof => "unexpected end of input",
            ParseErrorKind::UnexpectedToken => "unexpected token",
            ParseErrorKind::EmptySelectionSet => "selection set must not be empty",
            ParseErrorKind::ExpectedOperationKind => {
                "expected `query`, `mutation`, or `subscription`"
            }
            ParseErrorKind::ExpectedName => "expected a name",
            ParseErrorKind::ExpectedColon => "expected `:`",
            ParseErrorKind::ExpectedType => "expected a type reference",
            ParseErrorKind::ExpectedValue => "expected a value",
            ParseErrorKind::ExpectedDollar => "expected `$`",
            ParseErrorKind::ExpectedOnKeyword => "expected `on`",
            ParseErrorKind::UnclosedDelimiter => "unclosed delimiter",
            ParseErrorKind::UnknownDefinition => "unknown top-level definition",
        }
    }
}
