// SPDX-License-Identifier: MIT OR Apache-2.0
//! Operations parser (GraphQL Oct-2021 spec, sections 2.2–2.12).
//!
//! Hand-rolled LL(1) recursive descent over the lexer. One token of
//! lookahead via `Parser { peeked: Option<Token> }`. No backtracking.
//!
//! ### AST storage (R5 phase 3 — span-indexed flat array)
//!
//! All seven previously-distinct `Vec<T>` collections in the AST collapse
//! into ONE `Vec<Node<'src>>` shared by the entire document. Each list
//! field on every AST node is a [`NodeRange`] (`{ start: u32, len: u32 }`)
//! into the document's `nodes` arena. The `Node` enum carries every shape
//! that can appear inside a list:
//!
//! * `Node::Definition`         — top-level definitions
//! * `Node::VariableDefinition` — operation variables
//! * `Node::Directive`          — directive applications
//! * `Node::Argument`           — directive / field arguments
//! * `Node::Selection`          — fields, fragment spreads, inline fragments
//! * `Node::ObjectField`        — input-object fields
//! * `Node::Value`              — values inside `Value::List`
//!
//! Result: ONE `Vec` instantiation in the wasm binary instead of seven.
//! See `docs/investigation-r5-span-indexed-design.md` for the design
//! rationale and `docs/investigation-r3-bumpalo-panic-bloat.md` for why
//! the bumpalo path was abandoned.
//!
//! Public API stays single-lifetime: `Document<'src>`. List-field access
//! flows through typed projections on `Document` (e.g.
//! [`Document::definitions`], [`Document::directives`]) which decode the
//! correct `Node` variant for the caller.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::error::{ParseError, ParseErrorKind};
use crate::lex::{Lexer, Span, Token, TokenKind};

// --- Span-indexed shared arena ------------------------------------------

/// Index range into [`Document::nodes`]. The element type at the range
/// is determined by the parent field's expectation (e.g.
/// `OperationDefinition::directives` always points at `Node::Directive`
/// elements).
#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NodeRange {
    pub start: u32,
    pub len: u32,
}

impl NodeRange {
    const EMPTY: NodeRange = NodeRange { start: 0, len: 0 };

