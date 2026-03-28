use fw_graph_types::PgTypeOid;

pub mod oids {
    pub const BOOL: u32 = 16;
    pub const INT2: u32 = 21;
    pub const INT4: u32 = 23;
    pub const INT8: u32 = 20;
    pub const FLOAT4: u32 = 700;
    pub const FLOAT8: u32 = 701;
    pub const TEXT: u32 = 25;
    pub const VARCHAR: u32 = 1043;
    pub const CHAR: u32 = 18;
    pub const BPCHAR: u32 = 1042;
    pub const UUID: u32 = 2950;
    pub const TIMESTAMP: u32 = 1114;
    pub const TIMESTAMPTZ: u32 = 1184;
    pub const DATE: u32 = 1082;
    pub const JSON: u32 = 114;
    pub const JSONB: u32 = 3802;
    pub const NUMERIC: u32 = 1700;
    pub const BOOL_ARRAY: u32 = 1000;
    pub const INT4_ARRAY: u32 = 1007;
    pub const TEXT_ARRAY: u32 = 1009;
    pub const UUID_ARRAY: u32 = 2951;
    pub const FLOAT8_ARRAY: u32 = 1022;
    pub const JSONB_ARRAY: u32 = 3807;
}

/// Map a Postgres OID to a GraphQL scalar type name.
/// Returns `None` for unknown OIDs.
pub fn pg_oid_to_gql_type(oid: PgTypeOid) -> Option<&'static str> {
    use oids::*;
    match oid {
        BOOL => Some("Boolean"),
        INT2 | INT4 => Some("Int"),
        INT8 | NUMERIC => Some("BigInt"),
        FLOAT4 | FLOAT8 => Some("Float"),
        TEXT | VARCHAR | CHAR | BPCHAR => Some("String"),
        UUID => Some("UUID"),
        TIMESTAMP | TIMESTAMPTZ => Some("DateTime"),
        DATE => Some("Date"),
        JSON | JSONB => Some("JSON"),
        // Array types
        BOOL_ARRAY => Some("[Boolean]"),
        INT4_ARRAY => Some("[Int]"),
        TEXT_ARRAY => Some("[String]"),
        UUID_ARRAY => Some("[UUID]"),
        FLOAT8_ARRAY => Some("[Float]"),
        JSONB_ARRAY => Some("[JSON]"),
        _ => None,
    }
}

/// Return a GraphQL type reference string for the given OID.
/// Appends "!" for non-null. Falls back to "String" for unknown OIDs.
pub fn gql_type_ref(oid: PgTypeOid, is_not_null: bool) -> String {
    let base = pg_oid_to_gql_type(oid).unwrap_or("String");
    if is_not_null {
        format!("{}!", base)
    } else {
        base.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oids::*;

    #[test]
    fn test_bool() {
        assert_eq!(pg_oid_to_gql_type(BOOL), Some("Boolean"));
    }

    #[test]
    fn test_integers() {
        assert_eq!(pg_oid_to_gql_type(INT2), Some("Int"));
        assert_eq!(pg_oid_to_gql_type(INT4), Some("Int"));
        assert_eq!(pg_oid_to_gql_type(INT8), Some("BigInt"));
        assert_eq!(pg_oid_to_gql_type(NUMERIC), Some("BigInt"));
    }

    #[test]
    fn test_floats() {
        assert_eq!(pg_oid_to_gql_type(FLOAT4), Some("Float"));
        assert_eq!(pg_oid_to_gql_type(FLOAT8), Some("Float"));
    }

    #[test]
    fn test_strings() {
        assert_eq!(pg_oid_to_gql_type(TEXT), Some("String"));
        assert_eq!(pg_oid_to_gql_type(VARCHAR), Some("String"));
        assert_eq!(pg_oid_to_gql_type(CHAR), Some("String"));
        assert_eq!(pg_oid_to_gql_type(BPCHAR), Some("String"));
    }

    #[test]
    fn test_uuid() {
        assert_eq!(pg_oid_to_gql_type(UUID), Some("UUID"));
    }

    #[test]
    fn test_datetime() {
        assert_eq!(pg_oid_to_gql_type(TIMESTAMP), Some("DateTime"));
        assert_eq!(pg_oid_to_gql_type(TIMESTAMPTZ), Some("DateTime"));
        assert_eq!(pg_oid_to_gql_type(DATE), Some("Date"));
    }

    #[test]
    fn test_json() {
        assert_eq!(pg_oid_to_gql_type(JSON), Some("JSON"));
        assert_eq!(pg_oid_to_gql_type(JSONB), Some("JSON"));
    }

    #[test]
    fn test_arrays() {
        assert_eq!(pg_oid_to_gql_type(BOOL_ARRAY), Some("[Boolean]"));
        assert_eq!(pg_oid_to_gql_type(INT4_ARRAY), Some("[Int]"));
        assert_eq!(pg_oid_to_gql_type(TEXT_ARRAY), Some("[String]"));
        assert_eq!(pg_oid_to_gql_type(UUID_ARRAY), Some("[UUID]"));
        assert_eq!(pg_oid_to_gql_type(FLOAT8_ARRAY), Some("[Float]"));
        assert_eq!(pg_oid_to_gql_type(JSONB_ARRAY), Some("[JSON]"));
    }

    #[test]
    fn test_unknown_oid() {
        assert_eq!(pg_oid_to_gql_type(99999), None);
    }

    #[test]
    fn test_gql_type_ref_not_null() {
        assert_eq!(gql_type_ref(INT4, true), "Int!");
        assert_eq!(gql_type_ref(TEXT, true), "String!");
        assert_eq!(gql_type_ref(UUID, true), "UUID!");
    }

    #[test]
    fn test_gql_type_ref_nullable() {
        assert_eq!(gql_type_ref(INT4, false), "Int");
        assert_eq!(gql_type_ref(BOOL, false), "Boolean");
    }

    #[test]
    fn test_gql_type_ref_unknown_fallback() {
        assert_eq!(gql_type_ref(99999, false), "String");
        assert_eq!(gql_type_ref(99999, true), "String!");
    }
}
