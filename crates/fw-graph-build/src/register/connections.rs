//! Register GraphQL connection, edge, and PageInfo types.
//!
//! Implements the Relay-style cursor-based pagination pattern:
//! XConnection → [XEdge], XEdge → X node + cursor, PageInfo.

use async_graphql::dynamic::{Field, FieldFuture, FieldValue, Object, SchemaBuilder, TypeRef};
use async_graphql::Value;
use fw_graph_types::PgRow;

use crate::ir::ResolvedResource;
use crate::naming::{connection_type_name, edge_type_name};

// ── Data types carried in FieldValue for resolvers ────────────────────────────

/// Runtime data for a PageInfo object resolver.
#[derive(Debug, Clone)]
pub struct PageInfoData {
    pub has_next_page: bool,
    pub has_previous_page: bool,
    pub start_cursor: Option<String>,
    pub end_cursor: Option<String>,
}

/// Runtime data for an XConnection resolver.
#[derive(Debug, Clone)]
pub struct ConnectionData {
    pub rows: Vec<PgRow>,
    pub page_info: PageInfoData,
    pub total_count: Option<i64>,
}

/// Runtime data for an XEdge resolver.
#[derive(Debug, Clone)]
pub struct EdgeData {
    pub row: PgRow,
    pub cursor: String,
}

// ── PageInfo ──────────────────────────────────────────────────────────────────

/// Register the `PageInfo` type with cursor-pagination fields.
pub fn register_page_info(mut builder: SchemaBuilder) -> SchemaBuilder {
    let obj = Object::new("PageInfo")
        .field(Field::new(
            "hasNextPage",
            TypeRef::named_nn(TypeRef::BOOLEAN),
            |ctx| {
                FieldFuture::new(async move {
                    let v = ctx
                        .parent_value
                        .try_downcast_ref::<PageInfoData>()
                        .map(|d| d.has_next_page)
                        .unwrap_or(false);
                    Ok(Some(FieldValue::value(Value::from(v))))
                })
            },
        ))
        .field(Field::new(
            "hasPreviousPage",
            TypeRef::named_nn(TypeRef::BOOLEAN),
            |ctx| {
                FieldFuture::new(async move {
                    let v = ctx
                        .parent_value
                        .try_downcast_ref::<PageInfoData>()
                        .map(|d| d.has_previous_page)
                        .unwrap_or(false);
                    Ok(Some(FieldValue::value(Value::from(v))))
                })
            },
        ))
        .field(Field::new(
            "startCursor",
            TypeRef::named("Cursor"),
            |ctx| {
                FieldFuture::new(async move {
                    let cursor = ctx
                        .parent_value
                        .try_downcast_ref::<PageInfoData>()
                        .ok()
                        .and_then(|d| d.start_cursor.clone());
                    Ok(Some(match cursor {
                        Some(s) => FieldValue::value(Value::from(s)),
                        None => FieldValue::value(Value::Null),
                    }))
                })
            },
        ))
        .field(Field::new(
            "endCursor",
            TypeRef::named("Cursor"),
            |ctx| {
                FieldFuture::new(async move {
                    let cursor = ctx
                        .parent_value
                        .try_downcast_ref::<PageInfoData>()
                        .ok()
                        .and_then(|d| d.end_cursor.clone());
                    Ok(Some(match cursor {
                        Some(s) => FieldValue::value(Value::from(s)),
                        None => FieldValue::value(Value::Null),
                    }))
                })
            },
        ));

    builder = builder.register(obj);
    builder
}

// ── XConnection + XEdge ───────────────────────────────────────────────────────

