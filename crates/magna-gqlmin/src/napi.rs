// SPDX-License-Identifier: MIT OR Apache-2.0
//! napi-rs binding for the magna-gqlmin operations parser.
//!
//! Exposes `parseExecutableDocument(src: string)` to Node/Bun.
//!
//! ## R4 status (placeholder)
//!
//! The function currently returns a stable JSON envelope WITHOUT the parsed
//! AST. The real implementation requires:
//!
//! 1. `serde::Serialize` derives on the AST types in `parse/mod.rs` — these
//!    are deferred until R3's bumpalo migration lands and the AST shape is
//!    stable. Adding derives now would conflict with R3's concurrent edits.
//! 2. A `bumpalo::Bump` constructed per-call so the parsed `Document` lives
//!    long enough to be serialized. Trivially added once #1 lands.
//!
//! The function signature, JSON envelope shape, and feature wiring are
//! locked here so the post-R3 round only needs to swap the placeholder body.

use napi_derive::napi;

/// Parse a GraphQL executable document from a JS-side string.
///
/// Returns a JSON-encoded envelope of one of two shapes:
///
/// ```json
/// { "ok": true, "ast": { ... } }                                  // success
/// { "ok": false, "error": { "kind": <u8>, "span": [start, end] } } // failure
/// ```
///
/// In R4 this is a placeholder that always returns a stable
/// `ok=false, note="..."` envelope so JS callers can integrate against the
/// final shape today and pick up real parsing automatically once the
/// post-R3 round lands.
#[napi(js_name = "parseExecutableDocument")]
pub fn parse_executable_document(src: String) -> napi::Result<String> {
    // Touch `src` so the unused-variable lint stays quiet without `#[allow]`.
    let _ = src;
    // Hand-rolled JSON literal — no allocation beyond the single `String`,
    // and no risk of accidentally returning an invalid shape.
    Ok(concat!(
        r#"{"ok":false,"error":{"kind":0,"span":[0,0],"#,
        r#""note":"napi binding scaffolded; awaiting post-R3 AST integration"}}"#,
    )
    .to_string())
}
