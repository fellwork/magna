//! QueryExecutor and RequestConnection — the SQL execution layer for resolvers.

pub mod args;
pub mod cursor;
pub mod dataloader;
pub mod sql_bridge;

use std::sync::Arc;
use tokio::sync::Mutex;

use magna_types::{JwtClaims, PgRow, PgValue};
use magna_sql::{SqlBuilder, SqlFragment};
use sqlx::{pool::PoolConnection, PgPool, Postgres};

use sqlx::PgConnection;

use crate::executor::args::{ConnectionArgs, FilterOp, OrderDirection, QueryError};
use crate::executor::cursor::{decode_cursor, encode_cursor};
use crate::executor::sql_bridge::execute_on_conn;
use crate::ir::ResolvedResource;

// ── RLS helper ────────────────────────────────────────────────────────────────

/// Apply JWT claims as Postgres session configuration for RLS.
/// Mirrors magna-serv's apply_rls_context but lives here to avoid circular deps.
async fn apply_rls_context(conn: &mut PgConnection, claims: &JwtClaims) -> Result<(), sqlx::Error> {
    let claims_json = serde_json::to_string(&claims.raw).unwrap_or_default();
    let sub_str = claims.sub.to_string();
    let role_str = claims.role.as_str().to_owned();
    sqlx::query(
        "SELECT \
            set_config('request.jwt.claims', $1, true), \
            set_config('request.jwt.sub', $2, true), \
            set_config('role', $3, true)",
    )
    .bind(&claims_json)
    .bind(&sub_str)
    .bind(&role_str)
    .execute(conn)
    .await?;
    Ok(())
}

// ── ConnectionResult ──────────────────────────────────────────────────────────

/// The result of a SELECT for a connection field (allX).
pub struct ConnectionResult {
    pub rows: Vec<PgRow>,
    pub has_next_page: bool,
    pub has_previous_page: bool,
    pub start_cursor: Option<String>,
    pub end_cursor: Option<String>,
    pub total_count: Option<i64>,
}

// ── RequestConnection ─────────────────────────────────────────────────────────

/// A pooled Postgres connection with RLS context already applied.
/// Created by the HTTP middleware once per request, stored in async-graphql
/// request data, shared across all top-level resolvers in the same request.
pub struct RequestConnection {
    pub conn: Arc<Mutex<PoolConnection<Postgres>>>,
}

impl RequestConnection {
    /// Acquire a new connection from the pool and apply RLS context.
    pub async fn new(pool: &PgPool, claims: &JwtClaims) -> Result<Self, QueryError> {
        let mut conn = pool
            .acquire()
            .await
            .map_err(QueryError::Database)?;

        apply_rls_context(&mut conn, claims)
            .await
            .map_err(|e| QueryError::RlsError(e.to_string()))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Execute a SQL query using this pre-configured connection.
    pub async fn execute(&self, sql: &str, params: &[PgValue]) -> Result<Vec<PgRow>, QueryError> {
        let mut guard = self.conn.lock().await;
        execute_on_conn(&mut *guard, sql, params).await
    }
}

// ── QueryExecutor ─────────────────────────────────────────────────────────────

/// Builds and executes SQL queries for GraphQL resolvers.
/// Uses RequestConnection for top-level queries/mutations (RLS already applied).
/// Uses pool directly for DataLoader (creates own connections per batch).
pub struct QueryExecutor {
    pub pool: PgPool,
}

impl QueryExecutor {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // ── SELECT connection (allX) ──────────────────────────────────────────────

