// SPDX-License-Identifier: MIT OR Apache-2.0
//! GraphQL lexer (Oct-2021 spec, section 2.1.6).
//!
//! Hand-written, allocation-free, ASCII-only for identifiers (per spec).
//! `&'static str` error messages, no `format!`, no `regex`, no Unicode tables.
//! Strings are validated for closure but their contents are NOT decoded here
//! — the lexer hands back the raw lexeme via the source slice + span; callers
//! that need decoded strings own that work.

use crate::error::{ParseError, ParseErrorKind};

/// Byte-offset span into the source. `u32` is enough for any sane source
/// (4 GiB) and keeps `Token` 16 bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
    pub const fn empty(at: u32) -> Self {
        Self {
            start: at,
            end: at,
        }
    }
    pub const fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// A lexed token: kind + byte-span into the source. Lexeme text is recovered
/// via `&source[span]` if the caller needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TokenKind {
    // Punctuators (spec 2.1.8).
    Bang,        // !
    Dollar,      // $
    Amp,         // &
    LParen,      // (
    RParen,      // )
    Spread,      // ...
    Colon,       // :
    Eq,          // =
    At,          // @
    LBracket,    // [
    RBracket,    // ]
    LBrace,      // {
    Pipe,        // |
    RBrace,      // }
    // Lexical tokens that carry a payload (via span).
    Name,
    IntValue,
    FloatValue,
    StringValue,
    BlockStringValue,
    Eof,
}

/// Streaming lexer. `next_token` advances; the lexer never lets the cursor
/// pass EOF — repeated calls after EOF keep returning `TokenKind::Eof`.
#[derive(Debug, Clone)]
pub struct Lexer<'src> {
    src: &'src str,
    bytes: &'src [u8],
    pos: usize,
    started: bool,
}

