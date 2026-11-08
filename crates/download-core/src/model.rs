use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// Lifecycle state of a download task.
///
/// Persisted as a lower-case string. See [`crate::state`] for the allowed
/// transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Queued,
    Preparing,
    Connecting,
    Downloading,
    Pausing,
    Paused,
    Verifying,
    Completed,
    Failed,
    Cancelled,
    WaitingForNetwork,
}

impl DownloadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Preparing => "preparing",
            Self::Connecting => "connecting",
            Self::Downloading => "downloading",
            Self::Pausing => "pausing",
            Self::Paused => "paused",
            Self::Verifying => "verifying",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::WaitingForNetwork => "waiting_for_network",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "queued" => Self::Queued,
            "preparing" => Self::Preparing,
            "connecting" => Self::Connecting,
            "downloading" => Self::Downloading,
            "pausing" => Self::Pausing,
            "paused" => Self::Paused,
            "verifying" => Self::Verifying,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "waiting_for_network" => Self::WaitingForNetwork,
            _ => return None,
        })
    }

    /// A task that currently occupies (or is about to occupy) the engine.
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Preparing
                | Self::Connecting
                | Self::Downloading
                | Self::Pausing
                | Self::Verifying
        )
    }

    /// A task that can be started or resumed.
    pub fn is_startable(self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Paused | Self::Failed | Self::Cancelled | Self::WaitingForNetwork
        )
    }

    pub fn is_finished(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

impl std::fmt::Display for DownloadStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One contiguous byte range of a remote resource.
///
/// `end` is inclusive, matching HTTP range semantics. `downloaded` counts the
/// bytes already durably written from `start`; it is always a prefix of the
/// range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadSegment {
    pub index: u32,
    pub start: u64,
    pub end: u64,
    pub downloaded: u64,
}

impl DownloadSegment {
    pub fn new(index: u32, start: u64, end: u64) -> Result<Self, CoreError> {
        if end < start {
            return Err(CoreError::InvalidPlan(format!(
                "segment {index} ends before it starts"
            )));
        }
        Ok(Self {
            index,
            start,
            end,
            downloaded: 0,
        })
    }

    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start).saturating_add(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn remaining(&self) -> u64 {
        self.len().saturating_sub(self.downloaded)
    }

    pub fn is_complete(&self) -> bool {
        self.downloaded >= self.len()
    }

    pub fn next_offset(&self) -> Option<u64> {
        if self.is_complete() {
            None
        } else {
            Some(self.start + self.downloaded)
        }
    }
}

/// A complete, non-overlapping plan for a resource.
#[derive(Debug, Clone, Default)]
pub struct SegmentPlan;

impl SegmentPlan {
    /// Minimum size of a range when splitting. Small resources are downloaded
    /// with a single connection because parallel requests would be slower.
    pub const MIN_SEGMENT_BYTES: u64 = 512 * 1024;

    /// Build a plan for `total` bytes using up to `connections` ranges.
    pub fn build(total: u64, connections: u32) -> Result<Vec<DownloadSegment>, CoreError> {
        if total == 0 {
            return Err(CoreError::InvalidPlan(
                "cannot plan a zero-length resource".into(),
            ));
        }
        let connections = connections.clamp(1, 32) as u64;
        let by_size = (total / Self::MIN_SEGMENT_BYTES).max(1);
        let count = connections.min(by_size).min(total) as u32;

        let base = total / count as u64;
        let remainder = total % count as u64;

        let mut segments = Vec::with_capacity(count as usize);
        let mut start = 0u64;
        for index in 0..count {
            let len = base + if (index as u64) < remainder { 1 } else { 0 };
            let end = start
                .checked_add(len)
                .and_then(|v| v.checked_sub(1))
                .ok_or(CoreError::Overflow)?;
            segments.push(DownloadSegment::new(index, start, end)?);
            start = end + 1;
        }
        Ok(segments)
    }

