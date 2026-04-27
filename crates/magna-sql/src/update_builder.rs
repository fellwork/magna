//! UpdateBuilder — builds parameterized UPDATE ... SET ... WHERE ... RETURNING * queries.

use magna_types::PgValue;
use crate::fragment::SqlFragment;

/// Build `UPDATE "schema"."table" SET "col1"=$1, ... WHERE "pk_col"=$N RETURNING *`.
#[derive(Debug, Clone)]
pub struct UpdateBuilder {
    schema: String,
    table: String,
    set_columns: Vec<String>,
    where_columns: Vec<String>,
}

impl UpdateBuilder {
    pub fn new(schema: &str, table: &str) -> Self {
        Self {
            schema: schema.to_string(),
            table: table.to_string(),
            set_columns: Vec::new(),
            where_columns: Vec::new(),
        }
    }

    /// Add a SET column (value provided at build time).
    pub fn set(mut self, column: &str) -> Self {
        self.set_columns.push(column.to_string());
        self
    }

    /// Add a WHERE col = $N condition (value provided at build time, after set values).
    pub fn where_eq(mut self, column: &str) -> Self {
        self.where_columns.push(column.to_string());
        self
    }

    /// Build the fragment. `set_values` correspond to `.set()` columns (in order),
    /// `where_values` correspond to `.where_eq()` columns (in order).
    pub fn build(self, set_values: Vec<PgValue>, where_values: Vec<PgValue>) -> SqlFragment {
        assert_eq!(self.set_columns.len(), set_values.len());
        assert_eq!(self.where_columns.len(), where_values.len());

        let mut f = SqlFragment::empty();
        f = f.push_raw("UPDATE ");
        f = f.append(SqlFragment::qualified_ident(&self.schema, &self.table));
        f = f.push_raw(" SET ");

        for (i, (col, val)) in self.set_columns.iter().zip(set_values.into_iter()).enumerate() {
            if i > 0 {
                f = f.push_raw(", ");
            }
            f = f.append(SqlFragment::ident(col));
            f = f.push_raw(" = ");
            f = f.append(SqlFragment::param(val));
        }

        f = f.push_raw(" WHERE ");
        for (i, (col, val)) in self.where_columns.iter().zip(where_values.into_iter()).enumerate() {
            if i > 0 {
                f = f.push_raw(" AND ");
            }
            f = f.append(SqlFragment::ident(col));
            f = f.push_raw(" = ");
            f = f.append(SqlFragment::param(val));
        }

        f = f.push_raw(" RETURNING *");
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_single_set_single_where() {
        let frag = UpdateBuilder::new("public", "users")
            .set("email")
            .where_eq("id")
            .build(
                vec![PgValue::Text("new@example.com".to_string())],
                vec![PgValue::Int(42)],
            );
        let (sql, params) = frag.build();
        assert_eq!(
            sql,
            "UPDATE \"public\".\"users\" SET \"email\" = $1 WHERE \"id\" = $2 RETURNING *"
        );
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].as_text(), Some("new@example.com"));
        assert_eq!(params[1].as_i64(), Some(42));
    }

    #[test]
    fn test_update_multi_set() {
        let frag = UpdateBuilder::new("public", "users")
            .set("email")
            .set("name")
            .where_eq("id")
            .build(
                vec![PgValue::Text("a@b.com".to_string()), PgValue::Text("Bob".to_string())],
                vec![PgValue::Int(1)],
            );
        let (sql, _) = frag.build();
        assert!(sql.contains("\"email\" = $1, \"name\" = $2"));
        assert!(sql.contains("WHERE \"id\" = $3"));
    }

    #[test]
    fn test_update_composite_pk() {
        let frag = UpdateBuilder::new("public", "order_items")
            .set("qty")
            .where_eq("order_id")
            .where_eq("item_id")
            .build(
                vec![PgValue::Int(5)],
                vec![PgValue::Int(10), PgValue::Int(20)],
            );
        let (sql, _) = frag.build();
        assert!(sql.contains("WHERE \"order_id\" = $2 AND \"item_id\" = $3"));
    }
}
