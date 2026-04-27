//! DeleteBuilder — builds parameterized DELETE ... WHERE ... RETURNING * queries.

use magna_types::PgValue;
use crate::fragment::SqlFragment;

/// Build `DELETE FROM "schema"."table" WHERE "pk_col"=$1 RETURNING *`.
#[derive(Debug, Clone)]
pub struct DeleteBuilder {
    schema: String,
    table: String,
    where_columns: Vec<String>,
}

impl DeleteBuilder {
    pub fn new(schema: &str, table: &str) -> Self {
        Self {
            schema: schema.to_string(),
            table: table.to_string(),
            where_columns: Vec::new(),
        }
    }

    pub fn where_eq(mut self, column: &str) -> Self {
        self.where_columns.push(column.to_string());
        self
    }

    pub fn build(self, where_values: Vec<PgValue>) -> SqlFragment {
        assert_eq!(self.where_columns.len(), where_values.len());

        let mut f = SqlFragment::empty();
        f = f.push_raw("DELETE FROM ");
        f = f.append(SqlFragment::qualified_ident(&self.schema, &self.table));
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
    fn test_delete_single_pk() {
        let frag = DeleteBuilder::new("public", "users")
            .where_eq("id")
            .build(vec![PgValue::Int(42)]);
        let (sql, params) = frag.build();
        assert_eq!(sql, "DELETE FROM \"public\".\"users\" WHERE \"id\" = $1 RETURNING *");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].as_i64(), Some(42));
    }

    #[test]
    fn test_delete_composite_pk() {
        let frag = DeleteBuilder::new("public", "order_items")
            .where_eq("order_id")
            .where_eq("item_id")
            .build(vec![PgValue::Int(10), PgValue::Int(20)]);
        let (sql, _) = frag.build();
        assert!(sql.contains("WHERE \"order_id\" = $1 AND \"item_id\" = $2"));
    }

    #[test]
    fn test_delete_text_pk() {
        let frag = DeleteBuilder::new("public", "sessions")
            .where_eq("token")
            .build(vec![PgValue::Text("abc123".to_string())]);
        let (sql, params) = frag.build();
        assert!(sql.contains("RETURNING *"));
        assert_eq!(params.len(), 1);
    }
}
