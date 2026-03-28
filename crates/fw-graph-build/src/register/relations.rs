//! Build relation fields for GraphQL object types.
//!
//! For FK relations:
//! - BelongsTo: on source type, field `xByFkCol` returning target type
//! - HasMany: on target type, field `xsByFkCol` returning connection type

use async_graphql::dynamic::{Field, FieldFuture, InputValue, TypeRef};
use async_graphql::Value;

use crate::ir::{ResolvedRelation, ResolvedResource};
use crate::naming::{belongs_to_field_name, connection_type_name, has_many_field_name};

/// Build relation fields to be added to object types.
///
/// Returns `(type_name, Field)` pairs: `type_name` is the GraphQL object that
/// should receive the field.
pub fn build_relation_fields(
    relations: &[ResolvedRelation],
    resources: &[ResolvedResource],
) -> Vec<(String, Field)> {
    let mut out: Vec<(String, Field)> = Vec::new();

    for rel in relations {
        // Resolve source/target resource names
        let source_type = rel.source_resource.clone();
        let target_type = rel.target_resource.clone();

        // Find the source resource to get its table name for naming
        let source_table = resources
            .iter()
            .find(|r| r.name == source_type)
            .map(|r| r.table.as_str())
            .unwrap_or(&source_type);

        // Find the target resource for its table name
        let target_table = resources
            .iter()
            .find(|r| r.name == target_type)
            .map(|r| r.table.as_str())
            .unwrap_or(&target_type);

        // Use the first FK column for naming
        let fk_col = rel
            .source_columns
            .first()
            .map(|s| s.as_str())
            .unwrap_or("id");

        // ── BelongsTo: add field on source type ───────────────────────────────
        {
            let field_name = belongs_to_field_name(fk_col, target_table);
            let target_type_ref = target_type.clone();
            let field = Field::new(field_name, TypeRef::named(&target_type_ref), |_| {
                FieldFuture::from_value(Some(Value::Null))
            });
            out.push((source_type.clone(), field));
        }

        // ── HasMany: add field on target type (only if not unique) ────────────
        if !rel.is_unique {
            let field_name = has_many_field_name(fk_col, source_table);
            let conn_type = connection_type_name(&source_type);
            let field = Field::new(field_name, TypeRef::named(&conn_type), move |_| {
                FieldFuture::from_value(Some(Value::Null))
            })
            .argument(InputValue::new("first", TypeRef::named(TypeRef::INT)))
            .argument(InputValue::new("last", TypeRef::named(TypeRef::INT)))
            .argument(InputValue::new("after", TypeRef::named("Cursor")))
            .argument(InputValue::new("before", TypeRef::named("Cursor")));

            out.push((target_type.clone(), field));
        }
    }

    out
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::dynamic::{Field, FieldFuture, Object, Schema, TypeRef};
    use crate::ir::{ResolvedColumn, ResolvedRelation, ResolvedResource, ResourceKind};
    use crate::register::connections::{register_connection_types, register_page_info};
    use crate::register::scalars::register_scalars;

    fn make_user_resource() -> ResolvedResource {
        ResolvedResource {
            name: "User".to_string(),
            schema: "public".to_string(),
            table: "users".to_string(),
            kind: ResourceKind::Table,
            columns: vec![ResolvedColumn {
                pg_name: "id".to_string(),
                gql_name: "id".to_string(),
                type_oid: 20,
                gql_type: TypeRef::INT.to_string(),
                is_not_null: true,
                has_default: true,
            }],
            primary_key: vec!["id".to_string()],
            unique_constraints: vec![],
            class_oid: 1,
        }
    }

    fn make_post_resource() -> ResolvedResource {
        ResolvedResource {
            name: "Post".to_string(),
            schema: "public".to_string(),
            table: "posts".to_string(),
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
                    pg_name: "author_id".to_string(),
                    gql_name: "authorId".to_string(),
                    type_oid: 20,
                    gql_type: TypeRef::INT.to_string(),
                    is_not_null: false,
                    has_default: false,
                },
            ],
            primary_key: vec!["id".to_string()],
            unique_constraints: vec![],
            class_oid: 2,
        }
    }

    fn make_fk_relation() -> ResolvedRelation {
        ResolvedRelation {
            name: "posts_author_fk".to_string(),
            source_resource: "Post".to_string(),
            source_columns: vec!["author_id".to_string()],
            target_resource: "User".to_string(),
            target_columns: vec!["id".to_string()],
            is_unique: false,
        }
    }

    /// Relation fields should build when added to object types.
    #[test]
    fn test_build_relation_fields_builds_schema() {
        let user = make_user_resource();
        let post = make_post_resource();
        let rel = make_fk_relation();
        let resources = vec![user.clone(), post.clone()];
        let relations = vec![rel];

        let relation_fields = build_relation_fields(&relations, &resources);

        // Build objects and add relation fields
        let mut user_obj = Object::new("User").field(Field::new(
            "id",
            TypeRef::named_nn(TypeRef::INT),
            |_| FieldFuture::from_value(Some(async_graphql::Value::Null)),
        ));
        let mut post_obj = Object::new("Post").field(Field::new(
            "id",
            TypeRef::named_nn(TypeRef::INT),
            |_| FieldFuture::from_value(Some(async_graphql::Value::Null)),
        ));

        for (type_name, field) in relation_fields {
            if type_name == "User" {
                user_obj = user_obj.field(field);
            } else if type_name == "Post" {
                post_obj = post_obj.field(field);
            }
        }

        let query = Object::new("Query").field(Field::new(
            "placeholder",
            TypeRef::named(TypeRef::STRING),
            |_| FieldFuture::from_value(Some(async_graphql::Value::Null)),
        ));

        let mut builder = Schema::build("Query", None, None);
        builder = register_scalars(builder);
        builder = register_page_info(builder);
        // Register connection type for Post (for the has-many field)
        builder = register_connection_types(builder, &post);
        builder = builder.register(user_obj);
        builder = builder.register(post_obj);
        builder = builder.register(query);

        let schema = builder.finish();
        assert!(schema.is_ok(), "schema with relation fields should build: {:?}", schema.err());
    }

    /// BelongsTo produces a field on the source type.
    #[test]
    fn test_belongs_to_field_on_source() {
        let user = make_user_resource();
        let post = make_post_resource();
        let rel = make_fk_relation();
        let resources = vec![user, post];
        let fields = build_relation_fields(&[rel], &resources);

        // Should have a BelongsTo on Post (source) → User
        let post_fields: Vec<_> = fields.iter().filter(|(t, _)| t == "Post").collect();
        assert!(!post_fields.is_empty(), "should have field on Post");
    }

    /// HasMany (non-unique) produces a field on the target type.
    #[test]
    fn test_has_many_field_on_target() {
        let user = make_user_resource();
        let post = make_post_resource();
        let rel = make_fk_relation(); // is_unique = false
        let resources = vec![user, post];
        let fields = build_relation_fields(&[rel], &resources);

        let user_fields: Vec<_> = fields.iter().filter(|(t, _)| t == "User").collect();
        assert!(!user_fields.is_empty(), "should have has-many field on User");
    }

    /// Unique relation should NOT produce a has-many field.
    #[test]
    fn test_unique_relation_no_has_many() {
        let user = make_user_resource();
        let post = make_post_resource();
        let rel = ResolvedRelation {
            name: "unique_fk".to_string(),
            source_resource: "Post".to_string(),
            source_columns: vec!["author_id".to_string()],
            target_resource: "User".to_string(),
            target_columns: vec!["id".to_string()],
            is_unique: true, // unique → no has-many
        };
        let resources = vec![user, post];
        let fields = build_relation_fields(&[rel], &resources);

        // Should only have the BelongsTo (on Post), not has-many (on User)
        let user_fields: Vec<_> = fields.iter().filter(|(t, _)| t == "User").collect();
        assert!(user_fields.is_empty(), "unique relation should not produce has-many");

        let post_fields: Vec<_> = fields.iter().filter(|(t, _)| t == "Post").collect();
        assert!(!post_fields.is_empty(), "belongs-to should still be on Post");
    }
}
