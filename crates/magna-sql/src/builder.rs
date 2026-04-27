//! SqlBuilder — a composable SELECT query builder.
//!
//! Constructs parameterized Postgres SELECT queries using [`SqlFragment`] as
//! the internal IR. All user-provided values are parameterized (`$1`, `$2`, ...)
//! and NEVER interpolated into the query string.

use magna_types::PgValue;
use crate::fragment::SqlFragment;

// ---------------------------------------------------------------------------
// JoinType
// ---------------------------------------------------------------------------

/// The type of SQL JOIN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
}

impl JoinType {
    fn as_sql(&self) -> &'static str {
        match self {
            JoinType::Inner => "INNER JOIN",
            JoinType::Left  => "LEFT JOIN",
            JoinType::Right => "RIGHT JOIN",
            JoinType::Full  => "FULL JOIN",
        }
    }
}

// ---------------------------------------------------------------------------
// JoinClause
// ---------------------------------------------------------------------------

/// A single JOIN clause to be added to a query.
#[derive(Debug, Clone)]
pub struct JoinClause {
    pub join_type: JoinType,
    /// The table being joined (as a fragment, typically a qualified ident).
    pub table: SqlFragment,
    /// Optional alias for the joined table.
    pub alias: Option<String>,
    /// The ON condition.
    pub on: SqlFragment,
}

impl JoinClause {
    /// Create a new JOIN clause.
    pub fn new(join_type: JoinType, table: SqlFragment, on: SqlFragment) -> Self {
        Self {
            join_type,
            table,
            alias: None,
            on,
        }
    }

    /// Set an alias for the joined table.
    pub fn alias(mut self, alias: &str) -> Self {
        self.alias = Some(alias.to_string());
        self
    }
}

// ---------------------------------------------------------------------------
// OrderByItem
// ---------------------------------------------------------------------------

/// A single ORDER BY element.
#[derive(Debug, Clone)]
struct OrderByItem {
    column: SqlFragment,
    ascending: bool,
}

// ---------------------------------------------------------------------------
// ColumnExpr
// ---------------------------------------------------------------------------

/// A column expression with an optional alias.
#[derive(Debug, Clone)]
struct ColumnExpr {
    expr: SqlFragment,
    alias: Option<String>,
}

// ---------------------------------------------------------------------------
// SqlBuilder
// ---------------------------------------------------------------------------

/// A composable SELECT query builder.
///
/// # Safety
///
/// All user-provided values MUST go through [`SqlFragment::param`].
/// Table and column names are always double-quoted via [`SqlFragment::ident`].
/// The builder **never** interpolates raw values into the SQL string.
///
/// # Example
///
/// ```
/// use magna_sql::builder::{SqlBuilder, JoinType, JoinClause};
/// use magna_sql::fragment::SqlFragment;
/// use magna_types::PgValue;
///
/// let query = SqlBuilder::from("public", "users")
///     .column(SqlFragment::ident("id"), None)
///     .column(SqlFragment::ident("name"), Some("user_name"))
///     .where_clause(
///         SqlFragment::ident("active")
///             .push_raw(" = ")
///             .append(SqlFragment::param(PgValue::Bool(true)))
///     )
///     .order_by(SqlFragment::ident("name"), true)
///     .limit(10)
///     .offset(0)
///     .build();
///
/// let (sql, params) = query.build();
/// ```
#[derive(Debug, Clone)]
pub struct SqlBuilder {
    schema: String,
    table: String,
    table_alias: Option<String>,
    columns: Vec<ColumnExpr>,
    joins: Vec<JoinClause>,
    where_conditions: Vec<SqlFragment>,
    order_by: Vec<OrderByItem>,
    limit_val: Option<i64>,
    offset_val: Option<i64>,
}

impl SqlBuilder {
    // -- constructors -------------------------------------------------------

    /// Begin building a SELECT from `"schema"."table"`.
    pub fn from(schema: &str, table: &str) -> Self {
        Self {
            schema: schema.to_string(),
            table: table.to_string(),
            table_alias: None,
            columns: Vec::new(),
            joins: Vec::new(),
            where_conditions: Vec::new(),
            order_by: Vec::new(),
            limit_val: None,
            offset_val: None,
        }
    }

    // -- table alias --------------------------------------------------------

    /// Set an alias for the FROM table.
    pub fn table_alias(mut self, alias: &str) -> Self {
        self.table_alias = Some(alias.to_string());
        self
    }

    // -- columns ------------------------------------------------------------

    /// Add a column expression to the SELECT list.
    /// Pass `alias` to emit `expr AS "alias"`.
    pub fn column(mut self, expr: SqlFragment, alias: Option<&str>) -> Self {
        self.columns.push(ColumnExpr {
            expr,
            alias: alias.map(|a| a.to_string()),
        });
        self
    }