impl<'src> Lexer<'src> {
    pub fn new(src: &'src str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            started: false,
        }
    }

    pub fn source(&self) -> &'src str {
        self.src
    }

    /// Byte slice for a span. `&'src str` view.
    pub fn slice(&self, span: Span) -> &'src str {
        // span values originate from the lexer; they are always within bounds
        // and on UTF-8 boundaries (we only step by ASCII inside multi-byte
        // tokens).
        let s = span.start as usize;
        let e = span.end as usize;
        // SAFETY-equivalent without unsafe: defensive bound clamp.
        let s = if s > self.src.len() { self.src.len() } else { s };
        let e = if e > self.src.len() { self.src.len() } else { e };
        let s = if s > e { e } else { s };
        &self.src[s..e]
    }

    /// Advance and return the next token (or an error).
    pub fn next_token(&mut self) -> Result<Token, ParseError> {
        if !self.started {
            self.started = true;
            // Strip a leading UTF-8 BOM (EF BB BF) if present.
            if self.bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
                self.pos += 3;
            }
        }

        self.skip_insignificant();

        let start = self.pos;
        if start >= self.bytes.len() {
            let at = start as u32;
            return Ok(Token {
                kind: TokenKind::Eof,
                span: Span::new(at, at),
            });
        }

        let b = self.bytes[start];
        match b {
            b'!' => self.single(TokenKind::Bang),
            b'$' => self.single(TokenKind::Dollar),
            b'&' => self.single(TokenKind::Amp),
            b'(' => self.single(TokenKind::LParen),
            b')' => self.single(TokenKind::RParen),
            b':' => self.single(TokenKind::Colon),
            b'=' => self.single(TokenKind::Eq),
            b'@' => self.single(TokenKind::At),
            b'[' => self.single(TokenKind::LBracket),
            b']' => self.single(TokenKind::RBracket),
            b'{' => self.single(TokenKind::LBrace),
            b'|' => self.single(TokenKind::Pipe),
            b'}' => self.single(TokenKind::RBrace),
            b'.' => self.lex_spread(),
            b'_' | b'A'..=b'Z' | b'a'..=b'z' => self.lex_name(),
            b'-' | b'0'..=b'9' => self.lex_number(),
            b'"' => self.lex_string(),
            _ => Err(ParseError::new(
                Span::new(start as u32, (start + 1) as u32),
                ParseErrorKind::UnexpectedChar,
            )),
        }
    }

    // ---- Internals ----------------------------------------------------

    fn single(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        let start = self.pos as u32;
        self.pos += 1;
        Ok(Token {
            kind,
            span: Span::new(start, self.pos as u32),
        })
    }

    fn skip_insignificant(&mut self) {
        // Skip ASCII whitespace, line terminators, commas, comments. The spec
        // treats these as "insignificant" outside of string literals.
        loop {
            if self.pos >= self.bytes.len() {
                return;
            }
            let b = self.bytes[self.pos];
            match b {
                b' ' | b'\t' | b'\r' | b'\n' | b',' => {
                    self.pos += 1;
                }
                b'#' => {
                    // Comment to end-of-line.
                    self.pos += 1;
                    while self.pos < self.bytes.len() {
                        let c = self.bytes[self.pos];
                        if c == b'\n' || c == b'\r' {
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => return,
            }
        }
    }

    fn lex_spread(&mut self) -> Result<Token, ParseError> {
        let start = self.pos;
        if self.bytes.len() >= start + 3
            && self.bytes[start + 1] == b'.'
            && self.bytes[start + 2] == b'.'
        {
            self.pos = start + 3;
            return Ok(Token {
                kind: TokenKind::Spread,
                span: Span::new(start as u32, self.pos as u32),
            });
        }
        Err(ParseError::new(
            Span::new(start as u32, (start + 1) as u32),
            ParseErrorKind::UnexpectedChar,
        ))
    }

    fn lex_name(&mut self) -> Result<Token, ParseError> {
        let start = self.pos;
        // First byte already verified by caller match arm.
        self.pos += 1;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            let ok = matches!(b, b'_' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z');
            if !ok {
                break;
            }
            self.pos += 1;
        }
        Ok(Token {
            kind: TokenKind::Name,
            span: Span::new(start as u32, self.pos as u32),
        })
    }

    fn lex_number(&mut self) -> Result<Token, ParseError> {
        // Spec section 2.9: IntValue / FloatValue.
        // IntegerPart: -?(0 | NonZeroDigit Digit*)
        // FractionalPart: . Digit+
        // ExponentPart: (e|E) (+|-)? Digit+
        let start = self.pos;
        let mut is_float = false;

        if self.peek_byte() == Some(b'-') {
            self.pos += 1;
        }

        // Integer part: must have at least one digit.
        match self.peek_byte() {
            Some(b'0') => {
                self.pos += 1;
                // Per spec, no leading zeros: 0 may not be followed by another
                // digit. We enforce this conservatively.
                if let Some(c) = self.peek_byte() {
                    if c.is_ascii_digit() {
                        return Err(ParseError::new(
                            Span::new(start as u32, (self.pos + 1) as u32),
                            ParseErrorKind::InvalidNumber,
                        ));
                    }
                }
            }
            Some(c) if c.is_ascii_digit() => {
                while let Some(c) = self.peek_byte() {
                    if c.is_ascii_digit() {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
            }
            _ => {
                return Err(ParseError::new(
                    Span::new(start as u32, (self.pos + 1).min(self.bytes.len() + 1) as u32),
                    ParseErrorKind::InvalidNumber,
                ));
            }
        }

        // Fractional part.
        if self.peek_byte() == Some(b'.') {
            is_float = true;
            self.pos += 1;
            let frac_start = self.pos;
            while let Some(c) = self.peek_byte() {
                if c.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if self.pos == frac_start {
                return Err(ParseError::new(
                    Span::new(start as u32, self.pos as u32),
                    ParseErrorKind::InvalidNumber,
                ));
            }
        }

        // Exponent part.
        if matches!(self.peek_byte(), Some(b'e') | Some(b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek_byte(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            let exp_start = self.pos;
            while let Some(c) = self.peek_byte() {
                if c.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if self.pos == exp_start {
                return Err(ParseError::new(
                    Span::new(start as u32, self.pos as u32),
                    ParseErrorKind::InvalidNumber,
                ));
            }
        }

        // Per spec: numbers can't be immediately followed by NameStart or `.`
        // (otherwise `1.foo` would lex weirdly). Reject that here.
        if let Some(c) = self.peek_byte() {
            if matches!(c, b'_' | b'A'..=b'Z' | b'a'..=b'z' | b'.') {
                return Err(ParseError::new(
                    Span::new(start as u32, (self.pos + 1) as u32),
                    ParseErrorKind::InvalidNumber,
                ));
            }
        }

        Ok(Token {
            kind: if is_float {
                TokenKind::FloatValue
            } else {
                TokenKind::IntValue
            },
            span: Span::new(start as u32, self.pos as u32),
        })
    }

    fn lex_string(&mut self) -> Result<Token, ParseError> {
        let start = self.pos;
        // Block string: """ ... """
        if self.bytes.len() >= start + 3
            && self.bytes[start + 1] == b'"'
            && self.bytes[start + 2] == b'"'
        {
            self.pos = start + 3;
            loop {
                if self.pos >= self.bytes.len() {
                    return Err(ParseError::new(
                        Span::new(start as u32, self.pos as u32),
                        ParseErrorKind::InvalidBlockString,
                    ));
                }
                let b = self.bytes[self.pos];
                if b == b'\\'
                    && self.bytes.len() >= self.pos + 4
                    && self.bytes[self.pos + 1] == b'"'
                    && self.bytes[self.pos + 2] == b'"'
                    && self.bytes[self.pos + 3] == b'"'
                {
                    // \""" escape — consume all four bytes.
                    self.pos += 4;
                    continue;
                }
                if b == b'"'
                    && self.bytes.len() >= self.pos + 3
                    && self.bytes[self.pos + 1] == b'"'
                    && self.bytes[self.pos + 2] == b'"'
                {
                    self.pos += 3;
                    return Ok(Token {
                        kind: TokenKind::BlockStringValue,
                        span: Span::new(start as u32, self.pos as u32),
                    });
                }
                self.pos += 1;
            }
        }

        // Regular string.
        self.pos += 1; // skip opening "
        loop {
            if self.pos >= self.bytes.len() {
                return Err(ParseError::new(
                    Span::new(start as u32, self.pos as u32),
                    ParseErrorKind::InvalidString,
                ));
            }
            let b = self.bytes[self.pos];
            match b {
                b'"' => {
                    self.pos += 1;
                    return Ok(Token {
                        kind: TokenKind::StringValue,
                        span: Span::new(start as u32, self.pos as u32),
                    });
                }
                b'\n' | b'\r' => {
                    // Line terminators forbidden in regular strings.
                    return Err(ParseError::new(
                        Span::new(start as u32, self.pos as u32),
                        ParseErrorKind::InvalidString,
                    ));
                }
                b'\\' => {
                    // Validate escape: \" \\ \/ \b \f \n \r \t \uXXXX
                    if self.pos + 1 >= self.bytes.len() {
                        return Err(ParseError::new(
                            Span::new(start as u32, self.pos as u32),
                            ParseErrorKind::InvalidEscape,
                        ));
                    }
                    let esc = self.bytes[self.pos + 1];
                    match esc {
                        b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => {
                            self.pos += 2;
                        }
                        b'u' => {
                            if self.pos + 6 > self.bytes.len() {
                                return Err(ParseError::new(
                                    Span::new(start as u32, self.pos as u32),
                                    ParseErrorKind::InvalidEscape,
                                ));
                            }
                            for i in 2..6 {
                                if !self.bytes[self.pos + i].is_ascii_hexdigit() {
                                    return Err(ParseError::new(
                                        Span::new(start as u32, (self.pos + i) as u32),
                                        ParseErrorKind::InvalidEscape,
                                    ));
                                }
                            }
                            self.pos += 6;
                        }
                        _ => {
                            return Err(ParseError::new(
                                Span::new(start as u32, (self.pos + 2) as u32),
                                ParseErrorKind::InvalidEscape,
                            ));
                        }
                    }
                }
                _ => {
                    self.pos += 1;
                }
            }
        }
    }

    fn peek_byte(&self) -> Option<u8> {
        if self.pos < self.bytes.len() {
            Some(self.bytes[self.pos])
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_all(src: &str) -> Result<alloc::vec::Vec<Token>, ParseError> {
        let mut lex = Lexer::new(src);
        let mut out = alloc::vec::Vec::new();
        loop {
            let t = lex.next_token()?;
            let done = t.kind == TokenKind::Eof;
            out.push(t);
            if done {
                break;
            }
        }
        Ok(out)
    }

    #[test]
    fn empty_input_yields_eof() {
        let toks = lex_all("").unwrap();
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].kind, TokenKind::Eof);
        assert_eq!(toks[0].span, Span::new(0, 0));
    }

    #[test]
    fn punctuators_each_kind() {
        let toks = lex_all("! $ & ( ) ... : = @ [ ] { | }").unwrap();
        let kinds: alloc::vec::Vec<_> = toks.iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            alloc::vec![
                TokenKind::Bang,
                TokenKind::Dollar,
                TokenKind::Amp,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Spread,
                TokenKind::Colon,
                TokenKind::Eq,
                TokenKind::At,
                TokenKind::LBracket,
                TokenKind::RBracket,
                TokenKind::LBrace,
                TokenKind::Pipe,
                TokenKind::RBrace,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn name_token_basic() {
        let toks = lex_all("hello _under World123").unwrap();
        assert_eq!(toks.len(), 4); // 3 names + EOF
        for i in 0..3 {
            assert_eq!(toks[i].kind, TokenKind::Name);
        }
    }

    #[test]
    fn integer_literal() {
        let toks = lex_all("42 0 -7").unwrap();
        let kinds: alloc::vec::Vec<_> = toks.iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            alloc::vec![
                TokenKind::IntValue,
                TokenKind::IntValue,
                TokenKind::IntValue,
                TokenKind::Eof
            ]
        );
        let lex = Lexer::new("42 0 -7");
        assert_eq!(lex.slice(toks[2].span), "-7");
    }

    #[test]
    fn float_dot_and_exponent() {
        let toks = lex_all("1.5 1.5e10 1e-3 2E+5").unwrap();
        let kinds: alloc::vec::Vec<_> = toks.iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            alloc::vec![
                TokenKind::FloatValue,
                TokenKind::FloatValue,
                TokenKind::FloatValue,
                TokenKind::FloatValue,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn string_literal_and_escape() {
        let toks = lex_all(r#" "hello" "with\nescape" "ué" "#).unwrap();
        assert_eq!(toks[0].kind, TokenKind::StringValue);
        assert_eq!(toks[1].kind, TokenKind::StringValue);
        assert_eq!(toks[2].kind, TokenKind::StringValue);
    }

    #[test]
    fn unterminated_string_errors() {
        let mut lex = Lexer::new(r#"  "oops"#);
        let err = lex.next_token().expect_err("must error");
        assert_eq!(err.kind, ParseErrorKind::InvalidString);
        assert!(err.span.start <= err.span.end);
        assert_eq!(err.span.start, 2);
    }

    #[test]
    fn block_string_with_escape() {
        let src = r#""""hello \""" world""""#;
        let mut lex = Lexer::new(src);
        let t = lex.next_token().unwrap();
        assert_eq!(t.kind, TokenKind::BlockStringValue);
        // The outer triple-quotes plus payload — span covers entire literal.
        assert_eq!(t.span.start, 0);
        assert_eq!(t.span.end as usize, src.len());
        let next = lex.next_token().unwrap();
        assert_eq!(next.kind, TokenKind::Eof);
    }

    #[test]
    fn unterminated_block_string_errors() {
        let mut lex = Lexer::new(r#""""no end"#);
        let err = lex.next_token().expect_err("must error");
        assert_eq!(err.kind, ParseErrorKind::InvalidBlockString);
    }

    #[test]
    fn comment_skipped() {
        let toks = lex_all("# a comment\nfoo").unwrap();
        assert_eq!(toks.len(), 2);
        assert_eq!(toks[0].kind, TokenKind::Name);
        assert_eq!(toks[1].kind, TokenKind::Eof);
    }

    #[test]
    fn comma_is_insignificant() {
        let toks = lex_all("a,b").unwrap();
        let kinds: alloc::vec::Vec<_> = toks.iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            alloc::vec![TokenKind::Name, TokenKind::Name, TokenKind::Eof]
        );
    }

    #[test]
    fn bom_skipped_at_start() {
        let mut s = alloc::string::String::new();
        s.push('\u{FEFF}');
        s.push_str("foo");
        let toks = lex_all(&s).unwrap();
        assert_eq!(toks[0].kind, TokenKind::Name);
        assert_eq!(toks[0].span.start, 3); // BOM is 3 bytes in UTF-8
    }

    #[test]
    fn negative_int_lex() {
        let mut lex = Lexer::new("-42");
        let t = lex.next_token().unwrap();
        assert_eq!(t.kind, TokenKind::IntValue);
        assert_eq!(lex.slice(t.span), "-42");
    }

    #[test]
    fn float_with_dot_and_exp() {
        let mut lex = Lexer::new("1.5e10");
        let t = lex.next_token().unwrap();
        assert_eq!(t.kind, TokenKind::FloatValue);
        assert_eq!(lex.slice(t.span), "1.5e10");
    }

    #[test]
    fn spread_token() {
        let toks = lex_all("...").unwrap();
        assert_eq!(toks[0].kind, TokenKind::Spread);
    }

    #[test]
    fn unknown_char_errors() {
        let mut lex = Lexer::new("?");
        let err = lex.next_token().expect_err("must error");
        assert_eq!(err.kind, ParseErrorKind::UnexpectedChar);
    }

    #[test]
    fn span_slice_recovers_lexeme() {
        let lex = Lexer::new("abc");
        let mut l2 = Lexer::new("abc");
        let t = l2.next_token().unwrap();
        assert_eq!(lex.slice(t.span), "abc");
    }

    #[test]
    fn invalid_number_leading_zero() {
        let mut lex = Lexer::new("01");
        let err = lex.next_token().expect_err("must error");
        assert_eq!(err.kind, ParseErrorKind::InvalidNumber);
    }
}
