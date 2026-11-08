//! Native download engine.
//!
//! The engine is deliberately independent from persistence and from the UI. It
//! receives an explicit request (including any recovered segment state) and
//! reports progress through a bounded channel. This makes it usable by the
//! desktop app, the headless server and integration tests without change.

pub mod adaptive;
pub mod engine;
pub mod error;

use std::path::PathBuf;

use async_trait::async_trait;
use download_core::{DownloadSegment, EngineOptions, ProgressSnapshot, ResourceInfo};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use url::Url;

pub use adaptive::{Adaptation, AdaptiveController, AdaptiveGate, BandwidthLimiter};
pub use engine::NativeHttpEngine;
pub use error::EngineError;

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub task_id: String,
    pub url: Url,
    pub destination: PathBuf,
    pub expected_size: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub accepts_ranges: bool,
    pub segments: Vec<DownloadSegment>,
    pub options: EngineOptions,
}

/// Control surface handed to the engine for one transfer.
pub struct EngineContext {
    pub pause: CancellationToken,
    pub cancel: CancellationToken,
    pub progress: mpsc::Sender<ProgressSnapshot>,
}

impl EngineContext {
    pub fn new(progress: mpsc::Sender<ProgressSnapshot>) -> Self {
        Self {
            pause: CancellationToken::new(),
            cancel: CancellationToken::new(),
            progress,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DownloadOutcome {
    Completed {
        downloaded: u64,
        total: Option<u64>,
        segments: Vec<DownloadSegment>,
    },
    Paused {
        downloaded: u64,
        total: Option<u64>,
        segments: Vec<DownloadSegment>,
    },
    Cancelled {
        downloaded: u64,
        total: Option<u64>,
        segments: Vec<DownloadSegment>,
    },
}

impl DownloadOutcome {
    pub fn segments(&self) -> &[DownloadSegment] {
        match self {
            Self::Completed { segments, .. }
            | Self::Paused { segments, .. }
            | Self::Cancelled { segments, .. } => segments,
        }
    }

    pub fn downloaded(&self) -> u64 {
        match self {
            Self::Completed { downloaded, .. }
            | Self::Paused { downloaded, .. }
            | Self::Cancelled { downloaded, .. } => *downloaded,
        }
    }

    pub fn total(&self) -> Option<u64> {
        match self {
            Self::Completed { total, .. }
            | Self::Paused { total, .. }
            | Self::Cancelled { total, .. } => *total,
        }
    }
}

/// Engine abstraction. Additional engines (for example an isolated aria2
/// adapter) can implement this trait without touching scheduling or storage.
#[async_trait]
pub trait DownloadEngine: Send + Sync {
    fn name(&self) -> &'static str;

    /// Inspect a resource without downloading its body.
    async fn inspect(
        &self,
        url: &Url,
        options: &EngineOptions,
    ) -> Result<ResourceInfo, EngineError>;

    /// Start, resume or continue a transfer.
    async fn run(
        &self,
        request: DownloadRequest,
        context: EngineContext,
    ) -> Result<DownloadOutcome, EngineError>;

    /// Release engine resources. The native engine is stateless, so this is a
    /// no-op, but adapters that own child processes need it.
    async fn shutdown(&self) -> Result<(), EngineError> {
        Ok(())
    }
}
