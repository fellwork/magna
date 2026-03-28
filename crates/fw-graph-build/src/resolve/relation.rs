//! Resolver factories for relation fields (belongsTo, hasMany).
//!
//! Uses DataLoaderRegistry stored in schema data for batching.

use std::sync::Arc;

use async_graphql::dynamic::{Field, FieldFuture, FieldValue, TypeRef};
use async_graphql::Value;
use fw_graph_types::{PgRow, PgValue};

use crate::executor::dataloader::{DataLoaderRegistry, LoadKey};
use crate::ir::{ResolvedRelation, ResolvedResource};
use crate::naming::{belongs_to_field_name, connection_type_name, has_many_field_name};
use crate::register::connections::{ConnectionData, PageInfoData};

/// Build the `xByFkCol → TargetType` resolver for a belongsTo relation.
/// Registered on the source type (the one with the FK column).
pub fn build_belongs_to_resolver(
    rel: &ResolvedRelation,
    source_resource: &ResolvedResource,
    target_resource: &ResolvedResource,
) -> (String, Field) {
    let fk_col = rel
        .source_columns
        .first()
        .map(|s| s.clone())
        .unwrap_or_else(|| "id".to_string());
    let field_name = belongs_to_field_name(&fk_col, &target_resource.table);
    let target_type = target_resource.name.clone();

    let loader_key = format!(
        "{}:{}",
        target_resource.name,
        target_resource.primary_key.first().cloned().unwrap_or_default()
    );
    let fk_col_owned = fk_col.clone();

    let field = Field::new(&field_name, TypeRef::named(&target_type), move |ctx| {
        let loader_key = loader_key.clone();
        let fk_col = fk_col_owned.clone();
        FieldFuture::new(async move {
            let row = match ctx.parent_value.try_downcast_ref::<PgRow>() {
                Ok(r) => r,
                Err(_) => return Ok(None),
            };

            let fk_val = match row.get(fk_col.as_str()) {
                Some(v) => v,
                None => return Ok(Some(FieldValue::value(Value::Null))),
            };
            let key_str = pg_value_str(fk_val);
            if key_str.is_empty() {
                return Ok(Some(FieldValue::value(Value::Null)));
            }

            let registry = ctx
                .data_opt::<Arc<DataLoaderRegistry>>()
                .ok_or_else(|| async_graphql::Error::new("DataLoaderRegistry not in schema data"))?;

            let loader = registry.get_belongs_to(&loader_key).ok_or_else(|| {
                async_graphql::Error::new(format!("No belongsTo loader for key: {}", loader_key))
            })?;

            match loader.load_one(LoadKey(key_str)).await {
                Ok(Some(target_row)) => Ok(Some(FieldValue::owned_any(target_row))),
                Ok(None)             => Ok(Some(FieldValue::value(Value::Null))),
                Err(e)               => Err(async_graphql::Error::new(e.to_string())),
            }
        })
    });

    (source_resource.name.clone(), field)
}

/// Build the `xsByFkCol(first, last, after, before) → XConnection` resolver for a hasMany relation.
/// Registered on the target type (the one being referenced by the FK).
pub fn build_has_many_resolver(
    rel: &ResolvedRelation,
    source_resource: &ResolvedResource,
    target_resource: &ResolvedResource,
) -> (String, Field) {
    let fk_col = rel
        .source_columns
        .first()
        .cloned()
        .unwrap_or_else(|| "id".to_string());
    let field_name = has_many_field_name(&fk_col, &source_resource.table);
    let conn_type = connection_type_name(&source_resource.name);
    let target_pk = target_resource
        .primary_key
        .first()
        .cloned()
        .unwrap_or_default();

    let loader_key = format!("{}:{}", source_resource.name, fk_col);
    let target_pk_owned = target_pk.clone();

    let field = Field::new(&field_name, TypeRef::named(&conn_type), move |ctx| {
        let loader_key = loader_key.clone();
        let target_pk = target_pk_owned.clone();
        FieldFuture::new(async move {
            let row = match ctx.parent_value.try_downcast_ref::<PgRow>() {
                Ok(r) => r,
                Err(_) => {
                    return Ok(Some(FieldValue::owned_any(empty_connection())));
                }
            };

            let pk_val = match row.get(target_pk.as_str()) {
                Some(v) => v,
                None => return Ok(Some(FieldValue::owned_any(empty_connection()))),
            };
            let key_str = pg_value_str(pk_val);
            if key_str.is_empty() {
                return Ok(Some(FieldValue::owned_any(empty_connection())));
            }

            let registry = ctx
                .data_opt::<Arc<DataLoaderRegistry>>()
                .ok_or_else(|| async_graphql::Error::new("DataLoaderRegistry not in schema data"))?;

            let loader = registry.get_has_many(&loader_key).ok_or_else(|| {
                async_graphql::Error::new(format!("No hasMany loader for key: {}", loader_key))
            })?;

            let rows = loader
                .load_one(LoadKey(key_str))
                .await
                .map_err(|e| async_graphql::Error::new(e.to_string()))?
                .unwrap_or_default();

            let data = ConnectionData {
                total_count: None,
                page_info: PageInfoData {
                    has_next_page: false,
                    has_previous_page: false,
                    start_cursor: None,
                    end_cursor: None,
                },
                rows,
            };
            Ok(Some(FieldValue::owned_any(data)))
        })
    });

    (target_resource.name.clone(), field)
}

fn empty_connection() -> ConnectionData {
    ConnectionData {
        rows: vec![],
        total_count: Some(0),
        page_info: PageInfoData {
            has_next_page: false,
            has_previous_page: false,
            start_cursor: None,
            end_cursor: None,
        },
    }
}

fn pg_value_str(val: &PgValue) -> String {
    match val {
        PgValue::Null         => String::new(),
        PgValue::Bool(b)      => b.to_string(),
        PgValue::Int(n)       => n.to_string(),
        PgValue::Float(f)     => f.to_string(),
        PgValue::Text(s)      => s.clone(),
        PgValue::Uuid(u)      => u.to_string(),
        PgValue::Timestamp(t) => t.to_rfc3339(),
        PgValue::Json(j)      => j.to_string(),
        PgValue::Array(_)     => String::new(),
    }
}