    pub async fn select_connection(
        &self,
        conn: &RequestConnection,
        resource: &ResolvedResource,
        args: ConnectionArgs,
    ) -> Result<ConnectionResult, QueryError> {
        let pk_cols = &resource.primary_key;
        let default_pk = pk_cols.first().map(|s| s.as_str()).unwrap_or("id");

        // Determine direction and fetch limit.
        let (limit, is_backward) = match (args.first, args.last) {
            (Some(n), _) => (n, false),
            (_, Some(n)) => (n, true),
            _            => (20, false),
        };

        let mut builder = SqlBuilder::from(&resource.schema, &resource.table).column_star();

        // Apply cursor keyset condition.
        if let Some(cursor_str) = if is_backward { &args.before } else { &args.after } {
            if let Ok((pk_map, _sort_map)) = decode_cursor(cursor_str) {
                if let Some(pk_val_str) = pk_map.get(default_pk).and_then(|v| v.as_str()) {
                    let pk_col = resource.columns.iter().find(|c| c.pg_name == *default_pk);
                    let pg_val = pk_col
                        .map(|c| crate::executor::sql_bridge::parse_pk_string(pk_val_str, c.type_oid))
                        .unwrap_or_else(|| PgValue::Text(pk_val_str.to_string()));

                    let cond = if is_backward {
                        SqlFragment::ident(default_pk)
                            .push_raw(" < ")
                            .append(SqlFragment::param(pg_val))
                    } else {
                        SqlFragment::ident(default_pk)
                            .push_raw(" > ")
                            .append(SqlFragment::param(pg_val))
                    };
                    builder = builder.where_clause(cond);
                }
            }
        }

        // Apply condition (exact match per column).
        if let Some(cond_input) = &args.condition {
            for (col, val) in &cond_input.column_values {
                let cv = val.clone();
                let cond = SqlFragment::ident(col.as_str())
                    .push_raw(" = ")
                    .append(SqlFragment::param(cv));
                builder = builder.where_clause(cond);
            }
        }

        // Apply filter.
        if let Some(filter) = &args.filter {
            for cf in &filter.column_ops {
                let cv = cf.value.clone();
                let cond = build_filter_cond(&cf.column, &cf.op, cv);
                builder = builder.where_clause(cond);
            }
        }

        // Apply ORDER BY.
        if args.order_by.is_empty() {
            builder = builder.order_by(SqlFragment::ident(default_pk), !is_backward);
        } else {
            for ob in &args.order_by {
                let asc = if is_backward {
                    ob.direction != OrderDirection::Asc
                } else {
                    ob.direction == OrderDirection::Asc
                };
                builder = builder.order_by(SqlFragment::ident(&ob.column), asc);
            }
        }

        // Fetch one extra to determine hasNextPage/hasPreviousPage.
        builder = builder.limit(limit + 1);

        let (sql, params) = builder.build().build();
        let mut rows = conn.execute(&sql, &params).await?;

        let has_more = rows.len() > limit as usize;
        if has_more {
            rows.truncate(limit as usize);
        }

        // Re-reverse for backward pagination.
        if is_backward {
            rows.reverse();
        }

        // Build cursors from first/last row PKs.
        let start_cursor = rows.first().map(|row| {
            let pairs: Vec<(&str, String)> = pk_cols
                .iter()
                .map(|pk| {
                    let val = row.get(pk.as_str()).map(pg_value_str).unwrap_or_default();
                    (pk.as_str(), val)
                })
                .collect();
            let refs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
            encode_cursor(&refs, &[])
        });
        let end_cursor = rows.last().map(|row| {
            let pairs: Vec<(&str, String)> = pk_cols
                .iter()
                .map(|pk| {
                    let val = row.get(pk.as_str()).map(pg_value_str).unwrap_or_default();
                    (pk.as_str(), val)
                })
                .collect();
            let refs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
            encode_cursor(&refs, &[])
        });

        // Conditional totalCount query.
        let total_count = if args.need_total_count {
            let count_sql = format!(
                "SELECT COUNT(*) FROM \"{}\".\"{}\"",
                resource.schema, resource.table
            );
            let count_rows = conn.execute(&count_sql, &[]).await?;
            count_rows
                .first()
                .and_then(|r| r.get("count"))
                .and_then(|v| match v {
                    PgValue::Int(n) => Some(*n),
                    _ => None,
                })
        } else {
            None
        };

        Ok(ConnectionResult {
            has_next_page: !is_backward && has_more,
            has_previous_page: is_backward && has_more,
            start_cursor,
            end_cursor,
            total_count,
            rows,
        })
    }

    // ── SELECT by PK (xById) ─────────────────────────────────────────────────

