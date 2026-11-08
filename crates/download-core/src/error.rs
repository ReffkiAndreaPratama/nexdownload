use thiserror::Error;

/// Errors originating in the domain layer.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("unsupported URL scheme '{0}'; only http and https are allowed")]
    UnsupportedScheme(String),

    #[error("URL contains embedded credentials, which NexDownload refuses to store")]
    EmbeddedCredentials,

    #[error("unsafe or empty filename")]
    UnsafeFilename,

    #[error("destination escapes the configured download directory")]
    UnsafePath,

    #[error("invalid state transition from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("invalid SHA-256 checksum: {0}")]
    InvalidChecksum(String),

    #[error("invalid download segment plan: {0}")]
    InvalidPlan(String),

    #[error("numeric overflow while planning the download")]
    Overflow,
}
