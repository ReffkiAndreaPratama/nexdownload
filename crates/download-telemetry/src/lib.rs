//! Logging bootstrap and privacy helpers.

use std::sync::Once;

use tracing_subscriber::EnvFilter;

static INIT: Once = Once::new();

/// Initialise the global tracing subscriber. Safe to call multiple times.
pub fn init(default_level: &str) {
    INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(format!("nexdownload={default_level},download_engine={default_level},download_api={default_level},warn")));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(true)
            .with_thread_ids(false)
            .with_ansi(true)
            .try_init();
    });
}

/// Hide a bearer token while keeping a short fingerprint for correlation.
pub fn redact_token(token: &str) -> String {
    if token.len() <= 8 {
        return "***".into();
    }
    let prefix = &token[..4.min(token.len())];
    let suffix = &token[token.len().saturating_sub(4)..];
    format!("{prefix}…{suffix}")
}

/// Remove credentials and sensitive query parameters from a URL.
pub fn redact_url(input: &str) -> String {
    let Ok(mut url) = url::Url::parse(input) else {
        return "<invalid-url>".into();
    };
    if url.password().is_some() {
        let _ = url.set_password(Some("***"));
    }
    if !url.username().is_empty() {
        let _ = url.set_username("***");
    }
    let sensitive = [
        "token",
        "key",
        "signature",
        "sig",
        "password",
        "auth",
        "secret",
        "apikey",
    ];
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| {
            if sensitive.iter().any(|s| k.to_ascii_lowercase().contains(s)) {
                (k.to_string(), "***".to_string())
            } else {
                (k.to_string(), v.to_string())
            }
        })
        .collect();
    if !pairs.is_empty() {
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_credentials_and_tokens() {
        let redacted = redact_url("https://user:secret@example.com/file?token=abc&page=2");
        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("token=abc"));
        assert!(redacted.contains("page=2"));
    }

    #[test]
    fn redacts_short_tokens_completely() {
        assert_eq!(redact_token("abc"), "***");
        assert_eq!(redact_token("0123456789abcdef"), "0123…cdef");
    }
}
