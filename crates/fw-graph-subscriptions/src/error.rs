#[derive(Debug, thiserror::Error)]
pub enum SubscriptionError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Payload too large: {size} bytes (max 7900)")]
    PayloadTooLarge { size: usize },
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Channel error: {0}")]
    ChannelError(String),
}