    #[inline]
    pub fn is_empty(self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn len(self) -> usize {
        self.len as usize
    }
}

/// Tagged element of the unified node arena. One `Vec<Node>` per
/// document; every list field on the AST is a `NodeRange` slice into it.
#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub enum Node<'src> {
    Definition(Definition<'src>),
    VariableDefinition(VariableDefinition<'src>),
    Directive(Directive<'src>),
    Argument(Argument<'src>),
    Selection(Selection<'src>),
    ObjectField(ObjectField<'src>),
    Value(Value<'src>),
}

// --- AST ----------------------------------------------------------------

/// Top-level executable document.
///
/// Owns the shared `nodes` arena. List fields on every nested AST type
/// are `NodeRange` slices into this arena. Use the typed accessor
/// methods on `Document` (e.g. [`Document::definitions`],
/// [`Document::selections`]) to read them.
#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct Document<'src> {
    /// Range over `Node::Definition` elements in `nodes`. Use
    /// [`Document::definitions`] for typed access.
    pub definitions_range: NodeRange,
    pub nodes: Vec<Node<'src>>,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub enum Definition<'src> {
    Operation(OperationDefinition<'src>),
    Fragment(FragmentDefinition<'src>),
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
pub struct OperationDefinition<'src> {
    pub kind: OperationKind,
    pub name: Option<Name<'src>>,
    pub variable_definitions: NodeRange,
    pub directives: NodeRange,
    pub selection_set: SelectionSet,
    pub span: Span,
    /// True for `{ ... }` shorthand queries (no `query` keyword, no name).
    pub shorthand: bool,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct FragmentDefinition<'src> {
    pub name: Name<'src>,
    pub type_condition: NamedType<'src>,
    pub directives: NodeRange,
    pub selection_set: SelectionSet,
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
pub struct VariableDefinition<'src> {
    pub name: Name<'src>,
    pub var_type: Type<'src>,
    pub default_value: Option<Value<'src>>,
    pub directives: NodeRange,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct Directive<'src> {
    pub name: Name<'src>,
    pub arguments: NodeRange,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct Argument<'src> {
    pub name: Name<'src>,
    pub value: Value<'src>,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SelectionSet {
    pub selections: NodeRange,
    pub span: Span,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub enum Selection<'src> {
    Field(Field<'src>),
    FragmentSpread(FragmentSpread<'src>),
    InlineFragment(InlineFragment<'src>),
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct Field<'src> {
    pub alias: Option<Name<'src>>,
    pub name: Name<'src>,
    pub arguments: NodeRange,
    pub directives: NodeRange,
    pub selection_set: Option<SelectionSet>,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct FragmentSpread<'src> {
    pub name: Name<'src>,
    pub directives: NodeRange,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub struct InlineFragment<'src> {
    pub type_condition: Option<NamedType<'src>>,
    pub directives: NodeRange,
    pub selection_set: SelectionSet,
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub enum Type<'src> {
    Named(NamedType<'src>),
    List(Box<Type<'src>>),
    NonNull(Box<Type<'src>>),
}

#[cfg_attr(any(feature = "std", test), derive(Debug))]
#[derive(Clone, PartialEq)]
pub enum Value<'src> {
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
    /// Range over `Node::Value` elements in `Document::nodes`.
    List(NodeRange),
    /// Range over `Node::ObjectField` elements in `Document::nodes`.
    Object(NodeRange),
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
pub struct ObjectField<'src> {
    pub name: Name<'src>,
    pub value: Value<'src>,
}

// --- Typed accessors over the unified arena -----------------------------

/// Slice of `Node` elements pulled out of the document arena. Implements
/// the lookups callers want without exposing raw `&[Node]`.
///
/// One generic projector per list-element kind. Constructed by the
/// `Document::*` accessor methods — not by users directly.
pub struct NodeSlice<'doc, 'src, T: ?Sized + 'doc> {
    nodes: &'doc [Node<'src>],
    project: for<'a> fn(&'a Node<'src>) -> &'a T,
}

impl<'doc, 'src, T: ?Sized + 'doc> NodeSlice<'doc, 'src, T> {
    #[inline]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    #[inline]
    pub fn get(&self, i: usize) -> Option<&'doc T> {
        self.nodes.get(i).map(self.project)
    }

    #[inline]
    pub fn iter(&self) -> NodeSliceIter<'doc, 'src, T> {
        NodeSliceIter {
            nodes: self.nodes.iter(),
            project: self.project,
        }
    }
}

impl<'doc, 'src, T: ?Sized + 'doc> core::ops::Index<usize> for NodeSlice<'doc, 'src, T> {
    type Output = T;
    #[inline]
    fn index(&self, i: usize) -> &T {
        // `get` keeps the `core::slice::index` panic path unreachable from
        // this site. Out-of-bounds is funnelled through the format-free
        // `panic_invariant()`.
        match self.nodes.get(i) {
            Some(n) => (self.project)(n),
            None => panic_invariant(),
        }
    }
}

impl<'a, 'doc, 'src, T: ?Sized + 'doc> IntoIterator for &'a NodeSlice<'doc, 'src, T> {
    type Item = &'doc T;
    type IntoIter = NodeSliceIter<'doc, 'src, T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct NodeSliceIter<'doc, 'src, T: ?Sized + 'doc> {
    nodes: core::slice::Iter<'doc, Node<'src>>,
    project: for<'a> fn(&'a Node<'src>) -> &'a T,
}

impl<'doc, 'src, T: ?Sized + 'doc> Iterator for NodeSliceIter<'doc, 'src, T> {
    type Item = &'doc T;
    #[inline]
    fn next(&mut self) -> Option<&'doc T> {
        self.nodes.next().map(self.project)
    }
    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.nodes.size_hint()
    }
}

impl<'doc, 'src, T: ?Sized + 'doc> ExactSizeIterator for NodeSliceIter<'doc, 'src, T> {
    #[inline]
    fn len(&self) -> usize {
        self.nodes.len()
    }
}

// Projection functions — invariants enforced by the parser when it pushes
// nodes. If a NodeRange points at the wrong variant the projection
// panics via `panic_invariant()` (collapsed to `wasm32 unreachable`
// under panic = abort with no format-string emission).
//
// Higher-rank lifetime: borrow lifetime of the returned reference equals
// the borrow lifetime of `n`, satisfying `for<'a> fn(&'a Node<'src>) -> &'a T`.

#[inline]
fn project_definition<'a, 'src>(n: &'a Node<'src>) -> &'a Definition<'src> {
    match n {
        Node::Definition(v) => v,
        _ => panic_invariant(),
    }
}
#[inline]
fn project_variable_definition<'a, 'src>(n: &'a Node<'src>) -> &'a VariableDefinition<'src> {
    match n {
        Node::VariableDefinition(v) => v,
        _ => panic_invariant(),
    }
}
#[inline]
fn project_directive<'a, 'src>(n: &'a Node<'src>) -> &'a Directive<'src> {
    match n {
        Node::Directive(v) => v,
        _ => panic_invariant(),
    }
}
#[inline]
fn project_argument<'a, 'src>(n: &'a Node<'src>) -> &'a Argument<'src> {
    match n {
        Node::Argument(v) => v,
        _ => panic_invariant(),
    }
}
#[inline]
fn project_selection<'a, 'src>(n: &'a Node<'src>) -> &'a Selection<'src> {
    match n {
        Node::Selection(v) => v,
        _ => panic_invariant(),
    }
}
#[inline]
fn project_object_field<'a, 'src>(n: &'a Node<'src>) -> &'a ObjectField<'src> {
    match n {
        Node::ObjectField(v) => v,
        _ => panic_invariant(),
    }
}
#[inline]
fn project_value<'a, 'src>(n: &'a Node<'src>) -> &'a Value<'src> {
    match n {
        Node::Value(v) => v,
        _ => panic_invariant(),
    }
}

/// Format-free panic for AST invariant violations. Resolves to
/// `core::arch::wasm32::unreachable()` under `panic = abort`, with no
/// format-string or unicode-table emission.
#[cold]
#[inline(never)]
fn panic_invariant() -> ! {
    // No format string — keeps `core::fmt` unreachable from this site.
    panic!()
}

impl<'src> Document<'src> {
    /// Slice of top-level definitions.
    #[inline]
    pub fn definitions<'doc>(&'doc self) -> NodeSlice<'doc, 'src, Definition<'src>> {
        self.slice(self.definitions_range, project_definition)
    }

    /// Variable definitions of an operation.
    #[inline]
    pub fn variable_definitions<'doc>(
        &'doc self,
        r: NodeRange,
    ) -> NodeSlice<'doc, 'src, VariableDefinition<'src>> {
        self.slice(r, project_variable_definition)
    }

