use std::path::{Component, Path, PathBuf};

use percent_encoding::percent_decode_str;
use url::Url;

use crate::error::CoreError;

const MAX_URL_LEN: usize = 8192;
const MAX_FILENAME_CHARS: usize = 180;

/// Parse and validate a user supplied download URL.
///
/// Only `http` and `https` are accepted. Embedded credentials are rejected so
/// they can never end up in the database or logs.
pub fn parse_and_validate_url(input: &str) -> Result<Url, CoreError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidUrl("empty URL".into()));
    }
    if trimmed.len() > MAX_URL_LEN {
        return Err(CoreError::InvalidUrl("URL is too long".into()));
    }
    let url = Url::parse(trimmed).map_err(|e| CoreError::InvalidUrl(e.to_string()))?;
    match url.scheme() {
        "http" | "https" => {}
        other => return Err(CoreError::UnsupportedScheme(other.to_string())),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(CoreError::EmbeddedCredentials);
    }
    if url.host_str().map(str::is_empty).unwrap_or(true) {
        return Err(CoreError::InvalidUrl("URL has no host".into()));
    }
    Ok(url)
}

/// Normalise a remote-provided filename into something safe to place inside a
/// configured download directory.
pub fn sanitize_filename(input: &str) -> String {
    let decoded = percent_decode_str(input).decode_utf8_lossy();
    // Only keep the final component, regardless of separator style.
    let leaf = decoded
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("download")
        .trim();

    let mut cleaned = String::with_capacity(leaf.len());
    for ch in leaf.chars() {
        if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
            cleaned.push('_');
        } else {
            cleaned.push(ch);
        }
    }
    let cleaned = cleaned
        .trim_matches(|c: char| c == '.' || c.is_whitespace())
        .to_string();

    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return "download".into();
    }

    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem_upper = cleaned
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let mut cleaned = if reserved.contains(&stem_upper.as_str()) {
        format!("_{cleaned}")
    } else {
        cleaned
    };

    if cleaned.chars().count() > MAX_FILENAME_CHARS {
        let extension = Path::new(&cleaned)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e.chars().take(16).collect::<String>()))
            .unwrap_or_default();
        let keep = MAX_FILENAME_CHARS.saturating_sub(extension.chars().count());
        let stem: String = cleaned.chars().take(keep).collect();
        cleaned = format!("{}{}", stem.trim_end_matches(['.', ' ']), extension);
    }

    if cleaned.is_empty() {
        "download".into()
    } else {
        cleaned
    }
}

/// Join a single filename onto an existing directory while guaranteeing that
/// the result cannot escape it.
pub fn safe_destination(base: &Path, file_name: &str) -> Result<PathBuf, CoreError> {
    if file_name.trim().is_empty() {
        return Err(CoreError::UnsafeFilename);
    }
    // Reject raw path syntax before sanitising. Silently rewriting a path that
    // tried to escape the destination would hide a caller bug, so callers must
    // pass a single file name here.
    if file_name.contains('/') || file_name.contains('\\') {
        return Err(CoreError::UnsafePath);
    }
    let name = sanitize_filename(file_name);
    let candidate = Path::new(&name);
    if candidate.is_absolute()
        || candidate
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(CoreError::UnsafePath);
    }
    Ok(base.join(name))
}

/// Pick a non-existing path by appending ` (n)` before the extension.
pub fn unique_path(base: &Path, file_name: &str) -> Result<PathBuf, CoreError> {
    let candidate = safe_destination(base, file_name)?;
    if !candidate.exists() {
        return Ok(candidate);
    }
    let stem = candidate
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download");
    let extension = candidate.extension().and_then(|s| s.to_str());
    for index in 1..10_000u32 {
        let name = match extension {
            Some(ext) => format!("{stem} ({index}).{ext}"),
            None => format!("{stem} ({index})"),
        };
        let path = base.join(sanitize_filename(&name));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(CoreError::UnsafePath)
}

/// Derive a filename from a URL path, falling back to the host name.
pub fn filename_from_url(url: &Url) -> String {
    if let Some(last) = url.path_segments().and_then(|mut s| s.next_back()) {
        if !last.trim().is_empty() {
            return sanitize_filename(last);
        }
    }
    let host = url.host_str().unwrap_or("download");
    sanitize_filename(&format!("{host}-download"))
}

/// Parse `Content-Disposition` without pulling in a full header parser.
pub fn filename_from_content_disposition(value: &str) -> Option<String> {
    // RFC 5987 form wins when present.
    for part in value.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("filename*=") {
            let rest = rest.trim_matches('"');
            let encoded = rest.split_once("''").map(|(_, v)| v).unwrap_or(rest);
            let decoded = percent_decode_str(encoded).decode_utf8_lossy();
            let name = sanitize_filename(&decoded);
            if name != "download" {
                return Some(name);
            }
        }
    }
    for part in value.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("filename=") {
            let name = sanitize_filename(rest.trim_matches('"'));
            if name != "download" {
                return Some(name);
            }
        }
    }
    None
}

/// Validate and normalise a SHA-256 hex digest.
pub fn validate_sha256(value: &str) -> Result<String, CoreError> {
    let trimmed = value.trim();
    let trimmed = trimmed
        .strip_prefix("sha256:")
        .or_else(|| trimmed.strip_prefix("SHA256:"))
        .unwrap_or(trimmed)
        .trim();
    if trimmed.len() != 64 || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CoreError::InvalidChecksum(value.to_string()));
    }
    Ok(trimmed.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_dangerous_schemes_and_credentials() {
        assert!(parse_and_validate_url("file:///etc/passwd").is_err());
        assert!(parse_and_validate_url("javascript:alert(1)").is_err());
        assert!(parse_and_validate_url("https://user:pass@example.com/file").is_err());
        assert!(parse_and_validate_url("https://example.com/file.bin").is_ok());
    }

    #[test]
    fn sanitizes_path_traversal_and_windows_reserved_names() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("..\\..\\boot.ini"), "boot.ini");
        assert_eq!(sanitize_filename("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_filename("   "), "download");
        assert_eq!(sanitize_filename("file<name>?.zip"), "file_name__.zip");
    }

    #[test]
    fn safe_destination_stays_inside_base() {
        let base = Path::new("C:\\downloads");
        assert!(safe_destination(base, "..\\evil.exe").is_err());
        assert!(safe_destination(base, "C:\\Windows\\evil.exe").is_err());
        assert_eq!(
            safe_destination(base, "normal.zip").unwrap(),
            base.join("normal.zip")
        );
    }

    #[test]
    fn parses_content_disposition() {
        assert_eq!(
            filename_from_content_disposition("attachment; filename=\"report final.pdf\""),
            Some("report final.pdf".into())
        );
        assert_eq!(
            filename_from_content_disposition("attachment; filename*=UTF-8''na%C3%AFve.txt"),
            Some("naïve.txt".into())
        );
    }

    #[test]
    fn validates_checksums() {
        let digest = "a".repeat(64);
        assert_eq!(validate_sha256(&digest).unwrap(), digest);
        assert!(validate_sha256("sha256:xyz").is_err());
    }
}
