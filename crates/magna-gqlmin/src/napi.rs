// SPDX-License-Identifier: MIT OR Apache-2.0
//! napi-rs binding for the magna-gqlmin operations parser.
//!
//! Exposes `parseExecutableDocument(src: string)` to Node/Bun.
//!
//! ## R10 status (real implementation)
//!
//! The R4 placeholder is replaced with a real call to
//! [`crate::parse_executable_document`]. The success-path JSON envelope
//! includes the parsed `Document` serialized via the AST's `serde::Serialize`
//! derives (added in R10 alongside this module). Error paths emit a stable
//! `{ ok: false, error: { kind, span } }` shape.

use napi_derive::napi;

/// Parse a GraphQL executable document from a JS-side string.
///
/// Returns a JSON-encoded envelope of one of two shapes:
///
/// ```json
/// { "ok": true, "document": { ... } }                              // success
/// { "ok": false, "error": { "kind": <u8>, "span": [start, end] } } // failure
/// ```
///
/// The success-path `document` field carries the AST as serialized by the
/// `serde::Serialize` derives on `Document<'src>`. Sum types use
/// `#[serde(tag = "kind")]` so each variant is self-describing.
#[napi(js_name = "parseExecutableDocument")]
pub fn parse_executable_document(src: String) -> napi::Result<String> {
    match crate::parse_executable_document(&src) {
        Ok(doc) => {
            // Build the success envelope. `serde_json::to_value(&doc)`
            // converts the borrowed AST into an owned `Value` so the
            // outer envelope can be assembled without lifetime threading
            // through `serde_json::json!`.
            let document = serde_json::to_value(&doc)
                .map_err(|e| napi::Error::from_reason(e.to_string()))?;
            let envelope = serde_json::json!({
                "ok": true,
                "document": document,
            });
            serde_json::to_string(&envelope)
                .map_err(|e| napi::Error::from_reason(e.to_string()))
        }
        Err(err) => {
            let envelope = serde_json::json!({
                "ok": false,
                "error": {
                    "kind": err.kind as u8,
                    "span": [err.span.start, err.span.end],
                }
            });
            serde_json::to_string(&envelope)
                .map_err(|e| napi::Error::from_reason(e.to_string()))
        }
    }
}
