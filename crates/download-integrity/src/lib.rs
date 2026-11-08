//! Streaming file hashing and integrity verification.
//!
//! A matching checksum only proves byte identity with the expected digest. It
//! is **not** a malware scan and NexDownload never presents it as one.

use std::io::Read;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IntegrityError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("file size mismatch: expected {expected} bytes, found {actual}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("checksum mismatch: expected {expected}, found {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("invalid expected checksum")]
    InvalidExpected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrityStatus {
    Verified,
    ChecksumMismatch,
    SizeMismatch,
    NotConfigured,
}

impl IntegrityStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::ChecksumMismatch => "checksum_mismatch",
            Self::SizeMismatch => "size_mismatch",
            Self::NotConfigured => "not_configured",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReport {
    pub status: IntegrityStatus,
    pub algorithm: &'static str,
    pub actual_sha256: String,
    pub expected_sha256: Option<String>,
    pub actual_size: u64,
    pub expected_size: Option<u64>,
    pub checked_at: DateTime<Utc>,
}

impl IntegrityReport {
    pub fn is_ok(&self) -> bool {
        matches!(
            self.status,
            IntegrityStatus::Verified | IntegrityStatus::NotConfigured
        )
    }
}

/// Compute the SHA-256 digest of a file without loading it into memory.
pub async fn sha256_file(path: impl AsRef<Path>) -> Result<String, IntegrityError> {
    let path = path.as_ref().to_path_buf();
    let display = path.clone();
    let size = tokio::fs::metadata(&path)
        .await
        .map_err(|source| IntegrityError::Io {
            path: display.clone(),
            source,
        })?
        .len();

    tokio::task::spawn_blocking(move || hash_file_blocking(&path, size))
        .await
        .map_err(|e| IntegrityError::Io {
            path: PathBuf::from("<blocking task>"),
            source: std::io::Error::other(e.to_string()),
        })?
}

fn hash_file_blocking(path: &Path, _size: u64) -> Result<String, IntegrityError> {
    let mut file = std::fs::File::open(path).map_err(|source| IntegrityError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| IntegrityError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Verify size and an optional SHA-256 digest.
pub async fn verify_file(
    path: impl AsRef<Path>,
    expected_size: Option<u64>,
    expected_sha256: Option<&str>,
) -> Result<IntegrityReport, IntegrityError> {
    let path = path.as_ref();
    let actual_size = tokio::fs::metadata(path)
        .await
        .map_err(|source| IntegrityError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .len();

    if let Some(expected) = expected_size {
        if expected != actual_size {
            let actual = sha256_file(path).await?;
            return Ok(IntegrityReport {
                status: IntegrityStatus::SizeMismatch,
                algorithm: "sha256",
                actual_sha256: actual,
                expected_sha256: expected_sha256.map(str::to_ascii_lowercase),
                actual_size,
                expected_size: Some(expected),
                checked_at: Utc::now(),
            });
        }
    }

    let actual = sha256_file(path).await?;

    let status = match expected_sha256 {
        None => IntegrityStatus::NotConfigured,
        Some(expected) => {
            let expected = expected
                .trim()
                .strip_prefix("sha256:")
                .unwrap_or(expected.trim())
                .to_ascii_lowercase();
            if expected.len() != 64 || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(IntegrityError::InvalidExpected);
            }
            if expected == actual {
                IntegrityStatus::Verified
            } else {
                IntegrityStatus::ChecksumMismatch
            }
        }
    };

    Ok(IntegrityReport {
        status,
        algorithm: "sha256",
        actual_sha256: actual,
        expected_sha256: expected_sha256.map(str::to_ascii_lowercase),
        actual_size,
        expected_size,
        checked_at: Utc::now(),
    })
}

/// Compare two files by size first, then by digest.
pub async fn files_identical(
    a: impl AsRef<Path>,
    b: impl AsRef<Path>,
) -> Result<bool, IntegrityError> {
    let a = a.as_ref();
    let b = b.as_ref();
    let (ma, mb) = (tokio::fs::metadata(a).await, tokio::fs::metadata(b).await);
    let (ma, mb) = match (ma, mb) {
        (Ok(ma), Ok(mb)) => (ma, mb),
        (Err(source), _) => {
            return Err(IntegrityError::Io {
                path: a.to_path_buf(),
                source,
            })
        }
        (_, Err(source)) => {
            return Err(IntegrityError::Io {
                path: b.to_path_buf(),
                source,
            })
        }
    };
    if ma.len() != mb.len() {
        return Ok(false);
    }
    let (ha, hb) = tokio::try_join!(sha256_file(a), sha256_file(b))?;
    Ok(ha == hb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn verifies_matching_checksum() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.bin");
        tokio::fs::write(&path, b"nexdownload").await.unwrap();
        let digest = sha256_file(&path).await.unwrap();
        let report = verify_file(&path, Some(11), Some(&digest)).await.unwrap();
        assert!(report.is_ok());
        assert_eq!(report.status, IntegrityStatus::Verified);
    }

    #[tokio::test]
    async fn detects_checksum_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.bin");
        tokio::fs::write(&path, b"nexdownload").await.unwrap();
        let report = verify_file(&path, None, Some(&"0".repeat(64)))
            .await
            .unwrap();
        assert_eq!(report.status, IntegrityStatus::ChecksumMismatch);
    }

    #[tokio::test]
    async fn detects_size_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.bin");
        tokio::fs::write(&path, b"nexdownload").await.unwrap();
        let report = verify_file(&path, Some(99), None).await.unwrap();
        assert_eq!(report.status, IntegrityStatus::SizeMismatch);
    }
}