    /// Directive applications attached to any AST node.
    #[inline]
    pub fn directives<'doc>(&'doc self, r: NodeRange) -> NodeSlice<'doc, 'src, Directive<'src>> {
        self.slice(r, project_directive)
    }

    /// Arguments of a directive or field.
    #[inline]
    pub fn arguments<'doc>(&'doc self, r: NodeRange) -> NodeSlice<'doc, 'src, Argument<'src>> {
        self.slice(r, project_argument)
    }

    /// Selections inside a selection set.
    #[inline]
    pub fn selections<'doc>(&'doc self, r: NodeRange) -> NodeSlice<'doc, 'src, Selection<'src>> {
        self.slice(r, project_selection)
    }

    /// Object-literal field values.
    #[inline]
    pub fn object_fields<'doc>(
        &'doc self,
        r: NodeRange,
    ) -> NodeSlice<'doc, 'src, ObjectField<'src>> {
        self.slice(r, project_object_field)
    }

    /// Elements of a `Value::List`.
    #[inline]
    pub fn list_values<'doc>(&'doc self, r: NodeRange) -> NodeSlice<'doc, 'src, Value<'src>> {
        self.slice(r, project_value)
    }

    #[inline]
    fn slice<'doc, T: ?Sized + 'doc>(
        &'doc self,
        r: NodeRange,
        project: for<'a> fn(&'a Node<'src>) -> &'a T,
    ) -> NodeSlice<'doc, 'src, T> {
        let start = r.start as usize;
        let end = start + r.len as usize;
        // `get` returns Option, no panic-format reachable.
        let nodes = self.nodes.get(start..end).unwrap_or(&[]);
        NodeSlice { nodes, project }
    }
}

// --- Public entry point -------------------------------------------------

