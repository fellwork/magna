// SPDX-License-Identifier: MIT OR Apache-2.0
//! Validation rule tests (R4, step 9 partial).
//!
//! Two cases per rule (one passing, one failing) — 10 cases total. Failing
//! cases assert on the rule string only; the message/span are exercised
//! implicitly by being non-empty.

#![cfg(feature = "validate")]

use magna_gqlmin::{parse_executable_document, validate_operations};

// Inline `parse + validate` so the `Document` borrow stays alive across
// the `validate_operations` call.
fn rules(src: &str) -> Vec<&'static str> {
    let doc = parse_executable_document(src).unwrap_or_else(|e| {
        panic!("test fixture failed to parse: {:?} in:\n{}", e, src)
    });
    validate_operations(&doc).into_iter().map(|e| e.rule).collect()
}

// --- NoUndefinedVariables ----------------------------------------------

#[test]
fn no_undefined_variables_passes_when_var_declared() {
    let src = "query Q($id: ID!) { user(id: $id) { name } }";
    let r = rules(src);
    assert!(
        !r.contains(&"NoUndefinedVariables"),
        "expected no NoUndefinedVariables errors, got {r:?}"
    );
}

#[test]
fn no_undefined_variables_fails_when_var_missing() {
    let src = "query Q { user(id: $id) { name } }";
    let r = rules(src);
    assert!(
        r.contains(&"NoUndefinedVariables"),
        "expected NoUndefinedVariables error, got {r:?}"
    );
}

// --- NoUnusedVariables -------------------------------------------------

#[test]
fn no_unused_variables_passes_when_var_used() {
    let src = "query Q($id: ID!) { user(id: $id) { name } }";
    let r = rules(src);
    assert!(
        !r.contains(&"NoUnusedVariables"),
        "expected no NoUnusedVariables errors, got {r:?}"
    );
}

#[test]
fn no_unused_variables_fails_when_var_declared_but_unused() {
    let src = "query Q($id: ID!, $extra: String) { user(id: $id) { name } }";
    let r = rules(src);
    assert!(
        r.contains(&"NoUnusedVariables"),
        "expected NoUnusedVariables error, got {r:?}"
    );
}

// --- NoUnusedFragments -------------------------------------------------

#[test]
fn no_unused_fragments_passes_when_fragment_spread() {
    let src = "
        query Q { user { ...UserFields } }
        fragment UserFields on User { id name }
    ";
    let r = rules(src);
    assert!(
        !r.contains(&"NoUnusedFragments"),
        "expected no NoUnusedFragments errors, got {r:?}"
    );
}

#[test]
fn no_unused_fragments_fails_when_fragment_orphan() {
    let src = "
        query Q { user { id } }
        fragment Orphan on User { name }
    ";
    let r = rules(src);
    assert!(
        r.contains(&"NoUnusedFragments"),
        "expected NoUnusedFragments error, got {r:?}"
    );
}

// --- KnownFragmentNames ------------------------------------------------

#[test]
fn known_fragment_names_passes_when_target_defined() {
    let src = "
        query Q { user { ...F } }
        fragment F on User { id }
    ";
    let r = rules(src);
    assert!(
        !r.contains(&"KnownFragmentNames"),
        "expected no KnownFragmentNames errors, got {r:?}"
    );
}

#[test]
fn known_fragment_names_fails_for_undefined_spread() {
    let src = "query Q { user { ...DoesNotExist } }";
    let r = rules(src);
    assert!(
        r.contains(&"KnownFragmentNames"),
        "expected KnownFragmentNames error, got {r:?}"
    );
}

// --- UniqueOperationNames ----------------------------------------------

#[test]
fn unique_operation_names_passes_for_distinct_named_ops() {
    let src = "query A { x } query B { y }";
    let r = rules(src);
    assert!(
        !r.contains(&"UniqueOperationNames"),
        "expected no UniqueOperationNames errors, got {r:?}"
    );
}

#[test]
fn unique_operation_names_fails_for_duplicate_names() {
    let src = "query Dup { a } query Dup { b }";
    let r = rules(src);
    assert!(
        r.contains(&"UniqueOperationNames"),
        "expected UniqueOperationNames error, got {r:?}"
    );
}

// --- Bonus: anonymous-operation-with-others is also a UniqueOperationNames violation.

#[test]
fn anonymous_operation_with_others_flags_unique_names() {
    let src = "{ a } query Named { b }";
    let r = rules(src);
    assert!(
        r.contains(&"UniqueOperationNames"),
        "expected UniqueOperationNames error for anonymous-with-others, got {r:?}"
    );
}

#[test]
fn solitary_anonymous_query_passes() {
    let src = "{ a }";
    let r = rules(src);
    assert!(
        !r.contains(&"UniqueOperationNames"),
        "expected no UniqueOperationNames errors for sole anonymous op, got {r:?}"
    );
}
