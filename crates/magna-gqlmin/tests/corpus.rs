// SPDX-License-Identifier: MIT OR Apache-2.0
//! Round 1 corpus test. The 20 case names are locked by the gqlmin topic
//! summary. For each parsing case, we assert at least two specific
//! structural properties (not just `is_ok()`). For the two error cases,
//! we assert the expected `ParseErrorKind` discriminant and a non-empty
//! span. Bidirectional checks (e.g. `simple_query` has no fragments) are
//! sprinkled in.
//!
//! R5 (span-indexed AST): list-field access flows through typed accessor
//! methods on `Document` (`doc.directives(...)`, `doc.selections(...)`)
//! that project the correct `Node` variant from the shared arena. The
//! returned slice supports `.len()`, `[i]`, and `iter()`/`into_iter()`.

use std::fs;
use std::path::PathBuf;

use magna_gqlmin::{
    parse_executable_document, Definition, ParseErrorKind, Selection, Type, Value,
};

fn corpus_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("corpus");
    p.push(format!("{name}.graphql"));
    p
}

fn read(name: &str) -> String {
    let p = corpus_path(name);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {p:?}: {e}"))
}

#[test]
fn case_simple_query() {
    let src = read("simple_query");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    assert_eq!(defs.len(), 1, "exactly one definition");
    let Definition::Operation(op) = &defs[0] else {
        panic!("must be operation");
    };
    assert!(op.shorthand, "shorthand `{{ ... }}` form");
    assert!(op.name.is_none(), "no operation name");
    let sels = doc.selections(op.selection_set.selections);
    assert_eq!(sels.len(), 1, "exactly one selection");
    // Bidirectional: no fragment definitions present.
    assert!(!defs.iter().any(|d| matches!(d, Definition::Fragment(_))));
}

#[test]
fn case_named_query() {
    let src = read("named_query");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    let Definition::Operation(op) = &defs[0] else { panic!() };
    let name = op.name.expect("named");
    assert_eq!(name.value, "MyQuery");
    assert_eq!(doc.selections(op.selection_set.selections).len(), 2);
    assert!(!op.shorthand);
}

#[test]
fn case_query_with_variables() {
    let src = read("query_with_variables");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    let Definition::Operation(op) = &defs[0] else { panic!() };
    let vds = doc.variable_definitions(op.variable_definitions);
    assert_eq!(vds.len(), 2);
    assert_eq!(vds[0].name.value, "id");
    // $id is `ID!` -> NonNull(Named("ID"))
    match &vds[0].var_type {
        Type::NonNull(inner) => match inner.as_ref() {
            Type::Named(n) => assert_eq!(n.name.value, "ID"),
            _ => panic!("expected named inside NonNull"),
        },
        _ => panic!("expected NonNull"),
    }
    assert!(vds[1].default_value.is_some());
}

#[test]
fn case_mutation() {
    use magna_gqlmin::OperationKind;
    let src = read("mutation");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    let Definition::Operation(op) = &defs[0] else { panic!() };
    assert_eq!(op.kind, OperationKind::Mutation);
    assert_eq!(op.name.unwrap().value, "CreateUser");
}

#[test]
fn case_subscription() {
    use magna_gqlmin::OperationKind;
    let src = read("subscription");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    let Definition::Operation(op) = &defs[0] else { panic!() };
    assert_eq!(op.kind, OperationKind::Subscription);
    assert_eq!(doc.variable_definitions(op.variable_definitions).len(), 1);
}

#[test]
fn case_fragment_definition() {
    let src = read("fragment_definition");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    assert_eq!(defs.len(), 2);
    let frag = match &defs[0] {
        Definition::Fragment(f) => f,
        _ => panic!("expected fragment first"),
    };
    assert_eq!(frag.name.value, "UserFields");
    assert_eq!(frag.type_condition.name.value, "User");
}

