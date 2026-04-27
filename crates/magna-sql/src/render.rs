//! Rendering utilities for [`SqlFragment`] and [`SqlBuilder`].
//!
//! The primary rendering logic lives in [`SqlFragment::build`], which produces
//! the final `(String, Vec<PgValue>)` tuple. This module provides additional
//! convenience functions and formatting helpers.

use magna_types::PgValue;
use crate::fragment::SqlFragment;
use crate::builder::SqlBuilder;

/// Render a [`SqlBuilder`] directly into a `(query_string, params)` tuple.
///
/// This is a convenience wrapper that calls `builder.build().build()`:
/// first the builder produces a [`SqlFragment`], then the fragment is
/// flattened into the final SQL string with numbered `$N` parameters.
///
/// # Example
///
/// ```
/// use magna_sql::builder::SqlBuilder;
/// use magna_sql::fragment::SqlFragment;
/// use magna_sql::render;
///
/// let builder = SqlBuilder::from("public", "users")
///     .column(SqlFragment::ident("id"), None);
/// let (sql, params) = render::render(builder);
/// assert_eq!(sql, "SELECT \"id\" FROM \"public\".\"users\"");
/// ```
pub fn render(builder: SqlBuilder) -> (String, Vec<PgValue>) {
    builder.build().build()
}

/// Render a [`SqlFragment`] directly into a `(query_string, params)` tuple.
///
/// This is just `fragment.build()` but provided for naming symmetry.
pub fn render_fragment(fragment: SqlFragment) -> (String, Vec<PgValue>) {
    fragment.build()
}

/// Format a rendered query for debug logging. Parameter values are shown
/// inline as comments so the output is never executable — this is safe to
/// log because it cannot be copy-pasted into a SQL client.
///
/// # Example output
///
/// ```text
/// SELECT "id" FROM "public"."users" WHERE "active" = $1 /* Bool(true) */ LIMIT $2 /* Int(10) */
/// ```
pub fn debug_format(sql: &str, params: &[PgValue]) -> String {
    let mut out = sql.to_string();
    // Walk backwards so earlier replacements don't shift later indices.
    for (i, param) in params.iter().enumerate().rev() {
        let placeholder = format!("${}", i + 1);
        let annotated = format!("{} /* {:?} */", placeholder, param);
        out = out.replace(&placeholder, &annotated);
    }
    out
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_convenience_function() {
        let builder = SqlBuilder::from("public", "users")
            .column(SqlFragment::ident("id"), None)
            .column(SqlFragment::ident("email"), None);
        let (sql, params) = render(builder);
        assert_eq!(sql, "SELECT \"id\", \"email\" FROM \"public\".\"users\"");
        assert!(params.is_empty());
    }

    #[test]
    fn render_fragment_function() {
        let frag = SqlFragment::raw("SELECT 1");
        let (sql, params) = render_fragment(frag);
        assert_eq!(sql, "SELECT 1");
        assert!(params.is_empty());
    }

    #[test]
    fn debug_format_annotates_params() {
        let sql = "SELECT * FROM t WHERE id = $1 AND name = $2";
        let params = vec![PgValue::Int(42), PgValue::Text("alice".into())];
        let out = debug_format(sql, &params);
        assert!(out.contains("$1 /* Int(42) */"));
        assert!(out.contains("$2 /* Text(\"alice\") */"));
        // The original placeholders are still present (as part of the annotated form).
        assert!(out.contains("$1"));
        assert!(out.contains("$2"));
    }
}
