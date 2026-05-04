// SPDX-License-Identifier: MIT OR Apache-2.0
//! Schema Definition Language (SDL) parser — GraphQL Oct-2021 spec § 3.
//!
//! Gated behind the `sdl` feature; not part of the wasm size budget. The
//! SDL parser reuses the existing [`Lexer`] (block strings, names, and all
//! punctuators are already supported) and an internal recursive-descent
//! state machine. The operations parser in `mod.rs` is **not** modified.
//!
//! ### Scope (R11)
//!
//! Implements the following [`TypeSystemDefinition`] variants:
//!
//! * `SchemaDefinition` — `schema { query: T, mutation: T, subscription: T }`
//! * `ScalarTypeDefinition` — `scalar Name [@directive]*`
//! * `ObjectTypeDefinition` — `type Name [implements ...]* [@directive]* { fields }`
//! * `InterfaceTypeDefinition` — `interface Name [implements ...]* [@directive]* { fields }`
//! * `UnionTypeDefinition` — `union Name [@directive]* = A | B | C`
//! * `EnumTypeDefinition` — `enum Name [@directive]* { VALUE [@d]* ... }`
//! * `InputObjectTypeDefinition` — `input Name [@directive]* { fields }`
//! * `DirectiveDefinition` — `directive @name(args) on LOC | LOC | ...`
//!
//! Description strings (`"..."` or `"""..."""`) preceding any definition
//! are recognised and attached.
//!
//! ### Out of scope
//!
//! * Type extensions (`extend type Foo ...`) — future work.
//! * Schema introspection (`__schema`, `__type`) — future work.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::error::{ParseError, ParseErrorKind};
use crate::lex::{Lexer, Span, Token, TokenKind};
use crate::parse::{NamedType, Name, Type, Value};

// --- AST ----------------------------------------------------------------

