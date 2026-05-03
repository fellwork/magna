// SPDX-License-Identifier: MIT OR Apache-2.0
//! magna-gqlmin — lightweight GraphQL parser.
//!
//! **Stability: experimental.** Breaking changes may land in any minor
//! version on the 0.x track. See `GOVERNANCE.md` for the full policy.
//!
//! The crate compiles in three shapes:
//!
//! * Default `ops + std`: a zero-runtime-dep operations parser usable from
//!   ordinary Rust callers.
//! * `--no-default-features --features ops,wasm`: pure `no_std` build for
//!   `wasm32-unknown-unknown` (later rounds wire the export shim).
//! * `ops + sdl + validate + pretty + serde[+napi]`: full build for
//!   build-time tooling (later rounds).
//!
//! Round 1 ships the lexer, the operations AST, a recursive-descent
//! operations parser, and a 20-case test corpus. SDL parsing, validation,
//! pretty-error rendering, napi, and the wasm export shim are gated and
//! arrive in later rounds.

#![cfg_attr(not(feature = "std"), no_std)]
// The wasm shim (feature = "wasm") uses unsafe for allocator glue and extern "C"
// exports. Outside that module, unsafe is forbidden.
#![cfg_attr(not(feature = "wasm"), forbid(unsafe_code))]
#![deny(rust_2018_idioms)]

extern crate alloc;

pub mod error;
pub mod lex;
pub mod parse;

#[cfg(feature = "pretty")]
mod pretty;

#[cfg(feature = "wasm")]
mod wasm;

pub use error::{ParseError, ParseErrorKind};
pub use lex::{Lexer, Span, Token, TokenKind};
pub use parse::{
    Argument, Definition, Directive, Document, Field, FragmentDefinition, InlineFragment,
    Name, ObjectField, OperationDefinition, OperationKind, Selection, SelectionSet, StringValue,
    Type, Value, VariableDefinition,
};

/// Parse an executable GraphQL document (operations + fragment definitions).
///
/// This is the canonical `ops` entry point. Available whenever the `ops`
/// feature is enabled (which the default feature set turns on).
#[cfg(feature = "ops")]
pub fn parse_executable_document(src: &str) -> Result<Document<'_>, ParseError> {
    parse::parse_executable_document(src)
}
