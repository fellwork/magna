//! Resolver factories for top-level query fields (allX, xById).

use std::sync::Arc;

use async_graphql::dynamic::{Field, FieldFuture, FieldValue, InputValue, TypeRef};
use magna_types::PgValue;

use crate::executor::args::{ConnectionArgs, gql_value_to_pg_value};
use crate::executor::{ConnectionResult, QueryExecutor, RequestConnection};
use crate::ir::ResolvedResource;
use crate::naming::{
    all_query_field_name, by_pk_query_field_name, condition_type_name, connection_type_name,
    filter_type_name, order_by_type_name, to_camel_case,
};
use crate::register::connections::{ConnectionData, PageInfoData};
use crate::register::object_types::gql_type_to_type_ref;

/// Build the `allX(first, last, after, before, filter, orderBy, condition) → XConnection`
/// resolver field. Returns a `Field` ready to be added to the Query object.
pub fn build_allx_resolver(resource: &ResolvedResource, executor: Arc<QueryExecutor>) -> Field {
    let field_name = all_query_field_name(&resource.name);
    let conn_type = connection_type_name(&resource.name);
    let filter_type = filter_type_name(&resource.name);
    let order_by_type = order_by_type_name(&resource.name);
    let condition_type = condition_type_name(&resource.name);
    let resource = resource.clone();

    Field::new(field_name, TypeRef::named(&conn_type), move |ctx| {
        let executor = executor.clone();
        let resource = resource.clone();

        FieldFuture::new(async move {
            let conn = ctx.data_opt::<RequestConnection>().ok_or_else(|| {
                async_graphql::Error::new("No database connection in request context")
            })?;

            let args = ConnectionArgs::from_ctx(&ctx);

            let result: ConnectionResult = executor
                .select_connection(conn, &resource, args)
                .await
                .map_err(|e| e.into_gql())?;

            let data = ConnectionData {
                rows: result.rows,
                page_info: PageInfoData {
                    has_next_page: result.has_next_page,
                    has_previous_page: result.has_previous_page,
                    start_cursor: result.start_cursor,
                    end_cursor: result.end_cursor,
                },
                total_count: result.total_count,
            };
            Ok(Some(FieldValue::owned_any(data)))
        })
    })
    .argument(InputValue::new("first", TypeRef::named(TypeRef::INT)))
    .argument(InputValue::new("last", TypeRef::named(TypeRef::INT)))
    .argument(InputValue::new("after", TypeRef::named("Cursor")))
    .argument(InputValue::new("before", TypeRef::named("Cursor")))
    .argument(InputValue::new("filter", TypeRef::named(filter_type)))
    .argument(InputValue::new("orderBy", TypeRef::named_list(order_by_type)))
    .argument(InputValue::new("condition", TypeRef::named(condition_type)))
}

/// Build the `xById(pkCol: Type!) → X` resolver field.
pub fn build_by_pk_resolver(resource: &ResolvedResource, executor: Arc<QueryExecutor>) -> Field {
    let field_name = by_pk_query_field_name(&resource.name);
    let obj_type = resource.name.clone();
    let resource_for_field = resource.clone();

    // Collect PK column metadata for argument declarations.
    let pk_args: Vec<(String, String)> = resource
        .primary_key
        .iter()
        .map(|pk_col| {
            let gql_type = resource
                .columns
                .iter()
                .find(|c| &c.pg_name == pk_col)
                .map(|c| c.gql_type.clone())
                .unwrap_or_else(|| TypeRef::STRING.to_string());
            let arg_name = to_camel_case(pk_col);
            (arg_name, gql_type)
        })
        .collect();

    let resource = resource_for_field;
    let mut field = Field::new(field_name, TypeRef::named(&obj_type), move |ctx| {
        let executor = executor.clone();
        let resource = resource.clone();

        FieldFuture::new(async move {
            let conn = ctx.data_opt::<RequestConnection>().ok_or_else(|| {
                async_graphql::Error::new("No database connection in request context")
            })?;

            // Collect PK arg values from resolver context.
            let pk_values: Vec<(String, PgValue)> = resource
                .primary_key
                .iter()
                .map(|pk_col| {
                    let arg_name = to_camel_case(pk_col);
                    let gql_val = ctx
                        .args
                        .get(arg_name.as_str())
                        .map(|a| a.as_value().clone())
                        .unwrap_or(async_graphql::Value::Null);
                    let pg_val = gql_value_to_pg_value(&gql_val);
                    (pk_col.clone(), pg_val)
                })
                .collect();

            let pk_refs: Vec<(&str, PgValue)> =
                pk_values.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();

            match executor.select_by_pk(conn, &resource, pk_refs).await.map_err(|e| e.into_gql())? {
                Some(row) => Ok(Some(FieldValue::owned_any(row))),
                None      => Ok(None),
            }
        })
    });

    for (arg_name, gql_type) in pk_args {
        field = field.argument(InputValue::new(arg_name, gql_type_to_type_ref(&gql_type, true)));
    }

    field
}
