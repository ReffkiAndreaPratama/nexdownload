use serde::Serialize;
use thiserror::Error;

/// Typed engine failures. Variants map to user-facing diagnostics and retry
/// decisions; raw upstream details are kept in the message for developers.
#[derive(Debug, Clone, Error, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum EngineError {
    #[error("network error: {0}")]
    Network(String),

    #[error("DNS resolution failed: {0}")]
    Dns(String),

    #[error("TLS negotiation failed: {0}")]
    Tls(String),

    #[error("request timed out")]
    Timeout,

    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },

    #[error("I/O error: {0}")]
    Io(String),

    #[error("permission denied: {0}")]
    Permission(String),

    #[error("insufficient storage: {0}")]
    NoSpace(String),

    #[error("the remote resource changed during the transfer")]
    ResourceChanged,

    #[error("the server does not support byte ranges for this resource")]
    RangeUnsupported,

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("download cancelled")]
    Cancelled,

    #[error("internal engine error: {0}")]
    Internal(String),

    #[error("integrity failure: {0}")]
    Integrity(String),
}

impl EngineError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Network(_) => "network",
            Self::Dns(_) => "dns",
            Self::Tls(_) => "tls",
            Self::Timeout => "timeout",
            Self::Http { .. } => "http",
            Self::Io(_) => "io",
            Self::Permission(_) => "permission",
            Self::NoSpace(_) => "insufficient_storage",
            Self::ResourceChanged => "resource_changed",
            Self::RangeUnsupported => "range_unsupported",
            Self::InvalidRequest(_) => "invalid_request",
            Self::Cancelled => "cancelled",
            Self::Internal(_) => "internal",
            Self::Integrity(_) => "integrity",
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::Network(_) => "The connection to the server was interrupted.".into(),
            Self::Dns(_) => "The server name could not be resolved. Check your network or DNS settings.".into(),
            Self::Tls(_) => "A secure connection could not be established. The certificate may be invalid or the system clock may be wrong.".into(),
            Self::Timeout => "The server did not respond in time.".into(),
            Self::Http { status, .. } => format!("The server rejected the request with HTTP status {status}."),
            Self::Io(_) => "The downloaded data could not be written to disk.".into(),
            Self::Permission(_) => "NexDownload does not have permission to write to the destination folder.".into(),
            Self::NoSpace(_) => "There is not enough free disk space for this download.".into(),
            Self::ResourceChanged => "The file changed on the server since the download started. Restart it to get a consistent copy.".into(),
            Self::RangeUnsupported => "The server does not support resumable downloads; this transfer will use one connection.".into(),
            Self::InvalidRequest(_) => "The download request is not valid.".into(),
            Self::Cancelled => "The download was cancelled.".into(),
            Self::Internal(_) => "An internal engine error occurred. See the diagnostic log.".into(),
            Self::Integrity(_) => "The downloaded file failed integrity verification.".into(),
        }
    }

    pub fn hints(&self) -> Vec<String> {
        let hints: &[&str] = match self {
            Self::Network(_) => &[
                "Check that you are still connected to the internet.",
                "Try lowering the connection count in Settings.",
            ],
            Self::Dns(_) => &[
                "Try opening the URL in a browser.",
                "Check your DNS or VPN configuration.",
            ],
            Self::Tls(_) => &[
                "Verify the system date and time.",
                "The server certificate may be expired or self-signed.",
            ],
            Self::Timeout => &[
                "Retry the download.",
                "Increase the timeout in Settings.",
                "The server may be rate limiting this client.",
            ],
            Self::Http { .. } => &[
                "The link may have expired.",
                "Try the download in a browser to confirm it still works.",
            ],
            Self::Permission(_) => &[
                "Choose a destination inside your user profile.",
                "Close applications that may lock the file.",
            ],
            Self::NoSpace(_) => &["Free disk space, then resume the download."],
            Self::ResourceChanged => &["Use Retry to start the download again from a clean state."],
            Self::RangeUnsupported => {
                &["No action required; a single connection is used automatically."]
            }
            Self::Integrity(_) => {
                &["Delete the file and download it again from the original source."]
            }
            _ => &[],
        };
        hints.iter().map(|s| (*s).to_string()).collect()
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Network(_) | Self::Dns(_) | Self::Timeout | Self::Io(_)
        ) || matches!(self, Self::Http { status, .. } if *status == 408 || *status == 429 || (500..600).contains(status))
    }

    pub fn from_reqwest(error: reqwest::Error) -> Self {
        let text = error.to_string();
        let lower = text.to_ascii_lowercase();
        if error.is_timeout() {
            Self::Timeout
        } else if lower.contains("dns") || lower.contains("lookup") || lower.contains("resolving") {
            Self::Dns(text)
        } else if lower.contains("tls")
            || lower.contains("certificate")
            || lower.contains("handshake")
            || lower.contains("ssl")
        {
            Self::Tls(text)
        } else {
            Self::Network(text)
        }
    }

    pub fn from_io(error: std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::PermissionDenied => Self::Permission(error.to_string()),
            _ => {
                // ERROR_DISK_FULL / ENOSPC are not mapped to a stable portable
                // ErrorKind on every supported Rust release.
                let text = error.to_string();
                if error.raw_os_error() == Some(112)
                    || text.to_ascii_lowercase().contains("no space")
                    || text.to_ascii_lowercase().contains("disk full")
                {
                    Self::NoSpace(text)
                } else {
                    Self::Io(text)
                }
            }
        }
    }
}

impl From<std::io::Error> for EngineError {
    fn from(value: std::io::Error) -> Self {
        Self::from_io(value)
    }
}

impl From<reqwest::Error> for EngineError {
    fn from(value: reqwest::Error) -> Self {
        Self::from_reqwest(value)
    }
}

impl From<download_core::CoreError> for EngineError {
    fn from(value: download_core::CoreError) -> Self {
        match value {
            download_core::CoreError::InvalidPlan(message) => Self::InvalidRequest(message),
            other => Self::InvalidRequest(other.to_string()),
        }
    }
}