/// Register `XConnection` and `XEdge` types for a resource.
///
/// - `XConnection`: nodes, edges, totalCount, pageInfo
/// - `XEdge`: node, cursor
pub fn register_connection_types(
    mut builder: SchemaBuilder,
    resource: &ResolvedResource,
) -> SchemaBuilder {
    let type_name = resource.name.clone();
    let conn_name = connection_type_name(&type_name);
    let edge_name = edge_type_name(&type_name);

    // ── XEdge ─────────────────────────────────────────────────────────────────
    let edge_type_name_clone = type_name.clone();
    let edge_obj = Object::new(&edge_name)
        .field(Field::new(
            "node",
            TypeRef::named_nn(&edge_type_name_clone),
            |ctx| {
                FieldFuture::new(async move {
                    match ctx.parent_value.try_downcast_ref::<EdgeData>() {
                        Ok(edge) => Ok(Some(FieldValue::owned_any(edge.row.clone()))),
                        Err(_) => Ok(Some(FieldValue::value(Value::Null))),
                    }
                })
            },
        ))
        .field(Field::new("cursor", TypeRef::named_nn(TypeRef::STRING), |ctx| {
            FieldFuture::new(async move {
                match ctx.parent_value.try_downcast_ref::<EdgeData>() {
                    Ok(edge) => Ok(Some(FieldValue::value(Value::from(edge.cursor.clone())))),
                    Err(_) => Ok(Some(FieldValue::value(Value::Null))),
                }
            })
        }));

    // ── XConnection ───────────────────────────────────────────────────────────
    let nodes_type = type_name.clone();
    let edges_type = edge_name.clone();

    let conn_obj = Object::new(&conn_name)
        .field(Field::new(
            "nodes",
            TypeRef::named_nn_list_nn(&nodes_type),
            |ctx| {
                FieldFuture::new(async move {
                    match ctx.parent_value.try_downcast_ref::<ConnectionData>() {
                        Ok(data) => {
                            let items: Vec<FieldValue<'static>> = data
                                .rows
                                .iter()
                                .map(|row| FieldValue::owned_any(row.clone()))
                                .collect();
                            Ok(Some(FieldValue::list(items)))
                        }
                        Err(_) => Ok(Some(FieldValue::list(Vec::<FieldValue>::new()))),
                    }
                })
            },
        ))
        .field(Field::new(
            "edges",
            TypeRef::named_nn_list_nn(&edges_type),
            |ctx| {
                FieldFuture::new(async move {
                    match ctx.parent_value.try_downcast_ref::<ConnectionData>() {
                        Ok(data) => {
                            let items: Vec<FieldValue<'static>> = data
                                .rows
                                .iter()
                                .enumerate()
                                .map(|(i, row)| {
                                    FieldValue::owned_any(EdgeData {
                                        row: row.clone(),
                                        cursor: format!("cursor_{}", i),
                                    })
                                })
                                .collect();
                            Ok(Some(FieldValue::list(items)))
                        }
                        Err(_) => Ok(Some(FieldValue::list(Vec::<FieldValue>::new()))),
                    }
                })
            },
        ))
        .field(Field::new("totalCount", TypeRef::named(TypeRef::INT), |ctx| {
            FieldFuture::new(async move {
                match ctx.parent_value.try_downcast_ref::<ConnectionData>() {
                    Ok(data) => match data.total_count {
                        Some(n) => Ok(Some(FieldValue::value(Value::from(n as i32)))),
                        None => Ok(Some(FieldValue::value(Value::Null))),
                    },
                    Err(_) => Ok(Some(FieldValue::value(Value::Null))),
                }
            })
        }))
        .field(Field::new("pageInfo", TypeRef::named_nn("PageInfo"), |ctx| {
            FieldFuture::new(async move {
                match ctx.parent_value.try_downcast_ref::<ConnectionData>() {
                    Ok(data) => {
                        Ok(Some(FieldValue::owned_any(data.page_info.clone())))
                    }
                    Err(_) => Ok(Some(FieldValue::owned_any(PageInfoData {
                        has_next_page: false,
                        has_previous_page: false,
                        start_cursor: None,
                        end_cursor: None,
                    }))),
                }
            })
        }));

    builder = builder.register(edge_obj);
    builder = builder.register(conn_obj);
    builder
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::dynamic::{Schema};
    use crate::ir::ResolvedColumn;
    use crate::register::scalars::register_scalars;

    fn make_placeholder_query() -> Object {
        Object::new("Query").field(Field::new(
            "placeholder",
            TypeRef::named(TypeRef::STRING),
            |_| FieldFuture::from_value(Some(Value::Null)),
        ))
    }

    fn make_user_resource() -> ResolvedResource {
        ResolvedResource {
            name: "User".to_string(),
            schema: "public".to_string(),
            table: "users".to_string(),
            kind: crate::ir::ResourceKind::Table,
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
            class_oid: 12345,
        }
    }

    /// PageInfo and connection types should build a valid schema.
    #[test]
    fn test_register_page_info_builds_schema() {
        let mut builder = Schema::build("Query", None, None);
        builder = register_scalars(builder);
        builder = register_page_info(builder);

        // We need a Query type that references PageInfo
        let query = Object::new("Query").field(Field::new(
            "dummyPageInfo",
            TypeRef::named("PageInfo"),
            |_| FieldFuture::from_value(Some(Value::Null)),
        ));
        builder = builder.register(query);

        let schema = builder.finish();
        assert!(schema.is_ok(), "PageInfo schema should build: {:?}", schema.err());
    }

    /// Connection types require PageInfo + object type to be registered first.
    #[test]
    fn test_register_connection_types_builds_schema() {
        use crate::register::object_types::register_object_types;

        let resources = vec![make_user_resource()];

        let mut builder = Schema::build("Query", None, None);
        builder = register_scalars(builder);
        builder = register_page_info(builder);
        builder = register_object_types(builder, &resources);
        builder = register_connection_types(builder, &resources[0]);

        let query = Object::new("Query").field(Field::new(
            "allUsers",
            TypeRef::named("UsersConnection"),
            |_| FieldFuture::from_value(Some(Value::Null)),
        ));
        builder = builder.register(query);

        let schema = builder.finish();
        assert!(schema.is_ok(), "connection types schema should build: {:?}", schema.err());
    }

    /// PageInfoData struct should be constructable and cloneable.
    #[test]
    fn test_page_info_data_struct() {
        let data = PageInfoData {
            has_next_page: true,
            has_previous_page: false,
            start_cursor: Some("abc".to_string()),
            end_cursor: None,
        };
        let cloned = data.clone();
        assert_eq!(cloned.has_next_page, true);
        assert_eq!(cloned.has_previous_page, false);
        assert_eq!(cloned.start_cursor, Some("abc".to_string()));
        assert_eq!(cloned.end_cursor, None);
    }

    /// ConnectionData and EdgeData should be constructable.
    #[test]
    fn test_connection_edge_data_struct() {
        let page_info = PageInfoData {
            has_next_page: false,
            has_previous_page: false,
            start_cursor: None,
            end_cursor: None,
        };
        let conn = ConnectionData {
            rows: vec![],
            page_info,
            total_count: Some(0),
        };
        assert_eq!(conn.total_count, Some(0));
        assert!(conn.rows.is_empty());

        let edge = EdgeData {
            row: indexmap::IndexMap::new(),
            cursor: "cursor_0".to_string(),
        };
        assert_eq!(edge.cursor, "cursor_0");
    }

    /// Multiple connection types should build without conflict.
    #[test]
    fn test_register_multiple_connection_types() {
        use crate::register::object_types::register_object_types;

        let post_resource = ResolvedResource {
            name: "Post".to_string(),
            schema: "public".to_string(),
            table: "posts".to_string(),
            kind: crate::ir::ResourceKind::Table,
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
            class_oid: 99,
        };

        let resources = vec![make_user_resource(), post_resource];

        let mut builder = Schema::build("Query", None, None);
        builder = register_scalars(builder);
        builder = register_page_info(builder);
        builder = register_object_types(builder, &resources);
        for r in &resources {
            builder = register_connection_types(builder, r);
        }
        builder = builder.register(make_placeholder_query());

        let schema = builder.finish();
        assert!(schema.is_ok(), "multiple connection types should build: {:?}", schema.err());
    }
}
