// SPDX-License-Identifier: MIT OR Apache-2.0
//! SDL parser corpus (R11, step 8).
//!
//! Eight cases covering the GraphQL Oct-2021 spec § 3 type system
//! definitions implemented this round. Each test parses successfully and
//! asserts ≥2 structural properties.

#![cfg(feature = "sdl")]

use magna_gqlmin::{parse_schema, Type, TypeSystemDefinition};

const SCHEMA_DEFINITION: &str = include_str!("sdl-corpus/schema_definition.graphql");
const SCALAR_DEF: &str = include_str!("sdl-corpus/scalar_def.graphql");
const OBJECT_WITH_FIELDS: &str = include_str!("sdl-corpus/object_with_fields.graphql");
const INTERFACE_DEF: &str = include_str!("sdl-corpus/interface_def.graphql");
const OBJECT_IMPLEMENTS: &str = include_str!("sdl-corpus/object_implements.graphql");
const UNION_DEF: &str = include_str!("sdl-corpus/union_def.graphql");
const ENUM_DEF: &str = include_str!("sdl-corpus/enum_def.graphql");
const INPUT_DEF: &str = include_str!("sdl-corpus/input_def.graphql");

#[test]
fn schema_definition() {
    let doc = parse_schema(SCHEMA_DEFINITION).expect("schema definition parses");
    assert_eq!(doc.definitions.len(), 1);
    let TypeSystemDefinition::Schema(s) = &doc.definitions[0] else {
        panic!("expected Schema, got {:?}", doc.definitions[0]);
    };
    assert_eq!(s.operation_types.len(), 2);
    assert_eq!(s.operation_types[0].operation, "query");
    assert_eq!(s.operation_types[0].named_type.name.value, "Query");
    assert_eq!(s.operation_types[1].operation, "mutation");
    assert_eq!(s.operation_types[1].named_type.name.value, "Mutation");
}

#[test]
fn scalar_def() {
    let doc = parse_schema(SCALAR_DEF).expect("scalar def parses");
    assert_eq!(doc.definitions.len(), 1);
    let TypeSystemDefinition::Scalar(s) = &doc.definitions[0] else {
        panic!("expected Scalar, got {:?}", doc.definitions[0]);
    };
    assert_eq!(s.name.value, "DateTime");
    assert!(s.directives.is_empty());
}

#[test]
fn object_with_fields() {
    let doc = parse_schema(OBJECT_WITH_FIELDS).expect("object def parses");
    assert_eq!(doc.definitions.len(), 1);
    let TypeSystemDefinition::Object(o) = &doc.definitions[0] else {
        panic!("expected Object, got {:?}", doc.definitions[0]);
    };
    assert_eq!(o.name.value, "User");
    assert_eq!(o.fields.len(), 3);
    assert_eq!(o.fields[0].name.value, "id");
    // id: ID! → NonNull(Named(ID))
    assert!(matches!(o.fields[0].field_type, Type::NonNull(_)));
    assert_eq!(o.fields[1].name.value, "name");
    assert!(matches!(o.fields[1].field_type, Type::Named(_)));
    assert_eq!(o.fields[2].name.value, "age");
}

#[test]
fn interface_def() {
    let doc = parse_schema(INTERFACE_DEF).expect("interface def parses");
    assert_eq!(doc.definitions.len(), 1);
    let TypeSystemDefinition::Interface(i) = &doc.definitions[0] else {
        panic!("expected Interface, got {:?}", doc.definitions[0]);
    };
    assert_eq!(i.name.value, "Node");
    assert_eq!(i.fields.len(), 1);
    assert_eq!(i.fields[0].name.value, "id");
}

#[test]
fn object_implements() {
    let doc = parse_schema(OBJECT_IMPLEMENTS).expect("object implements parses");
    let TypeSystemDefinition::Object(o) = &doc.definitions[0] else {
        panic!("expected Object");
    };
    assert_eq!(o.name.value, "User");
    assert_eq!(o.implements.len(), 1);
    assert_eq!(o.implements[0].name.value, "Node");
    assert_eq!(o.fields.len(), 1);
}

#[test]
fn union_def() {
    let doc = parse_schema(UNION_DEF).expect("union def parses");
    let TypeSystemDefinition::Union(u) = &doc.definitions[0] else {
        panic!("expected Union");
    };
    assert_eq!(u.name.value, "Result");
    assert_eq!(u.members.len(), 3);
    let member_names: Vec<&str> = u.members.iter().map(|m| m.name.value).collect();
    assert_eq!(member_names, ["A", "B", "C"]);
}

#[test]
fn enum_def() {
    let doc = parse_schema(ENUM_DEF).expect("enum def parses");
    let TypeSystemDefinition::Enum(e) = &doc.definitions[0] else {
        panic!("expected Enum");
    };
    assert_eq!(e.name.value, "Color");
    assert_eq!(e.values.len(), 3);
    let value_names: Vec<&str> = e.values.iter().map(|v| v.name.value).collect();
    assert_eq!(value_names, ["RED", "GREEN", "BLUE"]);
}

#[test]
fn input_def() {
    let doc = parse_schema(INPUT_DEF).expect("input def parses");
    let TypeSystemDefinition::InputObject(i) = &doc.definitions[0] else {
        panic!("expected InputObject");
    };
    assert_eq!(i.name.value, "UserInput");
    assert_eq!(i.fields.len(), 2);
    assert_eq!(i.fields[0].name.value, "name");
    assert!(matches!(i.fields[0].value_type, Type::NonNull(_)));
    assert_eq!(i.fields[1].name.value, "age");
    // age has default value `0`
    assert!(i.fields[1].default_value.is_some());
}
