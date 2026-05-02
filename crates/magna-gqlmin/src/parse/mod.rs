// SPDX-License-Identifier: MIT OR Apache-2.0
//! Operations parser (GraphQL Oct-2021 spec, sections 2.2–2.12).
//!
//! Round 1 step 1: AST types and a stub entry point. The recursive-descent
//! body lands in step 4.

use alloc::vec::Vec;

use crate::error::{ParseError, ParseErrorKind};
use crate::lex::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Document<'src> {
    pub definitions: Vec<Definition<'src>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Definition<'src> {
    Operation(OperationDefinition<'src>),
    Fragment(FragmentDefinition<'src>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    Query,
    Mutation,
    Subscription,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationDefinition<'src> {
    pub kind: OperationKind,
    pub name: Option<Name<'src>>,
    pub variable_definitions: Vec<VariableDefinition<'src>>,
    pub directives: Vec<Directive<'src>>,
    pub selection_set: SelectionSet<'src>,
    pub span: Span,
    pub shorthand: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FragmentDefinition<'src> {
    pub name: Name<'src>,
    pub type_condition: NamedType<'src>,
    pub directives: Vec<Directive<'src>>,
    pub selection_set: SelectionSet<'src>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Name<'src> {
    pub value: &'src str,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamedType<'src> {
    pub name: Name<'src>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariableDefinition<'src> {
    pub name: Name<'src>,
    pub var_type: Type<'src>,
    pub default_value: Option<Value<'src>>,
    pub directives: Vec<Directive<'src>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Directive<'src> {
    pub name: Name<'src>,
    pub arguments: Vec<Argument<'src>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Argument<'src> {
    pub name: Name<'src>,
    pub value: Value<'src>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectionSet<'src> {
    pub selections: Vec<Selection<'src>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Selection<'src> {
    Field(Field<'src>),
    FragmentSpread(FragmentSpread<'src>),
    InlineFragment(InlineFragment<'src>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field<'src> {
    pub alias: Option<Name<'src>>,
    pub name: Name<'src>,
    pub arguments: Vec<Argument<'src>>,
    pub directives: Vec<Directive<'src>>,
    pub selection_set: Option<SelectionSet<'src>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FragmentSpread<'src> {
    pub name: Name<'src>,
    pub directives: Vec<Directive<'src>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InlineFragment<'src> {
    pub type_condition: Option<NamedType<'src>>,
    pub directives: Vec<Directive<'src>>,
    pub selection_set: SelectionSet<'src>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type<'src> {
    Named(NamedType<'src>),
    List(alloc::boxed::Box<Type<'src>>),
    NonNull(alloc::boxed::Box<Type<'src>>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value<'src> {
    Variable(Name<'src>),
    Int(&'src str),
    Float(&'src str),
    String(StringValue<'src>),
    Boolean(bool),
    Null,
    Enum(Name<'src>),
    List(Vec<Value<'src>>),
    Object(Vec<ObjectField<'src>>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StringValue<'src> {
    pub raw: &'src str,
    pub block: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectField<'src> {
    pub name: Name<'src>,
    pub value: Value<'src>,
}

pub fn parse_executable_document(src: &str) -> Result<Document<'_>, ParseError> {
    let _ = src;
    Err(ParseError::new(
        Span::new(0, 0),
        ParseErrorKind::UnexpectedEof,
    ))
}