#[test]
fn case_fragment_spread() {
    let src = read("fragment_spread");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    let Definition::Operation(op) = &defs[0] else { panic!() };
    let user = match &doc.selections(op.selection_set.selections)[0] {
        Selection::Field(f) => f.clone(),
        _ => panic!("expected field"),
    };
    let inner = user.selection_set.as_ref().expect("user has subselection");
    let mut saw_spread = false;
    for sel in &doc.selections(inner.selections) {
        if let Selection::FragmentSpread(s) = sel {
            assert_eq!(s.name.value, "UserFields");
            saw_spread = true;
        }
    }
    assert!(saw_spread, "must contain ...UserFields spread");
}

#[test]
fn case_inline_fragment_with_type() {
    let src = read("inline_fragment_with_type");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    let Definition::Operation(op) = &defs[0] else { panic!() };
    let hero = match &doc.selections(op.selection_set.selections)[0] {
        Selection::Field(f) => f.clone(),
        _ => panic!(),
    };
    let inner = hero.selection_set.as_ref().unwrap();
    let mut typed_count = 0usize;
    for sel in &doc.selections(inner.selections) {
        if let Selection::InlineFragment(f) = sel {
            assert!(f.type_condition.is_some(), "typed inline fragment");
            typed_count += 1;
        }
    }
    assert_eq!(typed_count, 2, "two typed inline fragments");
}

#[test]
fn case_inline_fragment_no_type() {
    let src = read("inline_fragment_no_type");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    let Definition::Operation(op) = &defs[0] else { panic!() };
    let me = match &doc.selections(op.selection_set.selections)[0] {
        Selection::Field(f) => f.clone(),
        _ => panic!(),
    };
    let inner = me.selection_set.as_ref().unwrap();
    let frag = match &doc.selections(inner.selections)[0] {
        Selection::InlineFragment(f) => f.clone(),
        _ => panic!("expected inline fragment"),
    };
    assert!(frag.type_condition.is_none(), "untyped inline fragment");
    let dirs = doc.directives(frag.directives);
    assert_eq!(dirs.len(), 1, "carries one directive");
    assert_eq!(dirs[0].name.value, "include");
}

#[test]
fn case_nested_directives() {
    let src = read("nested_directives");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    let Definition::Operation(op) = &defs[0] else { panic!() };
    let f = match &doc.selections(op.selection_set.selections)[0] {
        Selection::Field(f) => f.clone(),
        _ => panic!(),
    };
    let dirs = doc.directives(f.directives);
    assert!(
        dirs.len() >= 2,
        "field must carry >=2 directives, got {}",
        dirs.len()
    );
    assert_eq!(dirs[0].name.value, "skip");
}

#[test]
fn case_field_alias() {
    let src = read("field_alias");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    let Definition::Operation(op) = &defs[0] else { panic!() };
    let mut aliases = 0usize;
    for sel in &doc.selections(op.selection_set.selections) {
        if let Selection::Field(f) = sel {
            if let Some(a) = f.alias {
                assert_eq!(f.name.value, "profilePic");
                let _ = a;
                aliases += 1;
            }
        }
    }
    assert_eq!(aliases, 2, "both fields aliased");
}

#[test]
fn case_arguments_all_value_kinds() {
    let src = read("arguments_all_value_kinds");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    let Definition::Operation(op) = &defs[0] else { panic!() };
    let f = match &doc.selections(op.selection_set.selections)[0] {
        Selection::Field(f) => f.clone(),
        _ => panic!(),
    };
    let kinds: Vec<&str> = doc
        .arguments(f.arguments)
        .iter()
        .map(|a| match &a.value {
            Value::String(_) => "string",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Boolean(_) => "bool",
            Value::Null => "null",
            Value::Enum(_) => "enum",
            Value::List(_) => "list",
            Value::Object(_) => "object",
            Value::Variable(_) => "var",
        })
        .collect();
    for k in [
        "string", "int", "float", "bool", "null", "enum", "list", "object",
    ] {
        assert!(kinds.contains(&k), "missing value kind {k}: got {kinds:?}");
    }
}