    /// Convenience: select all columns (`*`).
    pub fn column_star(mut self) -> Self {
        self.columns.push(ColumnExpr {
            expr: SqlFragment::raw("*"),
            alias: None,
        });
        self
    }

    // -- WHERE --------------------------------------------------------------

    /// Add a WHERE condition. Multiple calls are combined with AND.
    pub fn where_clause(mut self, cond: SqlFragment) -> Self {
        self.where_conditions.push(cond);
        self
    }

    /// Alias for `where_clause` for ergonomic chaining.
    pub fn and_where(self, cond: SqlFragment) -> Self {
        self.where_clause(cond)
    }

    // -- JOINs --------------------------------------------------------------

    /// Add a JOIN clause.
    pub fn join(mut self, clause: JoinClause) -> Self {
        self.joins.push(clause);
        self
    }

    // -- ORDER BY -----------------------------------------------------------

    /// Add an ORDER BY column. `ascending = true` for ASC, `false` for DESC.
    pub fn order_by(mut self, col: SqlFragment, ascending: bool) -> Self {
        self.order_by.push(OrderByItem {
            column: col,
            ascending,
        });
        self
    }

    // -- LIMIT / OFFSET -----------------------------------------------------

    /// Set the LIMIT clause.
    pub fn limit(mut self, n: i64) -> Self {
        self.limit_val = Some(n);
        self
    }

    /// Set the OFFSET clause.
    pub fn offset(mut self, n: i64) -> Self {
        self.offset_val = Some(n);
        self
    }

    // -- build --------------------------------------------------------------

