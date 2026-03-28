//! Build GraphQL fields for PostgreSQL stored functions/procedures.
//!
//! Classification:
//! - Volatile → mutation field
//! - Stable/Immutable + returns_set → query connection field
//! - Stable/Immutable + !returns_set → query scalar/object field

use async_graphql::dynamic::{Field, FieldFuture, TypeRef};
use async_graphql::Value;
use fw_graph_introspect::{PgProc, ProcVolatility};

/// How a Postgres function should appear in the GraphQL schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    /// Goes on the Query type as a single-value field.
    QueryField,
    /// Goes on the Query type as a connection (paginated list).
    QueryConnection,
    /// Goes on the Mutation type.
    MutationField,
}

/// Classify a `PgProc` into a `FunctionKind`.
pub fn classify_function(proc: &PgProc) -> FunctionKind {
    match proc.volatility {
        ProcVolatility::Volatile => FunctionKind::MutationField,
        ProcVolatility::Stable | ProcVolatility::Immutable => {
            if proc.returns_set {
                FunctionKind::QueryConnection
            } else {
                FunctionKind::QueryField
            }
        }
    }
}

/// Build query and mutation fields for a slice of `PgProc`s.
///
/// Returns `(query_fields, mutation_fields)`.
///
/// Fields are stubs — resolvers return null; full implementations come in a
/// later build phase when the data plan is wired up.
pub fn build_function_fields(procs: &[PgProc]) -> (Vec<Field>, Vec<Field>) {
    let mut query_fields: Vec<Field> = Vec::new();
    let mut mutation_fields: Vec<Field> = Vec::new();

    for proc in procs {
        let kind = classify_function(proc);
        let field_name = proc.name.clone();

        // All function fields return String for now (type resolution from
        // return_type OID happens in a later phase when the type map is built).
        let field = Field::new(
            field_name,
            TypeRef::named(TypeRef::STRING),
            |_| FieldFuture::from_value(Some(Value::Null)),
        );

        match kind {
            FunctionKind::QueryField | FunctionKind::QueryConnection => {
                query_fields.push(field);
            }
            FunctionKind::MutationField => {
                mutation_fields.push(field);
            }
        }
    }

    (query_fields, mutation_fields)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use fw_graph_introspect::PgProc;

    fn make_proc(volatility: ProcVolatility, returns_set: bool, name: &str) -> PgProc {
        PgProc {
            oid: 1,
            name: name.to_string(),
            schema_oid: 1,
            arg_types: vec![],
            return_type: 25, // text
            returns_set,
            is_strict: false,
            volatility,
            language: "sql".to_string(),
        }
    }

    /// Volatile functions are mutations.
    #[test]
    fn test_volatile_is_mutation() {
        let proc = make_proc(ProcVolatility::Volatile, false, "do_thing");
        assert_eq!(classify_function(&proc), FunctionKind::MutationField);
    }

    /// Volatile + returns_set is still a mutation.
    #[test]
    fn test_volatile_returns_set_is_mutation() {
        let proc = make_proc(ProcVolatility::Volatile, true, "do_many");
        assert_eq!(classify_function(&proc), FunctionKind::MutationField);
    }

    /// Stable + returns_set → QueryConnection.
    #[test]
    fn test_stable_returns_set_is_query_connection() {
        let proc = make_proc(ProcVolatility::Stable, true, "get_items");
        assert_eq!(classify_function(&proc), FunctionKind::QueryConnection);
    }

    /// Stable + !returns_set → QueryField.
    #[test]
    fn test_stable_no_set_is_query_field() {
        let proc = make_proc(ProcVolatility::Stable, false, "get_one");
        assert_eq!(classify_function(&proc), FunctionKind::QueryField);
    }

    /// Immutable + returns_set → QueryConnection.
    #[test]
    fn test_immutable_returns_set_is_query_connection() {
        let proc = make_proc(ProcVolatility::Immutable, true, "list_constants");
        assert_eq!(classify_function(&proc), FunctionKind::QueryConnection);
    }

    /// Immutable + !returns_set → QueryField.
    #[test]
    fn test_immutable_no_set_is_query_field() {
        let proc = make_proc(ProcVolatility::Immutable, false, "compute");
        assert_eq!(classify_function(&proc), FunctionKind::QueryField);
    }

    /// build_function_fields distributes to query vs mutation.
    #[test]
    fn test_build_function_fields_distributes() {
        let procs = vec![
            make_proc(ProcVolatility::Stable, false, "get_one"),
            make_proc(ProcVolatility::Stable, true, "get_many"),
            make_proc(ProcVolatility::Volatile, false, "mutate_one"),
        ];

        let (query, mutation) = build_function_fields(&procs);
        assert_eq!(query.len(), 2, "should have 2 query fields");
        assert_eq!(mutation.len(), 1, "should have 1 mutation field");
    }

    /// Empty procs slice returns empty vecs.
    #[test]
    fn test_build_function_fields_empty() {
        let (query, mutation) = build_function_fields(&[]);
        assert!(query.is_empty());
        assert!(mutation.is_empty());
    }
}
