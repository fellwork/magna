use magna_types::FwGraphError;

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("Introspection error: {0}")]
    Introspection(#[from] FwGraphError),

    #[error("Unmapped PG type OID {oid} for column {table}.{column}")]
    UnmappedType {
        oid: u32,
        column: String,
        table: String,
    },

    #[error("Duplicate GraphQL type name: {0}")]
    DuplicateTypeName(String),

    #[error("Schema build failed: {0}")]
    SchemaFinish(String),

    #[error("Plugin '{plugin}' error: {message}")]
    PluginError {
        plugin: String,
        message: String,
    },
}