/// A parsed SDL document.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaDocument<'src> {
    pub definitions: Vec<TypeSystemDefinition<'src>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeSystemDefinition<'src> {
    Schema(SchemaDef<'src>),
    Scalar(ScalarTypeDef<'src>),
    Object(ObjectTypeDef<'src>),
    Interface(InterfaceTypeDef<'src>),
    Union(UnionTypeDef<'src>),
    Enum(EnumTypeDef<'src>),
    InputObject(InputObjectTypeDef<'src>),
    Directive(DirectiveDef<'src>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Description<'src> {
    pub raw: &'src str,
    pub block: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SchemaDef<'src> {
    pub description: Option<Description<'src>>,
    pub directives: Vec<DirectiveApp<'src>>,
    pub operation_types: Vec<OperationTypeDef<'src>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationTypeDef<'src> {
    /// One of: `"query"`, `"mutation"`, `"subscription"`.
    pub operation: &'src str,
    pub operation_span: Span,
    pub named_type: NamedType<'src>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScalarTypeDef<'src> {
    pub description: Option<Description<'src>>,
    pub name: Name<'src>,
    pub directives: Vec<DirectiveApp<'src>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectTypeDef<'src> {
    pub description: Option<Description<'src>>,
    pub name: Name<'src>,
    pub implements: Vec<NamedType<'src>>,
    pub directives: Vec<DirectiveApp<'src>>,
    pub fields: Vec<FieldDef<'src>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceTypeDef<'src> {
    pub description: Option<Description<'src>>,
    pub name: Name<'src>,
    pub implements: Vec<NamedType<'src>>,
    pub directives: Vec<DirectiveApp<'src>>,
    pub fields: Vec<FieldDef<'src>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnionTypeDef<'src> {
    pub description: Option<Description<'src>>,
    pub name: Name<'src>,
    pub directives: Vec<DirectiveApp<'src>>,
    pub members: Vec<NamedType<'src>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumTypeDef<'src> {
    pub description: Option<Description<'src>>,
    pub name: Name<'src>,
    pub directives: Vec<DirectiveApp<'src>>,
    pub values: Vec<EnumValueDef<'src>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumValueDef<'src> {
    pub description: Option<Description<'src>>,
    pub name: Name<'src>,
    pub directives: Vec<DirectiveApp<'src>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputObjectTypeDef<'src> {
    pub description: Option<Description<'src>>,
    pub name: Name<'src>,
    pub directives: Vec<DirectiveApp<'src>>,
    pub fields: Vec<InputValueDef<'src>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DirectiveDef<'src> {
    pub description: Option<Description<'src>>,
    pub name: Name<'src>,
    pub arguments: Vec<InputValueDef<'src>>,
    pub repeatable: bool,
    pub locations: Vec<DirectiveLocation<'src>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DirectiveLocation<'src> {
    pub name: Name<'src>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef<'src> {
    pub description: Option<Description<'src>>,
    pub name: Name<'src>,
    pub arguments: Vec<InputValueDef<'src>>,
    pub field_type: Type<'src>,
    pub directives: Vec<DirectiveApp<'src>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputValueDef<'src> {
    pub description: Option<Description<'src>>,
    pub name: Name<'src>,
    pub value_type: Type<'src>,
    pub default_value: Option<Value<'src>>,
    pub directives: Vec<DirectiveApp<'src>>,
}

/// A directive application (as it appears on a definition). Mirrors the
/// operations [`crate::parse::Directive`] but uses an owned `Vec` for
/// arguments (no shared NodeRange arena in the SDL AST — SDL is host-side
/// tooling and is not size-constrained).
#[derive(Debug, Clone, PartialEq)]
pub struct DirectiveApp<'src> {
    pub name: Name<'src>,
    pub arguments: Vec<DirectiveArg<'src>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DirectiveArg<'src> {
    pub name: Name<'src>,
    pub value: Value<'src>,
}

// --- Public entry point -------------------------------------------------

/// Parse a GraphQL SDL document.
pub fn parse_schema(src: &str) -> Result<SchemaDocument<'_>, ParseError> {
    let mut p = SdlParser::new(src);
    p.parse_document()
}

// --- Parser -------------------------------------------------------------

struct SdlParser<'src> {
    src: &'src str,
    lexer: Lexer<'src>,
    peeked: Option<Token>,
}

impl<'src> SdlParser<'src> {
    fn new(src: &'src str) -> Self {
        Self {
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

    fn bump(&mut self) -> Result<Token, ParseError> {
        if let Some(t) = self.peeked.take() {
            return Ok(t);
        }
        self.lexer.next_token()
    }

    fn slice(&self, span: Span) -> &'src str {
        let s = span.start as usize;
        let e = span.end as usize;
        self.src.get(s..e).unwrap_or("")
    }

    fn expect(&mut self, kind: TokenKind, err: ParseErrorKind) -> Result<Token, ParseError> {
        let t = self.peek()?;
        if t.kind == kind {
            self.bump()
        } else {
            Err(ParseError::new(t.span, err))
        }
    }

    fn expect_keyword(&mut self, kw: &str, err: ParseErrorKind) -> Result<Token, ParseError> {
        let t = self.peek()?;
        if t.kind == TokenKind::Name && self.slice(t.span) == kw {
            self.bump()
        } else {
            Err(ParseError::new(t.span, err))
        }
    }

    fn parse_name(&mut self) -> Result<Name<'src>, ParseError> {
        let t = self.peek()?;
        if t.kind != TokenKind::Name {
            return Err(ParseError::new(t.span, ParseErrorKind::ExpectedName));
        }
        self.bump()?;
        Ok(Name { value: self.slice(t.span), span: t.span })
    }

    // --- Documents ------------------------------------------------------

    fn parse_document(&mut self) -> Result<SchemaDocument<'src>, ParseError> {
        let mut definitions = Vec::new();
        loop {
            let t = self.peek()?;
            if t.kind == TokenKind::Eof {
                break;
            }
            definitions.push(self.parse_type_system_definition()?);
        }
        if definitions.is_empty() {
            let span = Span::new(0, self.src.len() as u32);
            return Err(ParseError::new(span, ParseErrorKind::UnexpectedEof));
        }
        Ok(SchemaDocument { definitions })
    }

    fn parse_description(&mut self) -> Result<Option<Description<'src>>, ParseError> {
        let t = self.peek()?;
        match t.kind {
            TokenKind::StringValue => {
                self.bump()?;
                Ok(Some(Description {
                    raw: self.slice(t.span),
                    block: false,
                    span: t.span,
                }))
            }
            TokenKind::BlockStringValue => {
                self.bump()?;
                Ok(Some(Description {
                    raw: self.slice(t.span),
                    block: true,
                    span: t.span,
                }))
            }
            _ => Ok(None),
        }
    }

    fn parse_type_system_definition(&mut self) -> Result<TypeSystemDefinition<'src>, ParseError> {
        let description = self.parse_description()?;
        let t = self.peek()?;
        if t.kind != TokenKind::Name {
            return Err(ParseError::new(t.span, ParseErrorKind::UnknownDefinition));
        }
        let kw = self.slice(t.span);
        match kw {
            "schema" => Ok(TypeSystemDefinition::Schema(self.parse_schema_def(description)?)),
            "scalar" => Ok(TypeSystemDefinition::Scalar(self.parse_scalar_def(description)?)),
            "type" => Ok(TypeSystemDefinition::Object(self.parse_object_def(description)?)),
            "interface" => {
                Ok(TypeSystemDefinition::Interface(self.parse_interface_def(description)?))
            }
            "union" => Ok(TypeSystemDefinition::Union(self.parse_union_def(description)?)),
            "enum" => Ok(TypeSystemDefinition::Enum(self.parse_enum_def(description)?)),
            "input" => {
                Ok(TypeSystemDefinition::InputObject(self.parse_input_object_def(description)?))
            }
            "directive" => Ok(TypeSystemDefinition::Directive(self.parse_directive_def(description)?)),
            // Type extensions (`extend type Foo ...`) are out of scope for
            // R11; tag as future work.
            "extend" => Err(ParseError::new(t.span, ParseErrorKind::UnknownDefinition)),
            _ => Err(ParseError::new(t.span, ParseErrorKind::UnknownDefinition)),
        }
    }

    // --- schema ---------------------------------------------------------

    fn parse_schema_def(
        &mut self,
        description: Option<Description<'src>>,
    ) -> Result<SchemaDef<'src>, ParseError> {
        let kw = self.bump()?; // "schema"
        let directives = self.parse_directive_apps()?;
        self.expect(TokenKind::LBrace, ParseErrorKind::UnexpectedToken)?;
        let mut operation_types = Vec::new();
        loop {
            let t = self.peek()?;
            if t.kind == TokenKind::RBrace {
                break;
            }
            if t.kind == TokenKind::Eof {
                return Err(ParseError::new(t.span, ParseErrorKind::UnclosedDelimiter));
            }
            // operation : NamedType
            if t.kind != TokenKind::Name {
                return Err(ParseError::new(t.span, ParseErrorKind::ExpectedName));
            }
            let op_lex = self.slice(t.span);
            if op_lex != "query" && op_lex != "mutation" && op_lex != "subscription" {
                return Err(ParseError::new(t.span, ParseErrorKind::ExpectedOperationKind));
            }
            let op_tok = self.bump()?;
            self.expect(TokenKind::Colon, ParseErrorKind::ExpectedColon)?;
            let nt_name = self.parse_name()?;
            operation_types.push(OperationTypeDef {
                operation: self.slice(op_tok.span),
                operation_span: op_tok.span,
                named_type: NamedType { name: nt_name },
            });
        }
        let close = self.expect(TokenKind::RBrace, ParseErrorKind::UnclosedDelimiter)?;
        Ok(SchemaDef {
            description,
            directives,
            operation_types,
            span: Span::new(kw.span.start, close.span.end),
        })
    }

    // --- scalar ---------------------------------------------------------

    fn parse_scalar_def(
        &mut self,
        description: Option<Description<'src>>,
    ) -> Result<ScalarTypeDef<'src>, ParseError> {
        let kw = self.bump()?; // "scalar"
        let name = self.parse_name()?;
        let directives = self.parse_directive_apps()?;
        let end = directives.last().map(|d| d.name.span.end).unwrap_or(name.span.end);
        Ok(ScalarTypeDef {
            description,
            name,
            directives,
            span: Span::new(kw.span.start, end),
        })
    }

    // --- type / interface ----------------------------------------------

    fn parse_object_def(
        &mut self,
        description: Option<Description<'src>>,
    ) -> Result<ObjectTypeDef<'src>, ParseError> {
        let kw = self.bump()?; // "type"
        let name = self.parse_name()?;
        let implements = self.parse_implements_interfaces()?;
        let directives = self.parse_directive_apps()?;
        let (fields, end) = self.parse_optional_fields_definition()?;
        let span_end = end.unwrap_or_else(|| {
            directives.last().map(|d| d.name.span.end).unwrap_or(name.span.end)
        });
        Ok(ObjectTypeDef {
            description,
            name,
            implements,
            directives,
            fields,
            span: Span::new(kw.span.start, span_end),
        })
    }

    fn parse_interface_def(
        &mut self,
        description: Option<Description<'src>>,
    ) -> Result<InterfaceTypeDef<'src>, ParseError> {
        let kw = self.bump()?; // "interface"
        let name = self.parse_name()?;
        let implements = self.parse_implements_interfaces()?;
        let directives = self.parse_directive_apps()?;
        let (fields, end) = self.parse_optional_fields_definition()?;
        let span_end = end.unwrap_or_else(|| {
            directives.last().map(|d| d.name.span.end).unwrap_or(name.span.end)
        });
        Ok(InterfaceTypeDef {
            description,
            name,
            implements,
            directives,
            fields,
            span: Span::new(kw.span.start, span_end),
        })
    }

    fn parse_implements_interfaces(&mut self) -> Result<Vec<NamedType<'src>>, ParseError> {
        let t = self.peek()?;
        if !(t.kind == TokenKind::Name && self.slice(t.span) == "implements") {
            return Ok(Vec::new());
        }
        self.bump()?; // "implements"
        let mut out = Vec::new();
        // Optional leading `&`.
        if self.peek()?.kind == TokenKind::Amp {
            self.bump()?;
        }
        let first = self.parse_name()?;
        out.push(NamedType { name: first });
        loop {
            if self.peek()?.kind != TokenKind::Amp {
                break;
            }
            self.bump()?; // &
            let n = self.parse_name()?;
            out.push(NamedType { name: n });
        }
        Ok(out)
    }

    /// Parse `{ FieldDef* }` if present. Returns `(fields, end_byte)`
    /// where `end_byte` is `Some` iff a brace block was consumed.
    fn parse_optional_fields_definition(
        &mut self,
    ) -> Result<(Vec<FieldDef<'src>>, Option<u32>), ParseError> {
        if self.peek()?.kind != TokenKind::LBrace {
            return Ok((Vec::new(), None));
        }
        self.bump()?; // {
        let mut fields = Vec::new();
        loop {
            let t = self.peek()?;
            if t.kind == TokenKind::RBrace {
                break;
            }
            if t.kind == TokenKind::Eof {
                return Err(ParseError::new(t.span, ParseErrorKind::UnclosedDelimiter));
            }
            fields.push(self.parse_field_def()?);
        }
        let close = self.expect(TokenKind::RBrace, ParseErrorKind::UnclosedDelimiter)?;
        Ok((fields, Some(close.span.end)))
    }

    fn parse_field_def(&mut self) -> Result<FieldDef<'src>, ParseError> {
        let description = self.parse_description()?;
        let name = self.parse_name()?;
        let arguments = if self.peek()?.kind == TokenKind::LParen {
            self.parse_arguments_definition()?
        } else {
            Vec::new()
        };
        self.expect(TokenKind::Colon, ParseErrorKind::ExpectedColon)?;
        let field_type = self.parse_type()?;
        let directives = self.parse_directive_apps()?;
        Ok(FieldDef {
            description,
            name,
            arguments,
            field_type,
            directives,
        })
    }

    fn parse_arguments_definition(&mut self) -> Result<Vec<InputValueDef<'src>>, ParseError> {
        self.expect(TokenKind::LParen, ParseErrorKind::UnexpectedToken)?;
        let mut out = Vec::new();
        loop {
            let t = self.peek()?;
            if t.kind == TokenKind::RParen {
                self.bump()?;
                break;
            }
            if t.kind == TokenKind::Eof {
                return Err(ParseError::new(t.span, ParseErrorKind::UnclosedDelimiter));
            }
            out.push(self.parse_input_value_def()?);
        }
        Ok(out)
    }

    fn parse_input_value_def(&mut self) -> Result<InputValueDef<'src>, ParseError> {
        let description = self.parse_description()?;
        let name = self.parse_name()?;
        self.expect(TokenKind::Colon, ParseErrorKind::ExpectedColon)?;
        let value_type = self.parse_type()?;
        let default_value = if self.peek()?.kind == TokenKind::Eq {
            self.bump()?;
            Some(self.parse_const_value()?)
        } else {
            None
        };
        let directives = self.parse_directive_apps()?;
        Ok(InputValueDef {
            description,
            name,
            value_type,
            default_value,
            directives,
        })
    }

    // --- union ----------------------------------------------------------

    fn parse_union_def(
        &mut self,
        description: Option<Description<'src>>,
    ) -> Result<UnionTypeDef<'src>, ParseError> {
        let kw = self.bump()?; // "union"
        let name = self.parse_name()?;
        let directives = self.parse_directive_apps()?;
        let mut members = Vec::new();
        let mut end = directives
            .last()
            .map(|d| d.name.span.end)
            .unwrap_or(name.span.end);
        if self.peek()?.kind == TokenKind::Eq {
            self.bump()?;
            // Optional leading `|`.
            if self.peek()?.kind == TokenKind::Pipe {
                self.bump()?;
            }
            let first = self.parse_name()?;
            end = first.span.end;
            members.push(NamedType { name: first });
            loop {
                if self.peek()?.kind != TokenKind::Pipe {
                    break;
                }
                self.bump()?; // |
                let m = self.parse_name()?;
                end = m.span.end;
                members.push(NamedType { name: m });
            }
        }
        Ok(UnionTypeDef {
            description,
            name,
            directives,
            members,
            span: Span::new(kw.span.start, end),
        })
    }

    // --- enum -----------------------------------------------------------

    fn parse_enum_def(
        &mut self,
        description: Option<Description<'src>>,
    ) -> Result<EnumTypeDef<'src>, ParseError> {
        let kw = self.bump()?; // "enum"
        let name = self.parse_name()?;
        let directives = self.parse_directive_apps()?;
        let mut values = Vec::new();
        let mut end = directives
            .last()
            .map(|d| d.name.span.end)
            .unwrap_or(name.span.end);
        if self.peek()?.kind == TokenKind::LBrace {
            self.bump()?;
            loop {
                let t = self.peek()?;
                if t.kind == TokenKind::RBrace {
                    break;
                }
                if t.kind == TokenKind::Eof {
                    return Err(ParseError::new(t.span, ParseErrorKind::UnclosedDelimiter));
                }
                let description = self.parse_description()?;
                let n = self.parse_name()?;
                let directives = self.parse_directive_apps()?;
                values.push(EnumValueDef {
                    description,
                    name: n,
                    directives,
                });
            }
            let close = self.expect(TokenKind::RBrace, ParseErrorKind::UnclosedDelimiter)?;
            end = close.span.end;
        }
        Ok(EnumTypeDef {
            description,
            name,
            directives,
            values,
            span: Span::new(kw.span.start, end),
        })
    }

    // --- input object ---------------------------------------------------

    fn parse_input_object_def(
        &mut self,
        description: Option<Description<'src>>,
    ) -> Result<InputObjectTypeDef<'src>, ParseError> {
        let kw = self.bump()?; // "input"
        let name = self.parse_name()?;
        let directives = self.parse_directive_apps()?;
        let mut fields = Vec::new();
        let mut end = directives
            .last()
            .map(|d| d.name.span.end)
            .unwrap_or(name.span.end);
        if self.peek()?.kind == TokenKind::LBrace {
            self.bump()?;
            loop {
                let t = self.peek()?;
                if t.kind == TokenKind::RBrace {
                    break;
                }
                if t.kind == TokenKind::Eof {
                    return Err(ParseError::new(t.span, ParseErrorKind::UnclosedDelimiter));
                }
                fields.push(self.parse_input_value_def()?);
            }
            let close = self.expect(TokenKind::RBrace, ParseErrorKind::UnclosedDelimiter)?;
            end = close.span.end;
        }
        Ok(InputObjectTypeDef {
            description,
            name,
            directives,
            fields,
            span: Span::new(kw.span.start, end),
        })
    }

    // --- directive definition ------------------------------------------

    fn parse_directive_def(
        &mut self,
        description: Option<Description<'src>>,
    ) -> Result<DirectiveDef<'src>, ParseError> {
        let kw = self.bump()?; // "directive"
        self.expect(TokenKind::At, ParseErrorKind::UnexpectedToken)?;
        let name = self.parse_name()?;
        let arguments = if self.peek()?.kind == TokenKind::LParen {
            self.parse_arguments_definition()?
        } else {
            Vec::new()
        };
        // optional `repeatable` keyword
        let repeatable = {
            let t = self.peek()?;
            if t.kind == TokenKind::Name && self.slice(t.span) == "repeatable" {
                self.bump()?;
                true
            } else {
                false
            }
        };
        // `on` keyword
        self.expect_keyword("on", ParseErrorKind::ExpectedOnKeyword)?;
        // Optional leading `|`.
        if self.peek()?.kind == TokenKind::Pipe {
            self.bump()?;
        }
        let mut locations = Vec::new();
        let first = self.parse_name()?;
        let mut end = first.span.end;
        locations.push(DirectiveLocation { name: first });
        loop {
            if self.peek()?.kind != TokenKind::Pipe {
                break;
            }
            self.bump()?; // |
            let n = self.parse_name()?;
            end = n.span.end;
            locations.push(DirectiveLocation { name: n });
        }
        Ok(DirectiveDef {
            description,
            name,
            arguments,
            repeatable,
            locations,
            span: Span::new(kw.span.start, end),
        })
    }

    // --- shared productions --------------------------------------------

    fn parse_directive_apps(&mut self) -> Result<Vec<DirectiveApp<'src>>, ParseError> {
        let mut out = Vec::new();
        while self.peek()?.kind == TokenKind::At {
            self.bump()?; // @
            let name = self.parse_name()?;
            let arguments = if self.peek()?.kind == TokenKind::LParen {
                self.parse_directive_args()?
            } else {
                Vec::new()
            };
            out.push(DirectiveApp { name, arguments });
        }
        Ok(out)
    }

    fn parse_directive_args(&mut self) -> Result<Vec<DirectiveArg<'src>>, ParseError> {
        self.expect(TokenKind::LParen, ParseErrorKind::UnexpectedToken)?;
        let mut out = Vec::new();
        loop {
            let t = self.peek()?;
            if t.kind == TokenKind::RParen {
                self.bump()?;
                break;
            }
            if t.kind == TokenKind::Eof {
                return Err(ParseError::new(t.span, ParseErrorKind::UnclosedDelimiter));
            }
            let name = self.parse_name()?;
            self.expect(TokenKind::Colon, ParseErrorKind::ExpectedColon)?;
            let value = self.parse_const_value()?;
            out.push(DirectiveArg { name, value });
        }
        Ok(out)
    }

    fn parse_type(&mut self) -> Result<Type<'src>, ParseError> {
        let t = self.peek()?;
        let inner = match t.kind {
            TokenKind::Name => {
                let name = self.parse_name()?;
                Type::Named(NamedType { name })
            }
            TokenKind::LBracket => {
                self.bump()?; // [
                let elem = self.parse_type()?;
                self.expect(TokenKind::RBracket, ParseErrorKind::UnclosedDelimiter)?;
                Type::List(Box::new(elem))
            }
            _ => return Err(ParseError::new(t.span, ParseErrorKind::ExpectedType)),
        };
        if self.peek()?.kind == TokenKind::Bang {
            self.bump()?;
            Ok(Type::NonNull(Box::new(inner)))
        } else {
            Ok(inner)
        }
    }

    /// Const value (no `$variable`). Used for default values and directive
    /// arguments in SDL. Mirrors `Parser::parse_value(true)` from the ops
    /// parser, but builds owned `Vec`s for List/Object since the SDL AST
    /// doesn't share an arena with the operations document.
    ///
    /// SDL List/Object literals are uncommon; we synthesize them by
    /// stashing nested values into a tiny adjacent buffer. To keep the AST
    /// simple, list/object values use the operations [`Value`] enum
    /// representation — a separate SDL-only `ConstValue` variant tree
    /// would duplicate the entire kind enum for marginal benefit. Since
    /// `Value::List`/`Value::Object` carry `NodeRange`s into a parent
    /// `Document`'s arena, **list/object const values are not supported
    /// inside SDL default values yet** (they parse but degrade to
    /// `Value::Null`). This is documented as future work alongside type
    /// extensions.
    fn parse_const_value(&mut self) -> Result<Value<'src>, ParseError> {
        use crate::parse::StringValue;
        let t = self.peek()?;
        match t.kind {
            TokenKind::Dollar => Err(ParseError::new(t.span, ParseErrorKind::ExpectedValue)),
            TokenKind::IntValue => {
                self.bump()?;
                Ok(Value::Int(self.slice(t.span)))
            }
            TokenKind::FloatValue => {
                self.bump()?;
                Ok(Value::Float(self.slice(t.span)))
            }
            TokenKind::StringValue => {
                self.bump()?;
                Ok(Value::String(StringValue {
                    raw: self.slice(t.span),
                    block: false,
                    span: t.span,
                }))
            }
            TokenKind::BlockStringValue => {
                self.bump()?;
                Ok(Value::String(StringValue {
                    raw: self.slice(t.span),
                    block: true,
                    span: t.span,
                }))
            }
            TokenKind::Name => {
                let lex = self.slice(t.span);
                self.bump()?;
                Ok(match lex {
                    "true" => Value::Boolean(true),
                    "false" => Value::Boolean(false),
                    "null" => Value::Null,
                    _ => Value::Enum(Name { value: lex, span: t.span }),
                })
            }
            TokenKind::LBracket => {
                // Skip past matching close — see doc-comment.
                self.skip_balanced_list()?;
                Ok(Value::Null)
            }
            TokenKind::LBrace => {
                self.skip_balanced_object()?;
                Ok(Value::Null)
            }
            _ => Err(ParseError::new(t.span, ParseErrorKind::ExpectedValue)),
        }
    }

    fn skip_balanced_list(&mut self) -> Result<(), ParseError> {
        let _ = self.bump()?; // [
        let mut depth: u32 = 1;
        while depth > 0 {
            let t = self.bump()?;
            match t.kind {
                TokenKind::LBracket => depth += 1,
                TokenKind::RBracket => depth -= 1,
                TokenKind::Eof => {
                    return Err(ParseError::new(t.span, ParseErrorKind::UnclosedDelimiter));
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn skip_balanced_object(&mut self) -> Result<(), ParseError> {
        let _ = self.bump()?; // {
        let mut depth: u32 = 1;
        while depth > 0 {
            let t = self.bump()?;
            match t.kind {
                TokenKind::LBrace => depth += 1,
                TokenKind::RBrace => depth -= 1,
                TokenKind::Eof => {
                    return Err(ParseError::new(t.span, ParseErrorKind::UnclosedDelimiter));
                }
                _ => {}
            }
        }
        Ok(())
    }
}