    /// Rebuild a single-connection plan. Used when the server does not support
    /// byte ranges.
    pub fn single(total: Option<u64>) -> Vec<DownloadSegment> {
        match total {
            Some(total) if total > 0 => vec![DownloadSegment {
                index: 0,
                start: 0,
                end: total - 1,
                downloaded: 0,
            }],
            _ => Vec::new(),
        }
    }
}

/// Resource metadata discovered before or during a transfer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceInfo {
    pub final_url: String,
    pub file_name: Option<String>,
    pub content_type: Option<String>,
    pub total_bytes: Option<u64>,
    pub accepts_ranges: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub server: Option<String>,
}

/// A point-in-time progress sample emitted by an engine.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProgressSnapshot {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bps: f64,
    pub active_connections: u32,
    pub segments: Vec<DownloadSegment>,
    pub adaptive_note: Option<String>,
}

/// Engine configuration that is safe to persist or send over the local API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineOptions {
    pub max_connections: u32,
    pub user_agent: String,
    pub timeout_secs: u64,
    pub max_retries: u32,
    pub bandwidth_limit_bps: Option<u64>,
    pub proxy: Option<String>,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            max_connections: 4,
            user_agent: concat!("NexDownload/", env!("CARGO_PKG_VERSION")).to_string(),
            timeout_secs: 30,
            max_retries: 4,
            bandwidth_limit_bps: None,
            proxy: None,
        }
    }
}

/// Persisted representation of a download task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTask {
    pub id: String,
    pub url: String,
    pub final_url: Option<String>,
    pub file_name: String,
    pub destination_dir: String,
    pub file_path: String,
    pub category: String,
    pub queue_id: Option<String>,
    pub collection_id: Option<String>,
    pub status: DownloadStatus,
    pub priority: i32,
    pub total_bytes: Option<i64>,
    pub downloaded_bytes: i64,
    pub connections: u32,
    pub speed_bps: f64,
    pub error: Option<String>,
    pub error_kind: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub accepts_ranges: bool,
    pub content_type: Option<String>,
    pub checksum_sha256: Option<String>,
    pub expected_checksum: Option<String>,
    pub source_domain: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub segments: Vec<DownloadSegment>,
}

impl DownloadTask {
    pub fn progress_pct(&self) -> f64 {
        match self.total_bytes {
            Some(total) if total > 0 => {
                (self.downloaded_bytes as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
            }
            _ => 0.0,
        }
    }

    pub fn recompute_downloaded(&mut self) {
        self.downloaded_bytes = self
            .segments
            .iter()
            .map(|segment| segment.downloaded.min(segment.len()) as i64)
            .sum();
    }

    pub fn segments_complete(&self) -> bool {
        !self.segments.is_empty() && self.segments.iter().all(DownloadSegment::is_complete)
    }

    pub fn eta_seconds(&self) -> Option<u64> {
        let total = self.total_bytes? as u64;
        if self.speed_bps <= 1.0 || total <= self.downloaded_bytes as u64 {
            return None;
        }
        Some(((total - self.downloaded_bytes as u64) as f64 / self.speed_bps).ceil() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_plan_covers_whole_resource_without_overlap() {
        for total in [1u64, 1024, 1024 * 1024, 10 * 1024 * 1024 + 7] {
            for connections in [1u32, 2, 4, 8, 64] {
                let plan = SegmentPlan::build(total, connections).unwrap();
                assert_eq!(plan[0].start, 0);
                assert_eq!(plan.last().unwrap().end, total - 1);
                for pair in plan.windows(2) {
                    assert_eq!(pair[0].end + 1, pair[1].start);
                }
                let sum: u64 = plan.iter().map(DownloadSegment::len).sum();
                assert_eq!(sum, total);
            }
        }
    }

    #[test]
    fn small_resource_uses_one_segment() {
        let plan = SegmentPlan::build(4096, 8).unwrap();
        assert_eq!(plan.len(), 1);
    }

    #[test]
    fn segment_plan_rejects_zero() {
        assert!(SegmentPlan::build(0, 4).is_err());
    }
}
