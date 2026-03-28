//! Register GraphQL mutation input/payload types and mutation fields.
//!
//! For each resource + behavior combination:
//! - INSERT → CreateXInput, CreateXPayload, createX mutation
//! - UPDATE → XPatch, UpdateXInput, UpdateXPayload, updateX mutation
//! - DELETE → DeleteXInput, DeleteXPayload, deleteX mutation

use async_graphql::dynamic::{
    Field, FieldFuture, InputObject, InputValue, Object, SchemaBuilder, TypeRef,
};
use async_graphql::Value;

use crate::ir::{BehaviorSet, ResolvedResource};
use crate::naming::{
    create_input_type_name, create_mutation_field_name, create_payload_type_name,
    delete_input_type_name, delete_mutation_field_name, delete_payload_type_name,
    patch_type_name, update_input_type_name, update_mutation_field_name,
    update_payload_type_name,
};

// ── Input / Payload registration ──────────────────────────────────────────────

/// Register mutation input and payload types for a resource based on its
/// enabled behaviors. Also returns the input/payload type names for reuse.
pub fn register_mutation_types(
    mut builder: SchemaBuilder,
    resource: &ResolvedResource,
    behaviors: &BehaviorSet,
) -> SchemaBuilder {
    let type_name = &resource.name;

    // ── INSERT ────────────────────────────────────────────────────────────────
    if behaviors.has(BehaviorSet::INSERT) {
        // CreateXInput: non-default, non-null columns are required; rest optional
        let mut create_input = InputObject::new(create_input_type_name(type_name));
        for col in &resource.columns {
            let field_type = if col.is_not_null && !col.has_default {
                TypeRef::named_nn(&col.gql_type)
            } else {
                TypeRef::named(&col.gql_type)
            };
            create_input = create_input.field(InputValue::new(&col.gql_name, field_type));
        }
        builder = builder.register(create_input);

        // CreateXPayload: x field + query stub
        let node_type = type_name.clone();
        let payload = Object::new(create_payload_type_name(type_name))
            .field(Field::new(
                type_name_to_field(type_name),
                TypeRef::named(&node_type),
                |_| FieldFuture::from_value(Some(Value::Null)),
            ))
            .field(Field::new(
                "query",
                TypeRef::named("Query"),
                |_| FieldFuture::from_value(Some(Value::Null)),
            ));
        builder = builder.register(payload);
    }

    // ── UPDATE ────────────────────────────────────────────────────────────────
    if behaviors.has(BehaviorSet::UPDATE) {
        // XPatch: all columns optional
        let mut patch = InputObject::new(patch_type_name(type_name));
        for col in &resource.columns {
            patch = patch.field(InputValue::new(&col.gql_name, TypeRef::named(&col.gql_type)));
        }
        builder = builder.register(patch);

        // UpdateXInput: nodeId (required) + patch (required)
        let update_input = InputObject::new(update_input_type_name(type_name))
            .field(InputValue::new("nodeId", TypeRef::named_nn(TypeRef::ID)))
            .field(InputValue::new(
                "patch",
                TypeRef::named_nn(patch_type_name(type_name)),
            ));
        builder = builder.register(update_input);

        // UpdateXPayload: x field + query stub
        let node_type = type_name.clone();
        let payload = Object::new(update_payload_type_name(type_name))
            .field(Field::new(
                type_name_to_field(type_name),
                TypeRef::named(&node_type),
                |_| FieldFuture::from_value(Some(Value::Null)),
            ))
            .field(Field::new(
                "query",
                TypeRef::named("Query"),
                |_| FieldFuture::from_value(Some(Value::Null)),
            ));
        builder = builder.register(payload);
    }

    // ── DELETE ────────────────────────────────────────────────────────────────
    if behaviors.has(BehaviorSet::DELETE) {
        // DeleteXInput: nodeId (required)
        let delete_input = InputObject::new(delete_input_type_name(type_name))
            .field(InputValue::new("nodeId", TypeRef::named_nn(TypeRef::ID)));
        builder = builder.register(delete_input);

        // DeleteXPayload: x field + deletedNodeId + query
        let node_type = type_name.clone();
        let payload = Object::new(delete_payload_type_name(type_name))
            .field(Field::new(
                type_name_to_field(type_name),
                TypeRef::named(&node_type),
                |_| FieldFuture::from_value(Some(Value::Null)),
            ))
            .field(Field::new(
                "deletedNodeId",
                TypeRef::named(TypeRef::ID),
                |_| FieldFuture::from_value(Some(Value::Null)),
            ))
            .field(Field::new(
                "query",
                TypeRef::named("Query"),
                |_| FieldFuture::from_value(Some(Value::Null)),
            ));
        builder = builder.register(payload);
    }

    builder
}

