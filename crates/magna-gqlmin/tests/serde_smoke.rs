// SPDX-License-Identifier: MIT OR Apache-2.0
//! Smoke test for the `serde` feature.
//!
//! R10 swapped the R4 placeholder for real `Serialize`/`Deserialize`
//! derives on the AST. This test confirms:
//!
//! 1. The crate compiles with `--features serde`.
//! 2. A parsed `Document` round-trips through `serde_json::to_string`
//!    producing non-empty JSON containing the expected substrings.
//! 3. The chosen sum-type tagging form (`#[serde(tag = "kind")]`) emits
//!    a stable, self-describing JSON shape.

#![cfg(feature = "serde")]
// The AST has a recursive `Box<Type>` (List/NonNull) plus several
// `serde(tag = "kind")` sum types; resolving the `Serialize` impl
// chain through `serde_json::Serializer` exceeds the default 128-step
// recursion limit.
#![recursion_limit = "2048"]

#[test]
fn serde_feature_compiles_and_imports_resolve() {
    // Sanity check on public re-exports under the `serde` feature.
    let _ = magna_gqlmin::Span { start: 0, end: 0 };
    let _ = magna_gqlmin::ParseErrorKind::UnexpectedEof;
}

#[test]
fn document_serializes_to_non_empty_json() {
    let src = "query Hello($x: Int!) @cached { user(id: $x) { id name } }";
    let doc = magna_gqlmin::parse_executable_document(src).expect("parse OK");
    let json = serde_json::to_string(&doc).expect("serialize OK");
    assert!(!json.is_empty(), "json must not be empty");
    // The kind-tag form produces structural markers we can grep for.
    assert!(json.contains("\"kind\":\"Operation\""), "expected Definition::Operation tag in JSON: {json}");
    assert!(json.contains("\"kind\":\"Field\""), "expected Selection::Field tag in JSON: {json}");
    assert!(json.contains("Hello"), "expected operation name in JSON: {json}");
    assert!(json.contains("user"), "expected field name in JSON: {json}");
    assert!(json.contains("\"definitions_range\""), "expected document shape: {json}");
}

#[test]
fn fragment_definition_serializes() {
    let src = "fragment F on User { id } { ...F }";
    let doc = magna_gqlmin::parse_executable_document(src).expect("parse OK");
    let json = serde_json::to_string(&doc).expect("serialize OK");
    assert!(json.contains("\"kind\":\"Fragment\""), "expected Definition::Fragment tag: {json}");
    assert!(json.contains("\"kind\":\"FragmentSpread\""), "expected FragmentSpread tag: {json}");
}

#[test]
fn block_string_value_serializes() {
    let src = "{ a(arg: \"\"\"hello\"\"\") }";
    let doc = magna_gqlmin::parse_executable_document(src).expect("parse OK");
    let json = serde_json::to_string(&doc).expect("serialize OK");
    assert!(json.contains("\"kind\":\"String\""), "expected Value::String tag: {json}");
    assert!(json.contains("\"block\":true"), "expected block flag preserved: {json}");
}