/// Parse an executable document. The returned `Document` borrows
/// identifiers and lexemes from `src`. AST list collections live in the
/// document's shared `nodes` arena (one `Vec<Node>` per document).
pub fn parse_executable_document<'src>(src: &'src str) -> Result<Document<'src>, ParseError> {
    let mut p = Parser::new(src);
    p.parse_document()
}

// --- Parser -------------------------------------------------------------

struct Parser<'src> {
    src: &'src str,
    lexer: Lexer<'src>,
    peeked: Option<Token>,
    /// Final shared node arena. List ranges (`NodeRange`) point here.
    nodes: Vec<Node<'src>>,
    /// Scratch stack used while building list productions. Each list
    /// production records `scratch.len()`, pushes children onto scratch
    /// during the list body, then drains them en bloc into `nodes` at
    /// the end. This guarantees a list's `NodeRange` points at
    /// contiguous nodes, even though parser recursion would otherwise
    /// interleave outer-list elements with inner-list contents.
    ///
    /// Same `Vec<Node>` element type as the final arena — only one Vec
    /// monomorphization is generated for the whole parser.
    scratch: Vec<Node<'src>>,
}

impl<'src> Parser<'src> {
    fn new(src: &'src str) -> Self {
        Self {
            src,
            lexer: Lexer::new(src),
            peeked: None,
            nodes: Vec::new(),
            scratch: Vec::new(),
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
        // Use `get` (returning Option) instead of `[..]` indexing so the
        // `core::str::index` panic path — which transitively reaches the
        // Unicode `printable.rs` property tables via Debug formatting —
        // stays unreachable from the parser hot path. Span values come from
        // the lexer and are valid by construction; the empty-string fallback
        // is defensive only.
        let s = span.start as usize;
        let e = span.end as usize;
        self.src.get(s..e).unwrap_or("")
    }

    fn expect(&mut self, kind: TokenKind, err: ParseErrorKind) -> Result<Token, ParseError> {
        let t = self.peek()?;
        if t.kind == kind {
            self.bump_tok()
        } else {
            Err(ParseError::new(t.span, err))
        }
    }

    /// Open a list production: returns the current scratch length.
    /// Push child `Node`s onto `self.scratch` between this call and the
    /// matching `close_list`.
    #[inline]
    fn open_list(&self) -> usize {
        self.scratch.len()
    }

    /// Close a list production: drain `scratch[start..]` into `self.nodes`
    /// en bloc and return the resulting NodeRange.
    #[inline]
    fn close_list(&mut self, scratch_start: usize) -> NodeRange {
        let count = self.scratch.len() - scratch_start;
        let nodes_start = self.nodes.len() as u32;
        self.nodes.extend(self.scratch.drain(scratch_start..));
        NodeRange {
            start: nodes_start,
            len: count as u32,
        }
    }

    // --- Productions ----------------------------------------------------

    fn parse_document(&mut self) -> Result<Document<'src>, ParseError> {
        let scratch_start = self.open_list();
        loop {
            let t = self.peek()?;
            if t.kind == TokenKind::Eof {
                break;
            }
            let def = self.parse_definition()?;
            self.scratch.push(Node::Definition(def));
        }
        if self.scratch.len() == scratch_start {
            // ExecutableDocument := ExecutableDefinition+
            let span = Span::new(0, self.src.len() as u32);
            return Err(ParseError::new(span, ParseErrorKind::UnexpectedEof));
        }
        let definitions_range = self.close_list(scratch_start);
        let nodes = core::mem::take(&mut self.nodes);
        Ok(Document {
            definitions_range,
            nodes,
        })
    }

    fn parse_definition(&mut self) -> Result<Definition<'src>, ParseError> {
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

