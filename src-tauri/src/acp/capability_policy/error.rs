use thiserror::Error;

#[derive(Debug, Error)]
pub enum CapabilityPolicyError {
    #[error("capability policy snapshot is invalid: {0}")]
    InvalidSnapshot(String),
    #[error("capability policy cache failed: {0}")]
    Cache(String),
    #[error("capability policy transport failed: {0}")]
    Transport(String),
    #[error("capability policy revision moved backwards")]
    RevisionRollback,
    #[error("capability policy revision reused different permissions")]
    RevisionCollision,
    #[error("capability policy returned not-modified without a trusted snapshot")]
    NotModifiedWithoutSnapshot,
    #[error("capability policy returned not-modified for an unconditional refresh")]
    UnconditionalNotModified,
}

impl CapabilityPolicyError {
    pub fn cache(error: impl ToString) -> Self {
        Self::Cache(error.to_string())
    }

    pub fn transport(error: impl ToString) -> Self {
        Self::Transport(error.to_string())
    }

    pub(crate) fn rejects_revision(&self) -> bool {
        matches!(self, Self::RevisionRollback | Self::RevisionCollision)
    }
}