/// Add mutation fields (createX, updateX, deleteX) to a Mutation object.
pub fn add_mutation_fields(
    mut mutation: Object,
    resource: &ResolvedResource,
    behaviors: &BehaviorSet,
) -> Object {
    let type_name = &resource.name;

    if behaviors.has(BehaviorSet::INSERT) {
        let field_name = create_mutation_field_name(type_name);
        let input_type = create_input_type_name(type_name);
        let payload_type = create_payload_type_name(type_name);

        mutation = mutation.field(
            Field::new(field_name, TypeRef::named(&payload_type), |_| {
                FieldFuture::from_value(Some(Value::Null))
            })
            .argument(InputValue::new(
                "input",
                TypeRef::named_nn(input_type),
            )),
        );
    }

    if behaviors.has(BehaviorSet::UPDATE) {
        let field_name = update_mutation_field_name(type_name);
        let input_type = update_input_type_name(type_name);
        let payload_type = update_payload_type_name(type_name);

        mutation = mutation.field(
            Field::new(field_name, TypeRef::named(&payload_type), |_| {
                FieldFuture::from_value(Some(Value::Null))
            })
            .argument(InputValue::new(
                "input",
                TypeRef::named_nn(input_type),
            )),
        );
    }

    if behaviors.has(BehaviorSet::DELETE) {
        let field_name = delete_mutation_field_name(type_name);
        let input_type = delete_input_type_name(type_name);
        let payload_type = delete_payload_type_name(type_name);

        mutation = mutation.field(
            Field::new(field_name, TypeRef::named(&payload_type), |_| {
                FieldFuture::from_value(Some(Value::Null))
            })
            .argument(InputValue::new(
                "input",
                TypeRef::named_nn(input_type),
            )),
        );
    }

    mutation
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Convert a PascalCase type name to a camelCase field name.
/// "User" → "user", "UserProfile" → "userProfile"
fn type_name_to_field(type_name: &str) -> String {
    let mut chars = type_name.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_lowercase().to_string() + chars.as_str(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::dynamic::{Schema, TypeRef};
    use crate::ir::{BehaviorSet, ResolvedColumn, ResourceKind};
    use crate::register::scalars::register_scalars;

    fn make_user_resource() -> ResolvedResource {
        ResolvedResource {
            name: "User".to_string(),
            schema: "public".to_string(),
            table: "users".to_string(),
            kind: ResourceKind::Table,
            columns: vec![
                ResolvedColumn {
                    pg_name: "id".to_string(),
                    gql_name: "id".to_string(),
                    type_oid: 20,
                    gql_type: TypeRef::INT.to_string(),
                    is_not_null: true,
                    has_default: true,
                },
                ResolvedColumn {
                    pg_name: "email".to_string(),
                    gql_name: "email".to_string(),
                    type_oid: 25,
                    gql_type: TypeRef::STRING.to_string(),
                    is_not_null: true,
                    has_default: false,
                },
            ],
            primary_key: vec!["id".to_string()],
            unique_constraints: vec![],
            class_oid: 10,
        }
    }

    fn build_full_mutation_schema(resource: &ResolvedResource, behaviors: BehaviorSet) -> Result<async_graphql::dynamic::Schema, async_graphql::dynamic::SchemaError> {
        let query = Object::new("Query").field(Field::new(
            "placeholder",
            TypeRef::named(TypeRef::STRING),
            |_| FieldFuture::from_value(Some(Value::Null)),
        ));

        let mut mutation = Object::new("Mutation");
        mutation = add_mutation_fields(mutation, resource, &behaviors);

        let mut builder = Schema::build("Query", Some("Mutation"), None);
        builder = register_scalars(builder);

        // Register object type for payload references
        use crate::register::object_types::register_object_types;
        builder = register_object_types(builder, std::slice::from_ref(resource));
        builder = register_mutation_types(builder, resource, &behaviors);

        builder = builder.register(query);
        builder = builder.register(mutation);
        builder.finish()
    }

    /// All mutation types should build for table defaults.
    #[test]
    fn test_register_all_mutation_types() {
        let resource = make_user_resource();
        let behaviors = BehaviorSet::table_defaults();
        let result = build_full_mutation_schema(&resource, behaviors);
        assert!(result.is_ok(), "all mutation types should build: {:?}", result.err());
    }

    /// Read-only behaviors should produce no mutation fields.
    #[test]
    fn test_read_only_behaviors_skip_mutations() {
        let resource = make_user_resource();
        let behaviors = BehaviorSet::view_defaults(); // no INSERT/UPDATE/DELETE

        let query = Object::new("Query").field(Field::new(
            "placeholder",
            TypeRef::named(TypeRef::STRING),
            |_| FieldFuture::from_value(Some(Value::Null)),
        ));

        // Mutation object with no fields is not valid — skip it for read-only
        let mut builder = Schema::build("Query", None, None);
        builder = register_mutation_types(builder, &resource, &behaviors);
        builder = builder.register(query);

        let schema = builder.finish();
        assert!(schema.is_ok(), "read-only schema should build: {:?}", schema.err());
    }

    /// INSERT-only behaviors should build create types.
    #[test]
    fn test_insert_only_behaviors() {
        let resource = make_user_resource();
        let mut behaviors = BehaviorSet::none();
        behaviors.add(BehaviorSet::INSERT);

        let result = build_full_mutation_schema(&resource, behaviors);
        assert!(result.is_ok(), "insert-only mutation schema should build: {:?}", result.err());
    }

    /// type_name_to_field converts correctly.
    #[test]
    fn test_type_name_to_field() {
        assert_eq!(type_name_to_field("User"), "user");
        assert_eq!(type_name_to_field("UserProfile"), "userProfile");
        assert_eq!(type_name_to_field(""), "");
    }
}