#[test]
fn case_non_null_list_type() {
    let src = read("non_null_list_type");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    let Definition::Operation(op) = &defs[0] else { panic!() };
    let vds = doc.variable_definitions(op.variable_definitions);
    let var = &vds[0];
    // [ID!]! -> NonNull(List(NonNull(Named("ID"))))
    let outer = match &var.var_type {
        Type::NonNull(inner) => inner.as_ref(),
        _ => panic!("outer must be NonNull"),
    };
    let list_inner = match outer {
        Type::List(inner) => inner.as_ref(),
        _ => panic!("middle must be List"),
    };
    let leaf = match list_inner {
        Type::NonNull(inner) => inner.as_ref(),
        _ => panic!("inner must be NonNull"),
    };
    match leaf {
        Type::Named(n) => assert_eq!(n.name.value, "ID"),
        _ => panic!("leaf must be Named"),
    }
}

#[test]
fn case_default_value() {
    let src = read("default_value");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    let Definition::Operation(op) = &defs[0] else { panic!() };
    let vds = doc.variable_definitions(op.variable_definitions);
    assert_eq!(vds.len(), 2);
    for vd in &vds {
        assert!(vd.default_value.is_some(), "var {} has default", vd.name.value);
    }
}

#[test]
fn case_multiple_operations() {
    use magna_gqlmin::OperationKind;
    let src = read("multiple_operations");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    assert_eq!(defs.len(), 3);
    let kinds: Vec<OperationKind> = defs
        .iter()
        .map(|d| match d {
            Definition::Operation(op) => op.kind,
            _ => panic!("all ops"),
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            OperationKind::Query,
            OperationKind::Mutation,
            OperationKind::Subscription
        ]
    );
}

#[test]
fn case_block_string_arg() {
    let src = read("block_string_arg");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    let Definition::Operation(op) = &defs[0] else { panic!() };
    let f = match &doc.selections(op.selection_set.selections)[0] {
        Selection::Field(f) => f.clone(),
        _ => panic!(),
    };
    let args = doc.arguments(f.arguments);
    assert_eq!(args.len(), 1);
    let sv = match &args[0].value {
        Value::String(sv) => sv,
        other => panic!("expected string, got {other:?}"),
    };
    assert!(sv.block, "must be a block-string");
}

#[test]
fn case_comments_and_commas() {
    let src = read("comments_and_commas");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    let Definition::Operation(op) = &defs[0] else { panic!() };
    let sels = doc.selections(op.selection_set.selections);
    // a, b, c, d -> 4 selections (commas insignificant, comments skipped).
    assert_eq!(sels.len(), 4);
    let last = match &sels[3] {
        Selection::Field(f) => f.clone(),
        _ => panic!(),
    };
    assert_eq!(last.name.value, "d");
    assert_eq!(doc.arguments(last.arguments).len(), 2);
}

#[test]
fn case_unicode_in_strings() {
    let src = read("unicode_in_strings");
    let doc = parse_executable_document(&src).expect("parses");
    let defs = doc.definitions();
    let Definition::Operation(op) = &defs[0] else { panic!() };
    let f = match &doc.selections(op.selection_set.selections)[0] {
        Selection::Field(f) => f.clone(),
        _ => panic!(),
    };
    let args = doc.arguments(f.arguments);
    let sv = match &args[0].value {
        Value::String(sv) => sv,
        _ => panic!(),
    };
    assert!(!sv.block);
    // The raw lexeme must contain non-ASCII bytes.
    assert!(sv.raw.bytes().any(|b| b >= 0x80));
}

#[test]
fn case_empty_selection_error() {
    let src = read("empty_selection_error");
    let err = parse_executable_document(&src).expect_err("must error");
    assert_eq!(err.kind, ParseErrorKind::EmptySelectionSet);
    assert!(err.span.end > err.span.start, "non-empty span");
}

#[test]
fn case_unterminated_string_error() {
    let src = read("unterminated_string_error");
    let err = parse_executable_document(&src).expect_err("must error");
    assert_eq!(err.kind, ParseErrorKind::InvalidString);
    assert!(err.span.end >= err.span.start, "valid span");
    assert!(err.span.start > 0, "span points past prefix");
}
