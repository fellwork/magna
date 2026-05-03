// SPDX-License-Identifier: MIT OR Apache-2.0
//! Operations parser (GraphQL Oct-2021 spec, sections 2.2–2.12).
//!
//! Hand-rolled LL(1) recursive descent over the lexer. One token of
//! lookahead via `Parser { peeked: Option<Token> }`. No backtracking.
//!
//! ### Arena-allocated AST (R3)
//!
//! All list collections in the AST are stored as
//! [`bumpalo::collections::Vec<'bump, T>`]. The seven distinct
//! `alloc::vec::Vec<T>` types from R1/R2 (`Definition`,
//! `VariableDefinition`, `Directive`, `Argument`, `Selection`,
//! `ObjectField`, `Value::List`) collapsed into one monomorphization
//! against `bumpalo::Bump`. See `docs/investigation-r2-wasm-size.md`.
//!
//! The AST therefore carries TWO lifetimes:
//!
//! * `'src` — the borrow over the input GraphQL text. Identifiers,
//!   numeric lexemes, and string raw-text all reference the input.
//! * `'bump` — the borrow over the caller-owned `bumpalo::Bump`.
//!   Every list field lives in this arena. Dropping the arena drops
//!   the entire AST in O(1).
//!
//! Callers construct an arena and pass it to
//! [`parse_executable_document`]. The arena is owned by the caller;
//! the parser does not allocate new arenas.

use bumpalo::collections::Vec as BVec;
use bumpalo::Bump;

use crate::error::{ParseError, ParseErrorKind};
use crate::lex::{Lexer, Span, Token, TokenKind};

// --- AST ----------------------------------------------------------------

