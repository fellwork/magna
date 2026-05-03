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

// Required so the wasm/no_std build can name `alloc::alloc::*` (in
// `wasm.rs`) and so `lex.rs` unit tests can use `alloc::vec!`/
// `alloc::string::String` symmetrically with the no_std path.
// `#[allow(unused_extern_crates)]` keeps `rust_2018_idioms` quiet when
// neither use site is active in a particular feature combination.
#[allow(unused_extern_crates)]
extern crate alloc;

pub mod error;
pub mod lex;
pub mod parse;

#[cfg(feature = "pretty")]
mod pretty;

#[cfg(feature = "validate")]
mod validate;
#[cfg(feature = "validate")]
pub use validate::{validate_operations, ValidationError};

#[cfg(feature = "napi")]
mod napi;

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
///
/// As of R3 the AST is bumpalo-arena-allocated; the caller owns the arena
/// and the returned [`Document`] borrows from it. The arena is freed when
/// the caller drops it (O(1) AST teardown). See
/// `docs/investigation-r2-wasm-size.md` for the rationale (single
/// monomorphization across the seven list types in the AST).
#[cfg(feature = "ops")]
pub fn parse_executable_document<'src, 'bump>(
    arena: &'bump bumpalo::Bump,
    src: &'src str,
) -> Result<Document<'src, 'bump>, ParseError> {
    parse::parse_executable_document(arena, src)
}

// Re-export the arena allocator so callers do not need to take a direct
// `bumpalo` dependency.
#[cfg(feature = "ops")]
pub use bumpalo::Bump;
