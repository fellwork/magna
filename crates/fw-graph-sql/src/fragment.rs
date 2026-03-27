//! SqlFragment — the internal intermediate representation for composable SQL.
//!
//! A fragment is a tree of SQL parts that can be composed without worrying about
//! parameter numbering. The `$N` placeholders are only assigned at build time,
//! once the full fragment tree is known.

use fw_graph_types::PgValue;

// ---------------------------------------------------------------------------
// SqlPart — a single node in the fragment tree
// ---------------------------------------------------------------------------

/// One element within a [`SqlFragment`]. These are accumulated in order
/// and flattened into a final SQL string at build time.
#[derive(Debug, Clone)]
pub enum SqlPart {
    /// A literal SQL string (keywords, operators, punctuation).
    /// MUST NOT contain user data — use [`SqlPart::Param`] for that.
    Literal(String),

    /// Index into the owning fragment's `params` vec.
    /// NOT the `$N` number — that is assigned during [`SqlFragment::build`].
    Param(usize),

    /// A nested fragment, inlined recursively at build time.
    Fragment(Box<SqlFragment>),

    /// A safely double-quoted SQL identifier (schema, table, column name).
    Identifier(String),
}

// ---------------------------------------------------------------------------
// SqlFragment
// ---------------------------------------------------------------------------

/// A composable SQL fragment with typed parameters.
///
/// Parameters are stored as values, not yet assigned `$N` numbers.
/// `$N` assignment happens only at [`build()`](SqlFragment::build) time,
/// once the full fragment tree is known.
#[derive(Debug, Clone)]
pub struct SqlFragment {
    /// Alternating sequence of literal SQL strings and parameter slots.
    pub(crate) parts: Vec<SqlPart>,
    /// The actual parameter values, indexed by `Param(i)`.
    pub(crate) params: Vec<PgValue>,
}

impl SqlFragment {
    // -- constructors -------------------------------------------------------

    /// Create a fragment containing a single raw SQL literal.
    ///
    /// **Safety contract**: callers MUST NOT pass user-supplied data here.
    /// Use [`SqlFragment::param`] for user values.
    pub fn raw(s: &str) -> Self {
        Self {
            parts: vec![SqlPart::Literal(s.to_string())],
            params: Vec::new(),
        }
    }

    /// Create a fragment containing a single parameterized value.
    /// The value will be emitted as `$N` at build time.
    pub fn param(v: PgValue) -> Self {
        Self {
            parts: vec![SqlPart::Param(0)],
            params: vec![v],
        }
    }

    /// Create a fragment containing a safely double-quoted identifier.
    /// Any embedded double quotes are escaped by doubling them.
    pub fn ident(name: &str) -> Self {
        Self {
            parts: vec![SqlPart::Identifier(name.to_string())],
            params: Vec::new(),
        }
    }

    /// Create a qualified identifier: `"schema"."name"`.
    pub fn qualified_ident(schema: &str, name: &str) -> Self {
        Self {
            parts: vec![
                SqlPart::Identifier(schema.to_string()),
                SqlPart::Literal(".".to_string()),
                SqlPart::Identifier(name.to_string()),
            ],
            params: Vec::new(),
        }
    }

    /// Create an empty fragment (no parts, no params).
    pub fn empty() -> Self {
        Self {
            parts: Vec::new(),
            params: Vec::new(),
        }
    }

    // -- composition --------------------------------------------------------

    /// Append another fragment after `self`, separated by `separator`.
    ///
    /// The `other` fragment's param indices are shifted by `self.params.len()`
    /// so there are no collisions.
    pub fn join(mut self, separator: &str, other: SqlFragment) -> Self {
        if !self.parts.is_empty() && !other.parts.is_empty() {
            self.parts.push(SqlPart::Literal(separator.to_string()));
        }
        let offset = self.params.len();
        let shifted = shift_parts(other.parts, offset);
        self.parts.extend(shifted);
        self.params.extend(other.params);
        self
    }

    /// Append another fragment directly after `self` (no separator).
    pub fn append(mut self, other: SqlFragment) -> Self {
        let offset = self.params.len();
        let shifted = shift_parts(other.parts, offset);
        self.parts.extend(shifted);
        self.params.extend(other.params);
        self
    }

    /// Append a raw literal string directly.
    pub fn push_raw(mut self, s: &str) -> Self {
        self.parts.push(SqlPart::Literal(s.to_string()));
        self
    }

    // -- build --------------------------------------------------------------

