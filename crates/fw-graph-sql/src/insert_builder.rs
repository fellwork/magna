//! InsertBuilder — builds parameterized INSERT ... RETURNING * queries.

use fw_graph_types::PgValue;
use crate::fragment::SqlFragment;

/// Build an `INSERT INTO "schema"."table" ("col1", ...) VALUES ($1, ...) RETURNING *` query.
#[derive(Debug, Clone)]
pub struct InsertBuilder {
    schema: String,
    table: String,
    columns: Vec<String>,
}

impl InsertBuilder {
    pub fn new(schema: &str, table: &str) -> Self {
        Self {
            schema: schema.to_string(),
            table: table.to_string(),
            columns: Vec::new(),
        }
    }

    /// Add a column name (value order must match column order).
    pub fn column(mut self, name: &str) -> Self {
        self.columns.push(name.to_string());
        self
    }

    /// Build into a `SqlFragment`. The caller provides values in the same
    /// order as the columns were added via `.column()`.
    pub fn build(self, values: Vec<PgValue>) -> SqlFragment {
        assert_eq!(
            self.columns.len(),
            values.len(),
            "InsertBuilder: column count {} != value count {}",
            self.columns.len(),
            values.len()
        );

        let mut f = SqlFragment::empty();
        f = f.push_raw("INSERT INTO ");
        f = f.append(SqlFragment::qualified_ident(&self.schema, &self.table));
        f = f.push_raw(" (");

        for (i, col) in self.columns.iter().enumerate() {
            if i > 0 {
                f = f.push_raw(", ");
            }
            f = f.append(SqlFragment::ident(col));
        }

        f = f.push_raw(") VALUES (");

        for (i, val) in values.into_iter().enumerate() {
            if i > 0 {
                f = f.push_raw(", ");
            }
            f = f.append(SqlFragment::param(val));
        }

        f = f.push_raw(") RETURNING *");
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_single_column() {
        let frag = InsertBuilder::new("public", "users")
            .column("email")
            .build(vec![PgValue::Text("a@b.com".to_string())]);
        let (sql, params) = frag.build();
        assert_eq!(sql, "INSERT INTO \"public\".\"users\" (\"email\") VALUES ($1) RETURNING *");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].as_text(), Some("a@b.com"));
    }

    #[test]
    fn test_insert_multi_column() {
        let frag = InsertBuilder::new("public", "users")
            .column("email")
            .column("name")
            .build(vec![
                PgValue::Text("a@b.com".to_string()),
                PgValue::Text("Alice".to_string()),
            ]);
        let (sql, params) = frag.build();
        assert!(sql.contains("\"email\", \"name\""));
        assert!(sql.contains("$1, $2"));
        assert!(sql.contains("RETURNING *"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_insert_values_are_parameterized() {
        let evil = "Robert'); DROP TABLE users; --";
        let frag = InsertBuilder::new("public", "users")
            .column("name")
            .build(vec![PgValue::Text(evil.to_string())]);
        let (sql, params) = frag.build();
        assert!(!sql.contains(evil), "evil input must not appear in SQL");
        assert_eq!(params[0].as_text(), Some(evil));
    }
}
