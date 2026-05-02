// SPDX-License-Identifier: MIT OR Apache-2.0
//! GraphQL lexer (Oct-2021 spec, section 2.1.6).
//!
//! Round 1 step 1: stubs only. The DFA implementation lands in step 3.

use crate::error::{ParseError, ParseErrorKind};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TokenKind {
    Bang,
    Dollar,
    Amp,
    LParen,
    RParen,
    Spread,
    Colon,
    Eq,
    At,
    LBracket,
    RBracket,
    LBrace,
    Pipe,
    RBrace,
    Name,
    IntValue,
    FloatValue,
    StringValue,
    BlockStringValue,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Lexer<'src> {
    src: &'src str,
}

impl<'src> Lexer<'src> {
    pub fn new(src: &'src str) -> Self {
        Self { src }
    }

    pub fn source(&self) -> &'src str {
        self.src
    }

    pub fn next_token(&mut self) -> Result<Token, ParseError> {
        Err(ParseError::new(
            Span::new(0, 0),
            ParseErrorKind::UnexpectedEof,
        ))
    }
}