    /// Consume the builder and produce a [`SqlFragment`] representing the full
    /// SELECT query. Call `.build()` on the result to get the final
    /// `(String, Vec<PgValue>)`.
    pub fn build(self) -> SqlFragment {
        let mut result = SqlFragment::empty();

        // SELECT columns
        result = result.push_raw("SELECT ");
        if self.columns.is_empty() {
            result = result.push_raw("*");
        } else {
            for (i, col) in self.columns.iter().enumerate() {
                if i > 0 {
                    result = result.push_raw(", ");
                }
                result = result.append(col.expr.clone());
                if let Some(alias) = &col.alias {
                    result = result.push_raw(" AS ");
                    result = result.append(SqlFragment::ident(alias));
                }
            }
        }

        // FROM
        result = result.push_raw(" FROM ");
        result = result.append(SqlFragment::qualified_ident(&self.schema, &self.table));
        if let Some(alias) = &self.table_alias {
            result = result.push_raw(" AS ");
            result = result.append(SqlFragment::ident(alias));
        }

        // JOINs
        for j in &self.joins {
            result = result.push_raw(" ");
            result = result.push_raw(j.join_type.as_sql());
            result = result.push_raw(" ");
            result = result.append(j.table.clone());
            if let Some(alias) = &j.alias {
                result = result.push_raw(" AS ");
                result = result.append(SqlFragment::ident(alias));
            }
            result = result.push_raw(" ON ");
            result = result.append(j.on.clone());
        }

        // WHERE
        if !self.where_conditions.is_empty() {
            result = result.push_raw(" WHERE ");
            for (i, cond) in self.where_conditions.into_iter().enumerate() {
                if i > 0 {
                    result = result.push_raw(" AND ");
                }
                // Wrap each condition in parens for safety.
                result = result.push_raw("(");
                result = result.append(cond);
                result = result.push_raw(")");
            }
        }

        // ORDER BY
        if !self.order_by.is_empty() {
            result = result.push_raw(" ORDER BY ");
            for (i, item) in self.order_by.iter().enumerate() {
                if i > 0 {
                    result = result.push_raw(", ");
                }
                result = result.append(item.column.clone());
                if item.ascending {
                    result = result.push_raw(" ASC");
                } else {
                    result = result.push_raw(" DESC");
                }
            }
        }

        // LIMIT
        if let Some(n) = self.limit_val {
            result = result.push_raw(" LIMIT ");
            result = result.append(SqlFragment::param(PgValue::Int(n)));
        }

        // OFFSET
        if let Some(n) = self.offset_val {
            result = result.push_raw(" OFFSET ");
            result = result.append(SqlFragment::param(PgValue::Int(n)));
        }

        result
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_select_with_columns() {
        let query = SqlBuilder::from("public", "users")
            .column(SqlFragment::ident("id"), None)
            .column(SqlFragment::ident("name"), Some("user_name"))
            .build();
        let (sql, params) = query.build();
        assert_eq!(
            sql,
            "SELECT \"id\", \"name\" AS \"user_name\" FROM \"public\".\"users\""
        );
        assert!(params.is_empty());
    }

    #[test]
    fn select_star_when_no_columns() {
        let query = SqlBuilder::from("public", "users").build();
        let (sql, _) = query.build();
        assert_eq!(sql, "SELECT * FROM \"public\".\"users\"");
    }

    #[test]
    fn select_star_explicit() {
        let query = SqlBuilder::from("public", "users")
            .column_star()
            .build();
        let (sql, _) = query.build();
        assert_eq!(sql, "SELECT * FROM \"public\".\"users\"");
    }

    #[test]
    fn where_conditions_with_params() {
        let query = SqlBuilder::from("public", "users")
            .column(SqlFragment::ident("id"), None)
            .where_clause(
                SqlFragment::ident("active")
                    .push_raw(" = ")
                    .append(SqlFragment::param(PgValue::Bool(true))),
            )
            .where_clause(
                SqlFragment::ident("age")
                    .push_raw(" > ")
                    .append(SqlFragment::param(PgValue::Int(18))),
            )
            .build();
        let (sql, params) = query.build();
        assert_eq!(
            sql,
            "SELECT \"id\" FROM \"public\".\"users\" WHERE (\"active\" = $1) AND (\"age\" > $2)"
        );
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].as_bool(), Some(true));
        assert_eq!(params[1].as_i64(), Some(18));
    }

    #[test]
    fn inner_join() {
        let on_cond = SqlFragment::ident("u")
            .push_raw(".")
            .append(SqlFragment::ident("id"))
            .push_raw(" = ")
            .append(SqlFragment::ident("p"))
            .push_raw(".")
            .append(SqlFragment::ident("user_id"));

        let query = SqlBuilder::from("public", "users")
            .table_alias("u")
            .column(SqlFragment::raw("u.*"), None)
            .join(
                JoinClause::new(
                    JoinType::Inner,
                    SqlFragment::qualified_ident("public", "posts"),
                    on_cond,
                )
                .alias("p"),
            )
            .build();
        let (sql, params) = query.build();
        assert_eq!(
            sql,
            "SELECT u.* FROM \"public\".\"users\" AS \"u\" \
             INNER JOIN \"public\".\"posts\" AS \"p\" \
             ON \"u\".\"id\" = \"p\".\"user_id\""
        );
        assert!(params.is_empty());
    }

    #[test]
    fn left_join() {
        let on_cond = SqlFragment::ident("users")
            .push_raw(".")
            .append(SqlFragment::ident("id"))
            .push_raw(" = ")
            .append(SqlFragment::ident("orders"))
            .push_raw(".")
            .append(SqlFragment::ident("user_id"));

        let query = SqlBuilder::from("public", "users")
            .column(SqlFragment::ident("id"), None)
            .join(JoinClause::new(
                JoinType::Left,
                SqlFragment::qualified_ident("public", "orders"),
                on_cond,
            ))
            .build();
        let (sql, _) = query.build();
        assert!(sql.contains("LEFT JOIN"));
        assert!(sql.contains("\"public\".\"orders\""));
    }

    #[test]
    fn order_by_limit_offset() {
        let query = SqlBuilder::from("public", "users")
            .column(SqlFragment::ident("id"), None)
            .order_by(SqlFragment::ident("name"), true)
            .order_by(SqlFragment::ident("id"), false)
            .limit(25)
            .offset(50)
            .build();
        let (sql, params) = query.build();
        assert_eq!(
            sql,
            "SELECT \"id\" FROM \"public\".\"users\" \
             ORDER BY \"name\" ASC, \"id\" DESC \
             LIMIT $1 OFFSET $2"
        );
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].as_i64(), Some(25));
        assert_eq!(params[1].as_i64(), Some(50));
    }

    #[test]
    fn sql_injection_prevention_values_always_parameterized() {
        // Simulate a malicious user value.
        let evil_input = "Robert'; DROP TABLE students;--";

        let query = SqlBuilder::from("public", "users")
            .column(SqlFragment::ident("id"), None)
            .where_clause(
                SqlFragment::ident("name")
                    .push_raw(" = ")
                    .append(SqlFragment::param(PgValue::Text(evil_input.to_string()))),
            )
            .build();
        let (sql, params) = query.build();

        // The evil string must NOT appear in the SQL — only as a parameter.
        assert!(!sql.contains(evil_input));
        assert!(sql.contains("$1"));
        assert_eq!(params[0].as_text(), Some(evil_input));
    }

    #[test]
    fn sql_injection_prevention_identifier_quotes_escaped() {
        // Even if someone passes a crafted identifier, embedded quotes are doubled.
        let evil_ident = "col\"; DROP TABLE users; --";
        let f = SqlFragment::ident(evil_ident);
        let (sql, _) = f.build();
        // The output should be: "col""; DROP TABLE users; --"
        // The embedded " is doubled, so the identifier boundary is never broken.
        assert!(sql.starts_with('"'));
        assert!(sql.ends_with('"'));
        // The critical security property: the embedded quote is doubled (""),
        // which means Postgres treats it as a literal quote inside the identifier
        // rather than a closing delimiter. Verify the doubling happened.
        assert_eq!(sql, "\"col\"\"; DROP TABLE users; --\"");
        // If we strip the outer quotes and un-double internal quotes, we get
        // the original string back — proving nothing escaped.
        let inner = &sql[1..sql.len() - 1];
        let unescaped = inner.replace("\"\"", "\"");
        assert_eq!(unescaped, evil_ident);
    }

    #[test]
    fn complex_query_with_joins_where_order_limit() {
        // Build: SELECT u."id", u."name", p."title"
        //        FROM "public"."users" AS "u"
        //        INNER JOIN "public"."posts" AS "p" ON "u"."id" = "p"."author_id"
        //        LEFT JOIN "public"."comments" AS "c" ON "p"."id" = "c"."post_id"
        //        WHERE ("u"."active" = $1) AND ("p"."published" = $2)
        //        ORDER BY "p"."created_at" DESC, "u"."name" ASC
        //        LIMIT $3 OFFSET $4

        let post_join_on = SqlFragment::ident("u")
            .push_raw(".")
            .append(SqlFragment::ident("id"))
            .push_raw(" = ")
            .append(SqlFragment::ident("p"))
            .push_raw(".")
            .append(SqlFragment::ident("author_id"));

        let comment_join_on = SqlFragment::ident("p")
            .push_raw(".")
            .append(SqlFragment::ident("id"))
            .push_raw(" = ")
            .append(SqlFragment::ident("c"))
            .push_raw(".")
            .append(SqlFragment::ident("post_id"));

        let query = SqlBuilder::from("public", "users")
            .table_alias("u")
            .column(
                SqlFragment::raw("u.").append(SqlFragment::ident("id")),
                None,
            )
            .column(
                SqlFragment::raw("u.").append(SqlFragment::ident("name")),
                None,
            )
            .column(
                SqlFragment::raw("p.").append(SqlFragment::ident("title")),
                None,
            )
            .join(
                JoinClause::new(
                    JoinType::Inner,
                    SqlFragment::qualified_ident("public", "posts"),
                    post_join_on,
                )
                .alias("p"),
            )
            .join(
                JoinClause::new(
                    JoinType::Left,
                    SqlFragment::qualified_ident("public", "comments"),
                    comment_join_on,
                )
                .alias("c"),
            )
            .where_clause(
                SqlFragment::ident("u")
                    .push_raw(".")
                    .append(SqlFragment::ident("active"))
                    .push_raw(" = ")
                    .append(SqlFragment::param(PgValue::Bool(true))),
            )
            .and_where(
                SqlFragment::ident("p")
                    .push_raw(".")
                    .append(SqlFragment::ident("published"))
                    .push_raw(" = ")
                    .append(SqlFragment::param(PgValue::Bool(true))),
            )
            .order_by(
                SqlFragment::raw("p.").append(SqlFragment::ident("created_at")),
                false,
            )
            .order_by(
                SqlFragment::raw("u.").append(SqlFragment::ident("name")),
                true,
            )
            .limit(20)
            .offset(40)
            .build();

        let (sql, params) = query.build();

        // Verify the full SQL structure.
        assert!(sql.starts_with("SELECT u.\"id\", u.\"name\", p.\"title\" FROM \"public\".\"users\" AS \"u\""));
        assert!(sql.contains("INNER JOIN \"public\".\"posts\" AS \"p\" ON \"u\".\"id\" = \"p\".\"author_id\""));
        assert!(sql.contains("LEFT JOIN \"public\".\"comments\" AS \"c\" ON \"p\".\"id\" = \"c\".\"post_id\""));
        assert!(sql.contains("WHERE (\"u\".\"active\" = $1) AND (\"p\".\"published\" = $2)"));
        assert!(sql.contains("ORDER BY p.\"created_at\" DESC, u.\"name\" ASC"));
        assert!(sql.contains("LIMIT $3 OFFSET $4"));

        // Verify all four params are present and correctly ordered.
        assert_eq!(params.len(), 4);
        assert_eq!(params[0].as_bool(), Some(true));
        assert_eq!(params[1].as_bool(), Some(true));
        assert_eq!(params[2].as_i64(), Some(20));
        assert_eq!(params[3].as_i64(), Some(40));
    }

    #[test]
    fn table_alias_emitted() {
        let query = SqlBuilder::from("app", "accounts")
            .table_alias("a")
            .column(SqlFragment::ident("id"), None)
            .build();
        let (sql, _) = query.build();
        assert_eq!(
            sql,
            "SELECT \"id\" FROM \"app\".\"accounts\" AS \"a\""
        );
    }
}
