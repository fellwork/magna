//! Build query root fields for resources.
//!
//! - CONNECTION behavior → allX(first, last, after, before, filter, orderBy, condition)
//!   returning XConnection
//! - SELECT_ONE behavior → xById(pk_cols...) returning X

use async_graphql::dynamic::{Field, FieldFuture, InputValue, TypeRef};
use async_graphql::Value;

use crate::ir::{BehaviorSet, ResolvedResource};
use crate::naming::{
    all_query_field_name, by_pk_query_field_name, condition_type_name, connection_type_name,
    filter_type_name, order_by_type_name,
};

/// Build query root fields for a resource based on its enabled behaviors.
///
/// Returns a list of `Field`s to be added to the Query root object.
pub fn build_query_fields(resource: &ResolvedResource, behaviors: &BehaviorSet) -> Vec<Field> {
    let mut fields: Vec<Field> = Vec::new();

    // ── CONNECTION: allX(...) → XConnection ───────────────────────────────────
    if behaviors.has(BehaviorSet::CONNECTION) {
        let field_name = all_query_field_name(&resource.name);
        let conn_type = connection_type_name(&resource.name);
        let filter_type = filter_type_name(&resource.name);
        let order_by_type = order_by_type_name(&resource.name);
        let condition_type = condition_type_name(&resource.name);

        let field = Field::new(field_name, TypeRef::named(&conn_type), |_| {
            FieldFuture::from_value(Some(Value::Null))
        })
        .argument(InputValue::new("first", TypeRef::named(TypeRef::INT)))
        .argument(InputValue::new("last", TypeRef::named(TypeRef::INT)))
        .argument(InputValue::new("after", TypeRef::named("Cursor")))
        .argument(InputValue::new("before", TypeRef::named("Cursor")))
        .argument(InputValue::new(
            "filter",
            TypeRef::named(filter_type),
        ))
        .argument(InputValue::new(
            "orderBy",
            TypeRef::named_list(order_by_type),
        ))
        .argument(InputValue::new(
            "condition",
            TypeRef::named(condition_type),
        ));

        fields.push(field);
    }

    // ── SELECT_ONE: xById(pk) → X ─────────────────────────────────────────────
    if behaviors.has(BehaviorSet::SELECT_ONE) {
        let field_name = by_pk_query_field_name(&resource.name);
        let obj_type = resource.name.clone();

        let mut field = Field::new(field_name, TypeRef::named(&obj_type), |_| {
            FieldFuture::from_value(Some(Value::Null))
        });

        // Add one argument per PK column
        for pk_col in &resource.primary_key {
            // Find the column's type
            let gql_type = resource
                .columns
                .iter()
                .find(|c| &c.pg_name == pk_col)
                .map(|c| c.gql_type.as_str())
                .unwrap_or(TypeRef::STRING);

            // Strip trailing "!" — nullability handled by named_nn
            let base_type = gql_type.trim_end_matches('!');
            let arg_name = crate::naming::to_camel_case(pk_col);
            field = field.argument(InputValue::new(arg_name, TypeRef::named_nn(base_type)));
        }

        fields.push(field);
    }

    fields
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::dynamic::{Field, FieldFuture, Object, Schema, TypeRef};
    use crate::ir::{BehaviorSet, ResolvedColumn, ResourceKind};
    use crate::register::conditions::register_condition_types;
    use crate::register::connections::{register_connection_types, register_page_info};
    use crate::register::filters::register_filter_types;
    use crate::register::object_types::register_object_types;
    use crate::register::order_by::register_order_by_types;
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
                    is_not_null: false,
                    has_default: false,
                },
            ],
            primary_key: vec!["id".to_string()],
            unique_constraints: vec![],
            class_oid: 10,
        }
    }

    fn build_query_schema(resource: &ResolvedResource, behaviors: BehaviorSet) -> Result<async_graphql::dynamic::Schema, async_graphql::dynamic::SchemaError> {
        let query_fields = build_query_fields(resource, &behaviors);

        let mut query = Object::new("Query").field(Field::new(
            "placeholder",
            TypeRef::named(TypeRef::STRING),
            |_| FieldFuture::from_value(Some(async_graphql::Value::Null)),
        ));

        for f in query_fields {
            query = query.field(f);
        }

        let resources = std::slice::from_ref(resource);
        let mut builder = Schema::build("Query", None, None);
        builder = register_scalars(builder);
        builder = register_page_info(builder);
        builder = register_object_types(builder, resources);
        builder = register_connection_types(builder, resource);
        builder = register_filter_types(builder, resources);
        builder = register_order_by_types(builder, resources);
        builder = register_condition_types(builder, resources);
        builder = builder.register(query);
        builder.finish()
    }

    /// Full table defaults (connection + select_one) should build.
    #[test]
    fn test_build_query_fields_table_defaults() {
        let resource = make_user_resource();
        let behaviors = BehaviorSet::table_defaults();
        let result = build_query_schema(&resource, behaviors);
        assert!(result.is_ok(), "query fields schema should build: {:?}", result.err());
    }

    /// Connection-only behaviors should build.
    #[test]
    fn test_build_query_fields_connection_only() {
        let resource = make_user_resource();
        let mut behaviors = BehaviorSet::none();
        behaviors.add(BehaviorSet::CONNECTION);
        behaviors.add(BehaviorSet::FILTER);
        behaviors.add(BehaviorSet::ORDER_BY);

        let result = build_query_schema(&resource, behaviors);
        assert!(result.is_ok(), "connection-only query fields should build: {:?}", result.err());
    }

    /// SELECT_ONE only should produce xById field.
    #[test]
    fn test_build_query_fields_select_one_only() {
        let resource = make_user_resource();
        let mut behaviors = BehaviorSet::none();
        behaviors.add(BehaviorSet::SELECT_ONE);

        let fields = build_query_fields(&resource, &behaviors);
        assert_eq!(fields.len(), 1, "select_one should produce 1 field");
    }

    /// No behaviors → no fields.
    #[test]
    fn test_build_query_fields_none() {
        let resource = make_user_resource();
        let behaviors = BehaviorSet::none();
        let fields = build_query_fields(&resource, &behaviors);
        assert!(fields.is_empty(), "no behaviors should produce no fields");
    }

    /// Both behaviors should produce 2 fields.
    #[test]
    fn test_build_query_fields_both_behaviors() {
        let resource = make_user_resource();
        let mut behaviors = BehaviorSet::none();
        behaviors.add(BehaviorSet::CONNECTION);
        behaviors.add(BehaviorSet::SELECT_ONE);

        let fields = build_query_fields(&resource, &behaviors);
        assert_eq!(fields.len(), 2, "both behaviors should produce 2 fields");
    }
}