/// Top-level executable document: a non-empty list of definitions.
#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct Document<'src, 'bump> {
    pub definitions: BVec<'bump, Definition<'src, 'bump>>,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub enum Definition<'src, 'bump> {
    Operation(OperationDefinition<'src, 'bump>),
    Fragment(FragmentDefinition<'src, 'bump>),
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    Query,
    Mutation,
    Subscription,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct OperationDefinition<'src, 'bump> {
    pub kind: OperationKind,
    pub name: Option<Name<'src>>,
    pub variable_definitions: BVec<'bump, VariableDefinition<'src, 'bump>>,
    pub directives: BVec<'bump, Directive<'src, 'bump>>,
    pub selection_set: SelectionSet<'src, 'bump>,
    pub span: Span,
    /// True for `{ ... }` shorthand queries (no `query` keyword, no name).
    pub shorthand: bool,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct FragmentDefinition<'src, 'bump> {
    pub name: Name<'src>,
    pub type_condition: NamedType<'src>,
    pub directives: BVec<'bump, Directive<'src, 'bump>>,
    pub selection_set: SelectionSet<'src, 'bump>,
    pub span: Span,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Name<'src> {
    pub value: &'src str,
    pub span: Span,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NamedType<'src> {
    pub name: Name<'src>,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct VariableDefinition<'src, 'bump> {
    pub name: Name<'src>,
    pub var_type: Type<'src, 'bump>,
    pub default_value: Option<Value<'src, 'bump>>,
    pub directives: BVec<'bump, Directive<'src, 'bump>>,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct Directive<'src, 'bump> {
    pub name: Name<'src>,
    pub arguments: BVec<'bump, Argument<'src, 'bump>>,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct Argument<'src, 'bump> {
    pub name: Name<'src>,
    pub value: Value<'src, 'bump>,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct SelectionSet<'src, 'bump> {
    pub selections: BVec<'bump, Selection<'src, 'bump>>,
    pub span: Span,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub enum Selection<'src, 'bump> {
    Field(Field<'src, 'bump>),
    FragmentSpread(FragmentSpread<'src, 'bump>),
    InlineFragment(InlineFragment<'src, 'bump>),
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct Field<'src, 'bump> {
    pub alias: Option<Name<'src>>,
    pub name: Name<'src>,
    pub arguments: BVec<'bump, Argument<'src, 'bump>>,
    pub directives: BVec<'bump, Directive<'src, 'bump>>,
    pub selection_set: Option<SelectionSet<'src, 'bump>>,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct FragmentSpread<'src, 'bump> {
    pub name: Name<'src>,
    pub directives: BVec<'bump, Directive<'src, 'bump>>,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct InlineFragment<'src, 'bump> {
    pub type_condition: Option<NamedType<'src>>,
    pub directives: BVec<'bump, Directive<'src, 'bump>>,
    pub selection_set: SelectionSet<'src, 'bump>,
}

/// `Type` references arena-allocated `Type` nodes for `List`/`NonNull`
/// wrappers. The previous `alloc::boxed::Box<Type>` representation was
/// kept zero-copy by replacing `Box` with `&'bump Type<'src, 'bump>` —
/// the inner type lives in the same `Bump` arena as the rest of the AST.
#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, Copy, PartialEq)]
pub enum Type<'src, 'bump> {
    Named(NamedType<'src>),
    List(&'bump Type<'src, 'bump>),
    NonNull(&'bump Type<'src, 'bump>),
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub enum Value<'src, 'bump> {
    Variable(Name<'src>),
    /// Unparsed integer lexeme (e.g. `"-42"`). Caller decodes.
    Int(&'src str),
    /// Unparsed float lexeme. Caller decodes.
    Float(&'src str),
    /// String literal — `raw` is the raw source slice including the quotes
    /// (regular or block). Caller decodes / unescapes if needed.
    String(StringValue<'src>),
    Boolean(bool),
    Null,
    Enum(Name<'src>),
    List(BVec<'bump, Value<'src, 'bump>>),
    Object(BVec<'bump, ObjectField<'src, 'bump>>),
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct StringValue<'src> {
    pub raw: &'src str,
    pub block: bool,
    pub span: Span,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct ObjectField<'src, 'bump> {
    pub name: Name<'src>,
    pub value: Value<'src, 'bump>,
}

// --- Public entry point -------------------------------------------------

/// Parse an executable document into a bumpalo-arena-backed AST.
///
/// The caller owns the arena. The returned `Document` borrows from
/// both `arena` (for list collections + boxed `Type` nodes) and `src`
/// (for identifiers and unparsed lexemes). Dropping the arena drops
/// the AST in O(1).
pub fn parse_executable_document<'src, 'bump>(
    arena: &'bump Bump,
    src: &'src str,
) -> Result<Document<'src, 'bump>, ParseError> {
    let mut p = Parser::new(arena, src);
    let doc = p.parse_document()?;
    Ok(doc)
}

// --- Parser -------------------------------------------------------------

struct Parser<'src, 'bump> {
    arena: &'bump Bump,
    src: &'src str,
    lexer: Lexer<'src>,
    peeked: Option<Token>,
}

impl<'src, 'bump> Parser<'src, 'bump> {
    fn new(arena: &'bump Bump, src: &'src str) -> Self {
        Self {
            arena,
            src,
            lexer: Lexer::new(src),
            peeked: None,
        }
    }

    fn peek(&mut self) -> Result<Token, ParseError> {
        if let Some(t) = self.peeked {
            return Ok(t);
        }
        let t = self.lexer.next_token()?;
        self.peeked = Some(t);
        Ok(t)
    }

    fn bump_tok(&mut self) -> Result<Token, ParseError> {
        if let Some(t) = self.peeked.take() {
            return Ok(t);
        }
        self.lexer.next_token()
    }

    fn slice(&self, span: Span) -> &'src str {
        let s = span.start as usize;
        let e = span.end as usize;
        let s = if s > self.src.len() { self.src.len() } else { s };
        let e = if e > self.src.len() { self.src.len() } else { e };
        let s = if s > e { e } else { s };
        &self.src[s..e]
    }

    fn expect(&mut self, kind: TokenKind, err: ParseErrorKind) -> Result<Token, ParseError> {
        let t = self.peek()?;
        if t.kind == kind {
            self.bump_tok()
        } else {
            Err(ParseError::new(t.span, err))
        }
    }

    // --- Productions ----------------------------------------------------

    fn parse_document(&mut self) -> Result<Document<'src, 'bump>, ParseError> {
        let mut defs = BVec::new_in(self.arena);
        loop {
            let t = self.peek()?;
            if t.kind == TokenKind::Eof {
                break;
            }
            defs.push(self.parse_definition()?);
        }
        if defs.is_empty() {
            // An empty document is technically a parse error per spec
            // (ExecutableDocument := ExecutableDefinition+). Surface it.
            let span = Span::new(0, self.src.len() as u32);
            return Err(ParseError::new(span, ParseErrorKind::UnexpectedEof));
        }
        Ok(Document { definitions: defs })
    }

    fn parse_definition(&mut self) -> Result<Definition<'src, 'bump>, ParseError> {
        let t = self.peek()?;
        match t.kind {
            TokenKind::LBrace => Ok(Definition::Operation(self.parse_shorthand_query()?)),
            TokenKind::Name => {
                let kw = self.slice(t.span);
                match kw {
                    "query" | "mutation" | "subscription" => {
                        Ok(Definition::Operation(self.parse_operation_definition()?))
                    }
                    "fragment" => Ok(Definition::Fragment(self.parse_fragment_definition()?)),
                    _ => Err(ParseError::new(t.span, ParseErrorKind::UnknownDefinition)),
                }
            }
            _ => Err(ParseError::new(t.span, ParseErrorKind::UnknownDefinition)),
        }
    }

    fn parse_shorthand_query(&mut self) -> Result<OperationDefinition<'src, 'bump>, ParseError> {
        let start = self.peek()?.span.start;
        let selection_set = self.parse_selection_set()?;
        let end = selection_set.span.end;
        Ok(OperationDefinition {
            kind: OperationKind::Query,
            name: None,
            variable_definitions: BVec::new_in(self.arena),
            directives: BVec::new_in(self.arena),
            selection_set,
            span: Span::new(start, end),
            shorthand: true,
        })
    }

    fn parse_operation_definition(
        &mut self,
    ) -> Result<OperationDefinition<'src, 'bump>, ParseError> {
        let kw_tok = self.bump_tok()?; // consume keyword
        let kind = match self.slice(kw_tok.span) {
            "query" => OperationKind::Query,
            "mutation" => OperationKind::Mutation,
            "subscription" => OperationKind::Subscription,
            _ => return Err(ParseError::new(kw_tok.span, ParseErrorKind::ExpectedOperationKind)),
        };

        let name = if self.peek()?.kind == TokenKind::Name {
            Some(self.parse_name()?)
        } else {
            None
        };

        let variable_definitions = if self.peek()?.kind == TokenKind::LParen {
            self.parse_variable_definitions()?
        } else {
            BVec::new_in(self.arena)
        };

        let directives = self.parse_directives()?;
        let selection_set = self.parse_selection_set()?;
        let end = selection_set.span.end;
        Ok(OperationDefinition {
            kind,
            name,
            variable_definitions,
            directives,
            selection_set,
            span: Span::new(kw_tok.span.start, end),
            shorthand: false,
        })
    }

    fn parse_fragment_definition(&mut self) -> Result<FragmentDefinition<'src, 'bump>, ParseError> {
        let kw_tok = self.bump_tok()?; // 'fragment'
        let name = self.parse_name()?;
        // 'on' keyword
        let on_tok = self.peek()?;
        if !(on_tok.kind == TokenKind::Name && self.slice(on_tok.span) == "on") {
            return Err(ParseError::new(on_tok.span, ParseErrorKind::ExpectedOnKeyword));
        }
        self.bump_tok()?;
        let type_cond_name = self.parse_name()?;
        let type_condition = NamedType { name: type_cond_name };
        let directives = self.parse_directives()?;
        let selection_set = self.parse_selection_set()?;
        let end = selection_set.span.end;
        Ok(FragmentDefinition {
            name,
            type_condition,
            directives,
            selection_set,
            span: Span::new(kw_tok.span.start, end),
        })
    }

    fn parse_name(&mut self) -> Result<Name<'src>, ParseError> {
        let t = self.peek()?;
        if t.kind != TokenKind::Name {
            return Err(ParseError::new(t.span, ParseErrorKind::ExpectedName));
        }
        self.bump_tok()?;
        Ok(Name {
            value: self.slice(t.span),
            span: t.span,
        })
    }

    fn parse_variable_definitions(
        &mut self,
    ) -> Result<BVec<'bump, VariableDefinition<'src, 'bump>>, ParseError> {
        // (
        self.expect(TokenKind::LParen, ParseErrorKind::UnexpectedToken)?;
        let mut out = BVec::new_in(self.arena);
        loop {
            let t = self.peek()?;
            if t.kind == TokenKind::RParen {
                self.bump_tok()?;
                break;
            }
            if t.kind == TokenKind::Eof {
                return Err(ParseError::new(t.span, ParseErrorKind::UnclosedDelimiter));
            }
            // $name : Type [= default] [Directives]
            self.expect(TokenKind::Dollar, ParseErrorKind::ExpectedDollar)?;
            let name = self.parse_name()?;
            self.expect(TokenKind::Colon, ParseErrorKind::ExpectedColon)?;
            let var_type = self.parse_type()?;
            let default_value = if self.peek()?.kind == TokenKind::Eq {
                self.bump_tok()?;
                Some(self.parse_value(/*const*/ true)?)
            } else {
                None
            };
            let directives = self.parse_directives()?;
            out.push(VariableDefinition {
                name,
                var_type,
                default_value,
                directives,
            });
        }
        Ok(out)
    }

    fn parse_type(&mut self) -> Result<Type<'src, 'bump>, ParseError> {
        let t = self.peek()?;
        let inner = match t.kind {
            TokenKind::Name => {
                let name = self.parse_name()?;
                Type::Named(NamedType { name })
            }
            TokenKind::LBracket => {
                self.bump_tok()?; // [
                let elem = self.parse_type()?;
                self.expect(TokenKind::RBracket, ParseErrorKind::UnclosedDelimiter)?;
                Type::List(self.arena.alloc(elem))
            }
            _ => {
                return Err(ParseError::new(t.span, ParseErrorKind::ExpectedType));
            }
        };
        if self.peek()?.kind == TokenKind::Bang {
            self.bump_tok()?;
            Ok(Type::NonNull(self.arena.alloc(inner)))
        } else {
            Ok(inner)
        }
    }

    fn parse_directives(&mut self) -> Result<BVec<'bump, Directive<'src, 'bump>>, ParseError> {
        let mut out = BVec::new_in(self.arena);
        while self.peek()?.kind == TokenKind::At {
            self.bump_tok()?; // @
            let name = self.parse_name()?;
            let arguments = if self.peek()?.kind == TokenKind::LParen {
                self.parse_arguments()?
            } else {
                BVec::new_in(self.arena)
            };
            out.push(Directive { name, arguments });
        }
        Ok(out)
    }

    fn parse_arguments(&mut self) -> Result<BVec<'bump, Argument<'src, 'bump>>, ParseError> {
        self.expect(TokenKind::LParen, ParseErrorKind::UnexpectedToken)?;
        let mut out = BVec::new_in(self.arena);
        loop {
            let t = self.peek()?;
            if t.kind == TokenKind::RParen {
                self.bump_tok()?;
                break;
            }
            if t.kind == TokenKind::Eof {
                return Err(ParseError::new(t.span, ParseErrorKind::UnclosedDelimiter));
            }
            let name = self.parse_name()?;
            self.expect(TokenKind::Colon, ParseErrorKind::ExpectedColon)?;
            let value = self.parse_value(false)?;
            out.push(Argument { name, value });
        }
        Ok(out)
    }

    fn parse_selection_set(&mut self) -> Result<SelectionSet<'src, 'bump>, ParseError> {
        let open = self.expect(TokenKind::LBrace, ParseErrorKind::UnexpectedToken)?;
        let mut selections = BVec::new_in(self.arena);
        loop {
            let t = self.peek()?;
            if t.kind == TokenKind::RBrace {
                let close = self.bump_tok()?;
                if selections.is_empty() {
                    return Err(ParseError::new(
                        Span::new(open.span.start, close.span.end),
                        ParseErrorKind::EmptySelectionSet,
                    ));
                }
                return Ok(SelectionSet {
                    selections,
                    span: Span::new(open.span.start, close.span.end),
                });
            }
            if t.kind == TokenKind::Eof {
                return Err(ParseError::new(t.span, ParseErrorKind::UnclosedDelimiter));
            }
            selections.push(self.parse_selection()?);
        }
    }

    fn parse_selection(&mut self) -> Result<Selection<'src, 'bump>, ParseError> {
        let t = self.peek()?;
        if t.kind == TokenKind::Spread {
            self.bump_tok()?; // ...
            let next = self.peek()?;
            // FragmentSpread: ... Name (Name != "on") [Directives]
            // InlineFragment (typed): ... on NamedType [Directives] SelectionSet
            // InlineFragment (untyped): ... [Directives] SelectionSet
            if next.kind == TokenKind::Name {
                let kw = self.slice(next.span);
                if kw == "on" {
                    self.bump_tok()?;
                    let type_cond_name = self.parse_name()?;
                    let directives = self.parse_directives()?;
                    let selection_set = self.parse_selection_set()?;
                    return Ok(Selection::InlineFragment(InlineFragment {
                        type_condition: Some(NamedType { name: type_cond_name }),
                        directives,
                        selection_set,
                    }));
                } else {
                    let name = self.parse_name()?;
                    let directives = self.parse_directives()?;
                    return Ok(Selection::FragmentSpread(FragmentSpread { name, directives }));
                }
            }
            // Untyped inline fragment: ... [@dir]* { ... }
            let directives = self.parse_directives()?;
            let selection_set = self.parse_selection_set()?;
            return Ok(Selection::InlineFragment(InlineFragment {
                type_condition: None,
                directives,
                selection_set,
            }));
        }

        // Field [Alias :] Name [Args] [Dir] [SelectionSet]
        if t.kind != TokenKind::Name {
            return Err(ParseError::new(t.span, ParseErrorKind::ExpectedName));
        }
        let first = self.parse_name()?;
        let (alias, name) = if self.peek()?.kind == TokenKind::Colon {
            self.bump_tok()?;
            let real = self.parse_name()?;
            (Some(first), real)
        } else {
            (None, first)
        };
        let arguments = if self.peek()?.kind == TokenKind::LParen {
            self.parse_arguments()?
        } else {
            BVec::new_in(self.arena)
        };
        let directives = self.parse_directives()?;
        let selection_set = if self.peek()?.kind == TokenKind::LBrace {
            Some(self.parse_selection_set()?)
        } else {
            None
        };
        Ok(Selection::Field(Field {
            alias,
            name,
            arguments,
            directives,
            selection_set,
        }))
    }

    /// Parse a Value. `is_const` rejects `$variable` (used in default values).
    fn parse_value(&mut self, is_const: bool) -> Result<Value<'src, 'bump>, ParseError> {
        let t = self.peek()?;
        match t.kind {
            TokenKind::Dollar => {
                if is_const {
                    return Err(ParseError::new(t.span, ParseErrorKind::ExpectedValue));
                }
                self.bump_tok()?;
                let name = self.parse_name()?;
                Ok(Value::Variable(name))
            }
            TokenKind::IntValue => {
                self.bump_tok()?;
                Ok(Value::Int(self.slice(t.span)))
            }
            TokenKind::FloatValue => {
                self.bump_tok()?;
                Ok(Value::Float(self.slice(t.span)))
            }
            TokenKind::StringValue => {
                self.bump_tok()?;
                Ok(Value::String(StringValue {
                    raw: self.slice(t.span),
                    block: false,
                    span: t.span,
                }))
            }
            TokenKind::BlockStringValue => {
                self.bump_tok()?;
                Ok(Value::String(StringValue {
                    raw: self.slice(t.span),
                    block: true,
                    span: t.span,
                }))
            }
            TokenKind::Name => {
                // true | false | null | Enum
                let lex = self.slice(t.span);
                self.bump_tok()?;
                Ok(match lex {
                    "true" => Value::Boolean(true),
                    "false" => Value::Boolean(false),
                    "null" => Value::Null,
                    _ => Value::Enum(Name { value: lex, span: t.span }),
                })
            }
            TokenKind::LBracket => {
                self.bump_tok()?;
                let mut items = BVec::new_in(self.arena);
                loop {
                    let nt = self.peek()?;
                    if nt.kind == TokenKind::RBracket {
                        self.bump_tok()?;
                        break;
                    }
                    if nt.kind == TokenKind::Eof {
                        return Err(ParseError::new(nt.span, ParseErrorKind::UnclosedDelimiter));
                    }
                    items.push(self.parse_value(is_const)?);
                }
                Ok(Value::List(items))
            }
            TokenKind::LBrace => {
                self.bump_tok()?;
                let mut fields = BVec::new_in(self.arena);
                loop {
                    let nt = self.peek()?;
                    if nt.kind == TokenKind::RBrace {
                        self.bump_tok()?;
                        break;
                    }
                    if nt.kind == TokenKind::Eof {
                        return Err(ParseError::new(nt.span, ParseErrorKind::UnclosedDelimiter));
                    }
                    let name = self.parse_name()?;
                    self.expect(TokenKind::Colon, ParseErrorKind::ExpectedColon)?;
                    let value = self.parse_value(is_const)?;
                    fields.push(ObjectField { name, value });
                }
                Ok(Value::Object(fields))
            }
            _ => Err(ParseError::new(t.span, ParseErrorKind::ExpectedValue)),
        }
    }
}
