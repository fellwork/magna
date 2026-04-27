//! Resolver factories for mutation fields (createX, updateX, deleteX).

use std::sync::Arc;

use async_graphql::dynamic::{Field, FieldFuture, FieldValue, InputValue, TypeRef};
use async_graphql::Value;
use magna_types::PgValue;

use crate::executor::args::gql_value_to_pg_value;
use crate::executor::{QueryExecutor, RequestConnection};
use crate::ir::ResolvedResource;
use crate::naming::{
    create_input_type_name, create_mutation_field_name, create_payload_type_name,
    delete_input_type_name, delete_mutation_field_name, delete_payload_type_name,
    update_input_type_name, update_mutation_field_name, update_payload_type_name,
};
use crate::register::node_interface::decode_node_id;

/// Build the `createX(input: CreateXInput!) → CreateXPayload` resolver field.
pub fn build_create_resolver(resource: &ResolvedResource, executor: Arc<QueryExecutor>) -> Field {
    let field_name = create_mutation_field_name(&resource.name);
    let input_type = create_input_type_name(&resource.name);
    let payload_type = create_payload_type_name(&resource.name);
    let resource = resource.clone();

    Field::new(field_name, TypeRef::named(&payload_type), move |ctx| {
        let executor = executor.clone();
        let resource = resource.clone();
        FieldFuture::new(async move {
            let conn = ctx.data_opt::<RequestConnection>().ok_or_else(|| {
                async_graphql::Error::new("No database connection in request context")
            })?;

            let input_val = ctx
                .args
                .get("input")
                .map(|a| a.as_value().clone())
                .ok_or_else(|| async_graphql::Error::new("missing input argument"))?;

            let columns: Vec<(String, PgValue)> = if let Value::Object(map) = &input_val {
                resource
                    .columns
                    .iter()
                    .filter_map(|col| {
                        map.get(col.gql_name.as_str()).map(|v| {
                            let val = gql_value_to_pg_value(v);
                            (col.pg_name.clone(), val)
                        })
                    })
                    .collect()
            } else {
                return Err(async_graphql::Error::new("input must be an object"));
            };

            let col_refs: Vec<(&str, PgValue)> =
                columns.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();

            let row = executor
                .insert(conn, &resource, col_refs)
                .await
                .map_err(|e| e.into_gql())?;

            Ok(Some(FieldValue::owned_any(row)))
        })
    })
    .argument(InputValue::new("input", TypeRef::named_nn(input_type)))
}

/// Build the `updateX(input: UpdateXInput!) → UpdateXPayload` resolver field.
pub fn build_update_resolver(resource: &ResolvedResource, executor: Arc<QueryExecutor>) -> Field {
    let field_name = update_mutation_field_name(&resource.name);
    let input_type = update_input_type_name(&resource.name);
    let payload_type = update_payload_type_name(&resource.name);
    let resource = resource.clone();

    Field::new(field_name, TypeRef::named(&payload_type), move |ctx| {
        let executor = executor.clone();
        let resource = resource.clone();
        FieldFuture::new(async move {
            let conn = ctx.data_opt::<RequestConnection>().ok_or_else(|| {
                async_graphql::Error::new("No database connection in request context")
            })?;

            let input_val = ctx
                .args
                .get("input")
                .map(|a| a.as_value().clone())
                .ok_or_else(|| async_graphql::Error::new("missing input argument"))?;

            let (node_id_str, patch_val) = if let Value::Object(map) = &input_val {
                let nid = map
                    .get("nodeId")
                    .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                    .ok_or_else(|| async_graphql::Error::new("missing nodeId in update input"))?;
                let patch = map
                    .get("patch")
                    .cloned()
                    .ok_or_else(|| async_graphql::Error::new("missing patch in update input"))?;
                (nid, patch)
            } else {
                return Err(async_graphql::Error::new("input must be an object"));
            };

            // Decode nodeId → PK values
            let (_type_name, pk_str) = decode_node_id(&node_id_str)
                .map_err(async_graphql::Error::new)?;
            let pk_owned = parse_pk_values_from_str(&resource, &pk_str)?;

            // Collect patch columns
            let set_owned: Vec<(String, PgValue)> = if let Value::Object(patch_map) = &patch_val {
                resource
                    .columns
                    .iter()
                    .filter_map(|col| {
                        patch_map.get(col.gql_name.as_str()).map(|v| {
                            (col.pg_name.clone(), gql_value_to_pg_value(v))
                        })
                    })
                    .collect()
            } else {
                return Err(async_graphql::Error::new("patch must be an object"));
            };

            let pk_refs: Vec<(&str, PgValue)> =
                pk_owned.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
            let set_refs: Vec<(&str, PgValue)> =
                set_owned.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();

            let row = executor
                .update(conn, &resource, pk_refs, set_refs)
                .await
                .map_err(|e| e.into_gql())?;

            match row {
                Some(r) => Ok(Some(FieldValue::owned_any(r))),
                None    => Ok(Some(FieldValue::value(Value::Null))),
            }
        })
    })
    .argument(InputValue::new("input", TypeRef::named_nn(input_type)))
}