    /// Flatten the fragment tree into a final SQL string and parameter list.
    ///
    /// This is the **only** place where `$N` numbers are assigned.
    /// Called once per request, just before handing to sqlx.
    pub fn build(self) -> (String, Vec<PgValue>) {
        let mut sql = String::new();
        let mut params = Vec::new();
        let mut counter: usize = 1;
        build_inner(&self.parts, &self.params, &mut sql, &mut params, &mut counter);
        (sql, params)
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Shift all `Param(i)` indices in `parts` by `offset`.
fn shift_parts(parts: Vec<SqlPart>, offset: usize) -> Vec<SqlPart> {
    if offset == 0 {
        return parts;
    }
    parts
        .into_iter()
        .map(|p| match p {
            SqlPart::Param(i) => SqlPart::Param(i + offset),
            SqlPart::Fragment(f) => SqlPart::Fragment(Box::new(shift_fragment(*f, offset))),
            other => other,
        })
        .collect()
}

/// Recursively shift param indices within a nested fragment.
fn shift_fragment(mut frag: SqlFragment, offset: usize) -> SqlFragment {
    frag.parts = shift_parts(frag.parts, offset);
    frag
}

/// Recursively flatten parts into the output SQL string and params vec.
fn build_inner(
    parts: &[SqlPart],
    param_store: &[PgValue],
    sql: &mut String,
    params: &mut Vec<PgValue>,
    counter: &mut usize,
) {
    for part in parts {
        match part {
            SqlPart::Literal(s) => sql.push_str(s),
            SqlPart::Identifier(s) => {
                sql.push('"');
                // Escape embedded double quotes by doubling them.
                sql.push_str(&s.replace('"', "\"\""));
                sql.push('"');
            }
            SqlPart::Param(i) => {
                sql.push('$');
                sql.push_str(&counter.to_string());
                *counter += 1;
                params.push(param_store[*i].clone());
            }
            SqlPart::Fragment(f) => {
                build_inner(&f.parts, &f.params, sql, params, counter);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Convenience free functions (re-exported from lib.rs)
// ---------------------------------------------------------------------------

/// Create a raw SQL literal fragment.
/// **Safety**: do NOT pass user data here — use [`param`] instead.
pub fn raw(s: &str) -> SqlFragment {
    SqlFragment::raw(s)
}

/// Create a parameterized value fragment.
pub fn param(v: PgValue) -> SqlFragment {
    SqlFragment::param(v)
}

/// Create a safely double-quoted identifier fragment.
pub fn ident(name: &str) -> SqlFragment {
    SqlFragment::ident(name)
}

/// Create a qualified identifier: `"schema"."name"`.
pub fn qualified_ident(schema: &str, name: &str) -> SqlFragment {
    SqlFragment::qualified_ident(schema, name)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_fragment_builds_literal() {
        let f = SqlFragment::raw("SELECT 1");
        let (sql, params) = f.build();
        assert_eq!(sql, "SELECT 1");
        assert!(params.is_empty());
    }

    #[test]
    fn param_fragment_assigns_dollar_n() {
        let f = SqlFragment::param(PgValue::Int(42));
        let (sql, params) = f.build();
        assert_eq!(sql, "$1");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].as_i64(), Some(42));
    }

    #[test]
    fn ident_fragment_double_quotes() {
        let f = SqlFragment::ident("user");
        let (sql, _) = f.build();
        assert_eq!(sql, "\"user\"");
    }

    #[test]
    fn ident_escapes_embedded_quotes() {
        let f = SqlFragment::ident("col\"name");
        let (sql, _) = f.build();
        assert_eq!(sql, "\"col\"\"name\"");
    }

    #[test]
    fn join_renumbers_params() {
        let a = SqlFragment::raw("a = ")
            .append(SqlFragment::param(PgValue::Int(1)));
        let b = SqlFragment::raw("b = ")
            .append(SqlFragment::param(PgValue::Text("hello".into())));
        let combined = a.join(" AND ", b);
        let (sql, params) = combined.build();
        assert_eq!(sql, "a = $1 AND b = $2");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].as_i64(), Some(1));
        assert_eq!(params[1].as_text(), Some("hello"));
    }

    #[test]
    fn qualified_ident_produces_schema_dot_table() {
        let f = SqlFragment::qualified_ident("public", "users");
        let (sql, _) = f.build();
        assert_eq!(sql, "\"public\".\"users\"");
    }

    #[test]
    fn nested_fragment_flattens_correctly() {
        let inner = SqlFragment::raw("id = ")
            .append(SqlFragment::param(PgValue::Int(5)));
        let outer = SqlFragment {
            parts: vec![
                SqlPart::Literal("SELECT * FROM t WHERE ".to_string()),
                SqlPart::Fragment(Box::new(inner)),
            ],
            params: Vec::new(),
        };
        let (sql, params) = outer.build();
        assert_eq!(sql, "SELECT * FROM t WHERE id = $1");
        assert_eq!(params.len(), 1);
    }
}