    fn parse_shorthand_query(&mut self) -> Result<OperationDefinition<'src>, ParseError> {
        let start = self.peek()?.span.start;
        let selection_set = self.parse_selection_set()?;
        let end = selection_set.span.end;
        Ok(OperationDefinition {
            kind: OperationKind::Query,
            name: None,
            variable_definitions: NodeRange::EMPTY,
            directives: NodeRange::EMPTY,
            selection_set,
            span: Span::new(start, end),
            shorthand: true,
        })
    }

    fn parse_operation_definition(&mut self) -> Result<OperationDefinition<'src>, ParseError> {
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
            NodeRange::EMPTY
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

    fn parse_fragment_definition(&mut self) -> Result<FragmentDefinition<'src>, ParseError> {
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

    fn parse_variable_definitions(&mut self) -> Result<NodeRange, ParseError> {
        // (
        self.expect(TokenKind::LParen, ParseErrorKind::UnexpectedToken)?;
        let scratch_start = self.open_list();
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
            self.scratch.push(Node::VariableDefinition(VariableDefinition {
                name,
                var_type,
                default_value,
                directives,
            }));
        }
        Ok(self.close_list(scratch_start))
    }

    fn parse_type(&mut self) -> Result<Type<'src>, ParseError> {
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
                Type::List(Box::new(elem))
            }
            _ => {
                return Err(ParseError::new(t.span, ParseErrorKind::ExpectedType));
            }
        };
        if self.peek()?.kind == TokenKind::Bang {
            self.bump_tok()?;
            Ok(Type::NonNull(Box::new(inner)))
        } else {
            Ok(inner)
        }
    }

    fn parse_directives(&mut self) -> Result<NodeRange, ParseError> {
        let scratch_start = self.open_list();
        while self.peek()?.kind == TokenKind::At {
            self.bump_tok()?; // @
            let name = self.parse_name()?;
            let arguments = if self.peek()?.kind == TokenKind::LParen {
                self.parse_arguments()?
            } else {
                NodeRange::EMPTY
            };
            self.scratch.push(Node::Directive(Directive { name, arguments }));
        }
        Ok(self.close_list(scratch_start))
    }

    fn parse_arguments(&mut self) -> Result<NodeRange, ParseError> {
        self.expect(TokenKind::LParen, ParseErrorKind::UnexpectedToken)?;
        let scratch_start = self.open_list();
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
            self.scratch.push(Node::Argument(Argument { name, value }));
        }
        Ok(self.close_list(scratch_start))
    }

    fn parse_selection_set(&mut self) -> Result<SelectionSet, ParseError> {
        let open = self.expect(TokenKind::LBrace, ParseErrorKind::UnexpectedToken)?;
        let scratch_start = self.open_list();
        loop {
            let t = self.peek()?;
            if t.kind == TokenKind::RBrace {
                let close = self.bump_tok()?;
                if self.scratch.len() == scratch_start {
                    return Err(ParseError::new(
                        Span::new(open.span.start, close.span.end),
                        ParseErrorKind::EmptySelectionSet,
                    ));
                }
                let selections = self.close_list(scratch_start);
                return Ok(SelectionSet {
                    selections,
                    span: Span::new(open.span.start, close.span.end),
                });
            }
            if t.kind == TokenKind::Eof {
                return Err(ParseError::new(t.span, ParseErrorKind::UnclosedDelimiter));
            }
            let sel = self.parse_selection()?;
            self.scratch.push(Node::Selection(sel));
        }
    }

    fn parse_selection(&mut self) -> Result<Selection<'src>, ParseError> {
        let t = self.peek()?;
        if t.kind == TokenKind::Spread {
            self.bump_tok()?; // ...
            let next = self.peek()?;
            // FragmentSpread / typed-or-untyped InlineFragment.
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
            NodeRange::EMPTY
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
    fn parse_value(&mut self, is_const: bool) -> Result<Value<'src>, ParseError> {
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
                let scratch_start = self.open_list();
                loop {
                    let nt = self.peek()?;
                    if nt.kind == TokenKind::RBracket {
                        self.bump_tok()?;
                        break;
                    }
                    if nt.kind == TokenKind::Eof {
                        return Err(ParseError::new(nt.span, ParseErrorKind::UnclosedDelimiter));
                    }
                    let v = self.parse_value(is_const)?;
                    self.scratch.push(Node::Value(v));
                }
                Ok(Value::List(self.close_list(scratch_start)))
            }
            TokenKind::LBrace => {
                self.bump_tok()?;
                let scratch_start = self.open_list();
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
                    self.scratch.push(Node::ObjectField(ObjectField { name, value }));
                }
                Ok(Value::Object(self.close_list(scratch_start)))
            }
            _ => Err(ParseError::new(t.span, ParseErrorKind::ExpectedValue)),
        }
    }
}
