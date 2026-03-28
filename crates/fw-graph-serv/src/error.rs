#[derive(Debug, thiserror::Error)]
pub enum ServError {
    #[error("JWT validation failed: {0}")]
    JwtError(String),
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    #[error("Schema registry error: {0}")]
    SchemaError(String),
    #[error("WebSocket error: {0}")]
    WebSocketError(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
}