    pub async fn select_by_pk(
        &self,
        conn: &RequestConnection,
        resource: &ResolvedResource,
        pk_values: Vec<(&str, PgValue)>,
    ) -> Result<Option<PgRow>, QueryError> {
        let mut builder = SqlBuilder::from(&resource.schema, &resource.table).column_star();
        for (col, val) in pk_values {
            builder = builder.where_clause(
                SqlFragment::ident(col)
                    .push_raw(" = ")
                    .append(SqlFragment::param(val)),
            );
        }
        builder = builder.limit(1);
        let (sql, params) = builder.build().build();
        let rows = conn.execute(&sql, &params).await?;
        Ok(rows.into_iter().next())
    }

    // ── INSERT ────────────────────────────────────────────────────────────────

    pub async fn insert(
        &self,
        conn: &RequestConnection,
        resource: &ResolvedResource,
        columns: Vec<(&str, PgValue)>,
    ) -> Result<PgRow, QueryError> {
        use magna_sql::InsertBuilder;
        let (cols, vals): (Vec<&str>, Vec<PgValue>) = columns.into_iter().unzip();
        let mut b = InsertBuilder::new(&resource.schema, &resource.table);
        for col in &cols {
            b = b.column(col);
        }
        let (sql, params) = b.build(vals).build();
        let rows = conn.execute(&sql, &params).await?;
        rows.into_iter().next().ok_or(QueryError::NotFound)
    }

    // ── UPDATE ────────────────────────────────────────────────────────────────

    pub async fn update(
        &self,
        conn: &RequestConnection,
        resource: &ResolvedResource,
        pk_values: Vec<(&str, PgValue)>,
        set_columns: Vec<(&str, PgValue)>,
    ) -> Result<Option<PgRow>, QueryError> {
        use magna_sql::UpdateBuilder;
        let (set_cols, set_vals): (Vec<&str>, Vec<PgValue>) = set_columns.into_iter().unzip();
        let (pk_cols, pk_vals): (Vec<&str>, Vec<PgValue>) = pk_values.into_iter().unzip();
        let mut b = UpdateBuilder::new(&resource.schema, &resource.table);
        for col in &set_cols {
            b = b.set(col);
        }
        for col in &pk_cols {
            b = b.where_eq(col);
        }
        let (sql, params) = b.build(set_vals, pk_vals).build();
        let rows = conn.execute(&sql, &params).await?;
        Ok(rows.into_iter().next())
    }

    // ── DELETE ────────────────────────────────────────────────────────────────

    pub async fn delete(
        &self,
        conn: &RequestConnection,
        resource: &ResolvedResource,
        pk_values: Vec<(&str, PgValue)>,
    ) -> Result<Option<PgRow>, QueryError> {
        use magna_sql::DeleteBuilder;
        let (pk_cols, pk_vals): (Vec<&str>, Vec<PgValue>) = pk_values.into_iter().unzip();
        let mut b = DeleteBuilder::new(&resource.schema, &resource.table);
        for col in &pk_cols {
            b = b.where_eq(col);
        }
        let (sql, params) = b.build(pk_vals).build();
        let rows = conn.execute(&sql, &params).await?;
        Ok(rows.into_iter().next())
    }

    // ── Batched SELECT for DataLoader (hasMany) ───────────────────────────────

    /// Execute a batched lateral-join SELECT for DataLoader hasMany relations.
    /// Returns a HashMap from parent_id string → Vec<PgRow>.
    pub async fn select_by_fk_batch(
        &self,
        claims: Option<&JwtClaims>,
        resource: &ResolvedResource,
        fk_column: &str,
        lookup_column_oid: u32,
        parent_ids: &[String],
        limit: i64,
    ) -> Result<std::collections::HashMap<String, Vec<PgRow>>, QueryError> {
        if parent_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let pk = resource.primary_key.first().map(|s| s.as_str()).unwrap_or("id");
        let cast = oid_cast(lookup_column_oid);
        let sql = format!(
            r#"SELECT "parent_id_key", sub.* FROM unnest($1::text[]) AS "parent_id_key"
CROSS JOIN LATERAL (
  SELECT * FROM "{schema}"."{table}"
  WHERE "{fk}" = "parent_id_key"{cast}
  ORDER BY "{pk}" ASC
  LIMIT $2
) sub"#,
            schema = resource.schema,
            table  = resource.table,
            fk     = fk_column,
            cast   = cast,
            pk     = pk,
        );

        let mut conn = self.pool.acquire().await.map_err(QueryError::Database)?;
        if let Some(c) = claims {
            apply_rls_context(&mut conn, c)
                .await
                .map_err(|e| QueryError::RlsError(e.to_string()))?;
        }

        let parent_ids_vec: Vec<String> = parent_ids.to_vec();
        let sqlx_rows = sqlx::query(&sql)
            .bind(parent_ids_vec)
            .bind(limit)
            .fetch_all(&mut *conn)
            .await
            .map_err(QueryError::Database)?;
        let rows: Vec<PgRow> = sqlx_rows.iter().map(magna_dataplan::decode_row).collect();

        let mut map: std::collections::HashMap<String, Vec<PgRow>> =
            std::collections::HashMap::new();
        for row in rows {
            if let Some(key) = row.get("parent_id_key").map(pg_value_str) {
                map.entry(key).or_default().push(row);
            }
        }
        Ok(map)
    }

