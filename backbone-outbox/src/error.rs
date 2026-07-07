//! Error type for the durable outbox/inbox/relay.

/// Errors from staging, relaying, or deduping durable events.
#[derive(Debug, thiserror::Error)]
pub enum OutboxError {
    /// A database error.
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    /// A schema name that is not a safe SQL identifier (`^[a-z_][a-z0-9_]*$`).
    #[error("invalid schema identifier: {0:?}")]
    InvalidSchema(String),
    /// The relay's `publish` callback failed for a record; the row is left unpublished for retry.
    #[error("publish failed: {0}")]
    Publish(String),
}

/// Result alias for outbox operations.
pub type Result<T> = std::result::Result<T, OutboxError>;

/// Reject a schema name that is not a safe, lowercase SQL identifier. Schema names come from trusted
/// module config, but they are interpolated into DDL/DML (Postgres has no bind parameter for an
/// identifier), so we validate defensively.
pub(crate) fn validate_schema(schema: &str) -> Result<()> {
    let ok = !schema.is_empty()
        && schema.len() <= 63
        && schema.bytes().next().map(|b| b == b'_' || b.is_ascii_lowercase()).unwrap_or(false)
        && schema.bytes().all(|b| b == b'_' || b.is_ascii_lowercase() || b.is_ascii_digit());
    if ok {
        Ok(())
    } else {
        Err(OutboxError::InvalidSchema(schema.to_string()))
    }
}