/// Build the `deleteX(input: DeleteXInput!) → DeleteXPayload` resolver field.
pub fn build_delete_resolver(resource: &ResolvedResource, executor: Arc<QueryExecutor>) -> Field {
    let field_name = delete_mutation_field_name(&resource.name);
    let input_type = delete_input_type_name(&resource.name);
    let payload_type = delete_payload_type_name(&resource.name);
    let resource = resource.clone();

    Field::new(field_name, TypeRef::named(&payload_type), move |ctx| {
        let executor = executor.clone();
        let resource = resource.clone();
        FieldFuture::new(async move {
            let conn = ctx.data_opt::<RequestConnection>().ok_or_else(|| {
                async_graphql::Error::new("No database connection in request context")
            })?;

            let input_val = ctx
                .args
                .get("input")
                .map(|a| a.as_value().clone())
                .ok_or_else(|| async_graphql::Error::new("missing input argument"))?;

            let node_id_str = if let Value::Object(map) = &input_val {
                map.get("nodeId")
                    .and_then(|v| if let Value::String(s) = v { Some(s.clone()) } else { None })
                    .ok_or_else(|| async_graphql::Error::new("missing nodeId in delete input"))?
            } else {
                return Err(async_graphql::Error::new("input must be an object"));
            };

            let (_type_name, pk_str) = decode_node_id(&node_id_str)
                .map_err(async_graphql::Error::new)?;
            let pk_owned = parse_pk_values_from_str(&resource, &pk_str)?;
            let pk_refs: Vec<(&str, PgValue)> =
                pk_owned.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();

            let row = executor
                .delete(conn, &resource, pk_refs)
                .await
                .map_err(|e| e.into_gql())?;

            match row {
                Some(r) => Ok(Some(FieldValue::owned_any(r))),
                None    => Ok(Some(FieldValue::value(Value::Null))),
            }
        })
    })
    .argument(InputValue::new("input", TypeRef::named_nn(input_type)))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parse "42" or "col1=val1,col2=val2" into (pg_col_name, PgValue) pairs.
fn parse_pk_values_from_str(
    resource: &ResolvedResource,
    pk_str: &str,
) -> Result<Vec<(String, PgValue)>, async_graphql::Error> {
    if resource.primary_key.len() == 1 {
        let pk_col = resource.primary_key[0].clone();
        let type_oid = resource
            .columns
            .iter()
            .find(|c| c.pg_name == pk_col)
            .map(|c| c.type_oid)
            .unwrap_or(25);
        let val = crate::executor::sql_bridge::parse_pk_string(pk_str, type_oid);
        Ok(vec![(pk_col, val)])
    } else {
        // Composite PK: "col1=val1,col2=val2"
        let pairs: Vec<(String, PgValue)> = pk_str
            .split(',')
            .filter_map(|part| {
                let mut kv = part.splitn(2, '=');
                let key = kv.next()?.trim().to_string();
                let val_str = kv.next()?.trim();
                let col = resource.columns.iter().find(|c| c.pg_name == key)?;
                let val = crate::executor::sql_bridge::parse_pk_string(val_str, col.type_oid);
                Some((key, val))
            })
            .collect();
        if pairs.is_empty() {
            Err(async_graphql::Error::new("could not parse composite PK from nodeId"))
        } else {
            Ok(pairs)
        }
    }
}
