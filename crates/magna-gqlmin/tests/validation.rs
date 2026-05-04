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

// ========================================================================
// Schema-aware validation rules (R11, step 9 completion)
// ========================================================================

#[cfg(feature = "sdl")]
mod schema_aware {
    use magna_gqlmin::{parse_executable_document, parse_schema, validate};

    const SCHEMA: &str = "
        type Query {
            user(id: ID!): User
            search(filter: Filter): [User!]
            count: Int
        }
        type User {
            id: ID!
            name: String
            age: Int
            best_friend: User
        }
        input Filter {
            name: String
            min_age: Int
        }
        enum Color {
            RED
            GREEN
            BLUE
        }
    ";

    fn rules_against(op: &str, schema_src: &str) -> Vec<&'static str> {
        let doc = parse_executable_document(op).expect("op fixture parses");
        let schema = parse_schema(schema_src).expect("schema fixture parses");
        validate(&doc, &schema).into_iter().map(|e| e.rule).collect()
    }

    fn rules(op: &str) -> Vec<&'static str> {
        rules_against(op, SCHEMA)
    }

    // --- KnownTypeNames ------------------------------------------------

    #[test]
    fn known_type_names_passes_for_declared_type() {
        let src = "query Q($id: ID!) { user(id: $id) { name } }";
        let r = rules(src);
        assert!(
            !r.contains(&"KnownTypeNames"),
            "expected no KnownTypeNames errors, got {r:?}"
        );
    }

    #[test]
    fn known_type_names_fails_for_undeclared_type() {
        let src = "query Q($id: NotAType!) { user(id: \"x\") { name } }";
        let r = rules(src);
        assert!(
            r.contains(&"KnownTypeNames"),
            "expected KnownTypeNames error, got {r:?}"
        );
    }

    // --- FieldsOnCorrectType -------------------------------------------

    #[test]
    fn fields_on_correct_type_passes_for_declared_field() {
        let src = "query Q { user(id: \"x\") { name age } }";
        let r = rules(src);
        assert!(
            !r.contains(&"FieldsOnCorrectType"),
            "expected no FieldsOnCorrectType errors, got {r:?}"
        );
    }

    #[test]
    fn fields_on_correct_type_fails_for_undeclared_field() {
        let src = "query Q { user(id: \"x\") { ssn } }";
        let r = rules(src);
        assert!(
            r.contains(&"FieldsOnCorrectType"),
            "expected FieldsOnCorrectType error, got {r:?}"
        );
    }

    // --- ScalarLeafs ---------------------------------------------------

    #[test]
    fn scalar_leafs_passes_for_correct_shape() {
        let src = "query Q { user(id: \"x\") { name } count }";
        let r = rules(src);
        assert!(
            !r.contains(&"ScalarLeafs"),
            "expected no ScalarLeafs errors, got {r:?}"
        );
    }

    #[test]
    fn scalar_leafs_fails_when_leaf_has_selection() {
        let src = "query Q { user(id: \"x\") { name { extra } } }";
        let r = rules(src);
        assert!(
            r.contains(&"ScalarLeafs"),
            "expected ScalarLeafs error, got {r:?}"
        );
    }

    // --- KnownArgumentNames --------------------------------------------

    #[test]
    fn known_argument_names_passes_for_declared_arg() {
        let src = "query Q { user(id: \"x\") { name } }";
        let r = rules(src);
        assert!(
            !r.contains(&"KnownArgumentNames"),
            "expected no KnownArgumentNames errors, got {r:?}"
        );
    }

    #[test]
    fn known_argument_names_fails_for_undeclared_arg() {
        let src = "query Q { user(id: \"x\", who: \"y\") { name } }";
        let r = rules(src);
        assert!(
            r.contains(&"KnownArgumentNames"),
            "expected KnownArgumentNames error, got {r:?}"
        );
    }

    // --- ArgumentsOfCorrectType ----------------------------------------

    #[test]
    fn arguments_of_correct_type_passes_for_matching_kinds() {
        let src = "query Q { user(id: \"x\") { name } }";
        let r = rules(src);
        assert!(
            !r.contains(&"ArgumentsOfCorrectType"),
            "expected no ArgumentsOfCorrectType errors, got {r:?}"
        );
    }

    #[test]
    fn arguments_of_correct_type_fails_for_mismatched_kind() {
        // user(id: ID!) — Boolean literal does not match ID
        let src = "query Q { user(id: true) { name } }";
        let r = rules(src);
        assert!(
            r.contains(&"ArgumentsOfCorrectType"),
            "expected ArgumentsOfCorrectType error, got {r:?}"
        );
    }
}