    // ── Batched SELECT for DataLoader (belongsTo) ─────────────────────────────

    /// Execute a batched SELECT for DataLoader belongsTo relations.
    /// Looks up target rows by their PK (one per parent FK value).
    pub async fn select_by_pk_batch(
        &self,
        claims: Option<&JwtClaims>,
        resource: &ResolvedResource,
        pk_column: &str,
        lookup_column_oid: u32,
        ids: &[String],
    ) -> Result<std::collections::HashMap<String, PgRow>, QueryError> {
        if ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let sql = format!(
            r#"SELECT * FROM "{schema}"."{table}" WHERE "{pk}" = ANY($1{cast})"#,
            schema = resource.schema,
            table  = resource.table,
            pk     = pk_column,
            cast   = array_oid_cast(lookup_column_oid),
        );

        let mut conn = self.pool.acquire().await.map_err(QueryError::Database)?;
        if let Some(c) = claims {
            apply_rls_context(&mut conn, c)
                .await
                .map_err(|e| QueryError::RlsError(e.to_string()))?;
        }

        let ids_vec: Vec<String> = ids.to_vec();
        let sqlx_rows = sqlx::query(&sql)
            .bind(ids_vec)
            .fetch_all(&mut *conn)
            .await
            .map_err(QueryError::Database)?;
        let rows: Vec<PgRow> = sqlx_rows.iter().map(magna_dataplan::decode_row).collect();

        let mut map = std::collections::HashMap::new();
        for row in rows {
            if let Some(key) = row.get(pk_column).map(pg_value_str) {
                map.insert(key, row);
            }
        }
        Ok(map)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

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

fn oid_cast(type_oid: u32) -> &'static str {
    match type_oid {
        2950        => "::uuid",
        23 | 20 | 21 => "::bigint",
        _           => "",
    }
}

fn array_oid_cast(type_oid: u32) -> &'static str {
    match type_oid {
        2950        => "::uuid[]",
        23 | 20 | 21 => "::bigint[]",
        _           => "::text[]",
    }
}

fn build_filter_cond(column: &str, op: &FilterOp, val: PgValue) -> SqlFragment {
    let col = SqlFragment::ident(column);
    let p = SqlFragment::param(val.clone());
    match op {
        FilterOp::Eq         => col.push_raw(" = ").append(p),
        FilterOp::Ne         => col.push_raw(" <> ").append(p),
        FilterOp::Lt         => col.push_raw(" < ").append(p),
        FilterOp::Lte        => col.push_raw(" <= ").append(p),
        FilterOp::Gt         => col.push_raw(" > ").append(p),
        FilterOp::Gte        => col.push_raw(" >= ").append(p),
        FilterOp::In         => col.push_raw(" = ANY(").append(p).push_raw(")"),
        FilterOp::Like       => col.push_raw(" LIKE ").append(p),
        FilterOp::Ilike      => col.push_raw(" ILIKE ").append(p),
        FilterOp::StartsWith => col.push_raw(" LIKE ").append(SqlFragment::param(PgValue::Text(
            match val {
                PgValue::Text(s) => format!("{}%", s),
                _                => String::new(),
            },
        ))),
        FilterOp::IsNull => match val {
            PgValue::Bool(true) => col.push_raw(" IS NULL"),
            _                   => col.push_raw(" IS NOT NULL"),
        },
    }
}
