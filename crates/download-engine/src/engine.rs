use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use download_core::{
    filename_from_content_disposition, filename_from_url, DownloadSegment, EngineOptions,
    ProgressSnapshot, ResourceInfo, SegmentPlan,
};
use reqwest::header::{
    HeaderName, ACCEPT_ENCODING, ACCEPT_RANGES, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE,
    CONTENT_TYPE, ETAG, LAST_MODIFIED, RANGE, SERVER,
};
use reqwest::{Client, StatusCode};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use url::Url;

use crate::adaptive::{AdaptiveController, AdaptiveGate, BandwidthLimiter};
use crate::error::EngineError;
use crate::{DownloadEngine, DownloadOutcome, DownloadRequest, EngineContext};

#[derive(Default)]
pub struct NativeHttpEngine;

impl NativeHttpEngine {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl DownloadEngine for NativeHttpEngine {
    fn name(&self) -> &'static str {
        "native-http"
    }

    async fn inspect(
        &self,
        url: &Url,
        options: &EngineOptions,
    ) -> Result<ResourceInfo, EngineError> {
        let client = build_client(options, Some(Duration::from_secs(25)))?;
        let mut info = ResourceInfo {
            final_url: url.to_string(),
            ..Default::default()
        };

        let head = client.head(url.clone()).send().await;
        let mut head_error = None;
        let mut head_ok = false;
        match head {
            Ok(response) if response.status().is_success() => {
                head_ok = true;
                fill_from_response(&mut info, &response, url);
            }
            Ok(response) => {
                head_error = Some(status_error(response.status()));
            }
            Err(error) => {
                head_error = Some(EngineError::from_reqwest(error));
            }
        }

        let needs_probe = !head_ok || info.total_bytes.is_none() || !info.accepts_ranges;
        if needs_probe {
            let probe = client
                .get(url.clone())
                .header(RANGE, "bytes=0-0")
                .send()
                .await;
            match probe {
                Ok(response) => {
                    let status = response.status();
                    if status == StatusCode::PARTIAL_CONTENT {
                        info.accepts_ranges = true;
                        fill_from_response(&mut info, &response, url);
                        if let Some(range) = header_string(&response, CONTENT_RANGE) {
                            if let Some((_, _, total)) = parse_content_range(&range) {
                                info.total_bytes = total;
                            }
                        }
                    } else if status.is_success() {
                        // The server ignored the Range header. We still get a
                        // reliable size, but segmented transfer is unsafe.
                        fill_from_response(&mut info, &response, url);
                        info.accepts_ranges = false;
                    } else if !head_ok {
                        return Err(status_error(status));
                    }
                }
                Err(error) => {
                    if !head_ok {
                        return Err(EngineError::from_reqwest(error));
                    }
                    debug!(%url, "probe request failed after successful HEAD: {error}");
                }
            }
        }

        if !head_ok && info.final_url.is_empty() {
            if let Some(error) = head_error {
                return Err(error);
            }
        }
        if info.file_name.is_none() {
            info.file_name = Some(filename_from_url(url));
        }
        Ok(info)
    }

    async fn run(
        &self,
        request: DownloadRequest,
        context: EngineContext,
    ) -> Result<DownloadOutcome, EngineError> {
        let options = request.options.clone();
        let info = self.inspect(&request.url, &options).await?;

        // Validate that a resumed transfer still refers to the same resource.
        if !request.segments.is_empty() {
            if let (Some(saved), Some(fresh)) = (request.expected_size, info.total_bytes) {
                if saved != fresh {
                    return Err(EngineError::ResourceChanged);
                }
            }
            if let (Some(saved), Some(fresh)) = (request.etag.as_deref(), info.etag.as_deref()) {
                if saved != fresh {
                    return Err(EngineError::ResourceChanged);
                }
            } else if request.etag.is_none() {
                if let (Some(saved), Some(fresh)) = (
                    request.last_modified.as_deref(),
                    info.last_modified.as_deref(),
                ) {
                    if saved != fresh {
                        return Err(EngineError::ResourceChanged);
                    }
                }
            }
        }

        let total = info
            .total_bytes
            .or(request.expected_size)
            .filter(|v| *v > 0);
        let accepts_ranges = info.accepts_ranges && total.is_some();

        let Some(total) = total else {
            return self.run_unknown_length(&request, &info, &context).await;
        };

        if accepts_ranges {
            let mut segments = request.segments.clone();
            if !plan_is_valid(&segments, total) {
                if segments.iter().any(|s| s.downloaded > 0) {
                    return Err(EngineError::ResourceChanged);
                }
                segments = SegmentPlan::build(total, options.max_connections)?;
            }
            if segments.is_empty() {
                segments = SegmentPlan::build(total, options.max_connections)?;
            }
            self.run_ranged(&request, &info, &context, total, segments)
                .await
        } else {
            // Single connection: restart from a clean slate because byte ranges
            // are unavailable, so appending would be unsafe.
            self.run_single_known(&request, &info, &context, total)
                .await
        }
    }
}

struct SegmentCell {
    index: u32,
    start: u64,
    end: u64,
    downloaded: AtomicU64,
    lock: Mutex<()>,
}

impl SegmentCell {
    fn snapshot(&self) -> DownloadSegment {
        DownloadSegment {
            index: self.index,
            start: self.start,
            end: self.end,
            downloaded: self
                .downloaded
                .load(Ordering::Relaxed)
                .min(self.end.saturating_sub(self.start).saturating_add(1)),
        }
    }
}

struct Shared {
    cells: Vec<SegmentCell>,
    queue: Mutex<VecDeque<usize>>,
    downloaded: AtomicU64,
    errors: AtomicU64,
    gate: AdaptiveGate,
    abort: CancellationToken,
    total: u64,
    use_ranges: bool,
    limiter: Option<Arc<BandwidthLimiter>>,
}

impl Shared {
    fn snapshot_segments(&self) -> Vec<DownloadSegment> {
        self.cells.iter().map(SegmentCell::snapshot).collect()
    }

    fn all_complete(&self) -> bool {
        self.cells.iter().all(|cell| {
            cell.downloaded.load(Ordering::Relaxed)
                >= cell.end.saturating_sub(cell.start).saturating_add(1)
        })
    }
}

#[derive(Clone, Copy, Debug)]
enum WorkerStop {
    Pause,
    Cancel,
    Abort,
}

enum TransferError {
    Stop(WorkerStop),
    Retry(EngineError),
    Fatal(EngineError),
}

impl NativeHttpEngine {
    async fn run_ranged(
        &self,
        request: &DownloadRequest,
        info: &ResourceInfo,
        context: &EngineContext,
        total: u64,
        segments: Vec<DownloadSegment>,
    ) -> Result<DownloadOutcome, EngineError> {
        if let Some(parent) = request.destination.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(EngineError::from_io)?;
        }

        prepare_file(&request.destination, total, false).await?;

        let initial_downloaded: u64 = segments
            .iter()
            .map(|s| s.downloaded.min(s.len()))
            .sum::<u64>();
        let cells: Vec<SegmentCell> = segments
            .iter()
            .map(|segment| SegmentCell {
                index: segment.index,
                start: segment.start,
                end: segment.end,
                downloaded: AtomicU64::new(segment.downloaded.min(segment.len())),
                lock: Mutex::new(()),
            })
            .collect();
        let queue: VecDeque<usize> = segments
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.is_complete())
            .map(|(index, _)| index)
            .collect();

        if queue.is_empty() {
            return Ok(DownloadOutcome::Completed {
                downloaded: total,
                total: Some(total),
                segments,
            });
        }

        let shared = Arc::new(Shared {
            cells,
            queue: Mutex::new(queue),
            downloaded: AtomicU64::new(initial_downloaded),
            errors: AtomicU64::new(0),
            gate: AdaptiveGate::new(request.options.max_connections),
            abort: CancellationToken::new(),
            total,
            use_ranges: true,
            limiter: request
                .options
                .bandwidth_limit_bps
                .filter(|v| *v > 0)
                .map(|rate| Arc::new(BandwidthLimiter::new(rate))),
        });

        let client = build_client(&request.options, None)?;
        let worker_count = (segments.len() as u32)
            .min(request.options.max_connections.max(1))
            .max(1) as usize;

        info!(
            task = %request.task_id,
            connections = worker_count,
            total,
            resumed_from = initial_downloaded,
            "starting ranged download"
        );

        let monitor_stop = CancellationToken::new();
        let monitor = spawn_monitor(
            shared.clone(),
            context.progress.clone(),
            request.options.max_connections,
            monitor_stop.clone(),
        );

        let mut set = JoinSet::new();
        for worker_id in 0..worker_count {
            let worker_shared = shared.clone();
            let worker_client = client.clone();
            let worker_url = request.url.clone();
            let destination = request.destination.clone();
            let etag = request.etag.clone().or_else(|| info.etag.clone());
            let last_modified = request
                .last_modified
                .clone()
                .or_else(|| info.last_modified.clone());
            let pause = context.pause.clone();
            let cancel = context.cancel.clone();
            let max_retries = request.options.max_retries;
            set.spawn(async move {
                worker_loop(
                    worker_id as u32,
                    worker_client,
                    worker_url,
                    destination,
                    etag,
                    last_modified,
                    worker_shared,
                    pause,
                    cancel,
                    max_retries,
                )
                .await
            });
        }

        let mut first_error: Option<EngineError> = None;
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                        shared.abort.cancel();
                    }
                }
                Err(join_error) => {
                    if first_error.is_none() {
                        first_error = Some(EngineError::Internal(format!(
                            "download worker panicked: {join_error}"
                        )));
                        shared.abort.cancel();
                    }
                }
            }
        }

        monitor_stop.cancel();
        let _ = monitor.await;

        if let Some(error) = first_error {
            warn!(task = %request.task_id, "download failed: {error}");
            return Err(error);
        }

        sync_file(&request.destination).await?;
        let segments = shared.snapshot_segments();
        let downloaded = segments
            .iter()
            .map(|s| s.downloaded.min(s.len()))
            .sum::<u64>();

        if context.cancel.is_cancelled() {
            return Ok(DownloadOutcome::Cancelled {
                downloaded,
                total: Some(total),
                segments,
            });
        }
        if context.pause.is_cancelled() {
            return Ok(DownloadOutcome::Paused {
                downloaded,
                total: Some(total),
                segments,
            });
        }
        if !shared.all_complete() || downloaded < total {
            return Err(EngineError::Internal(format!(
                "workers stopped before completing the resource ({downloaded}/{total} bytes)"
            )));
        }

        let _ = context.progress.try_send(ProgressSnapshot {
            downloaded_bytes: total,
            total_bytes: Some(total),
            speed_bps: 0.0,
            active_connections: 0,
            segments: segments.clone(),
            adaptive_note: None,
        });

        info!(task = %request.task_id, total, "ranged download completed");
        Ok(DownloadOutcome::Completed {
            downloaded: total,
            total: Some(total),
            segments,
        })
    }

    async fn run_single_known(
        &self,
        request: &DownloadRequest,
        info: &ResourceInfo,
        context: &EngineContext,
        total: u64,
    ) -> Result<DownloadOutcome, EngineError> {
        if let Some(parent) = request.destination.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(EngineError::from_io)?;
        }
        prepare_file(&request.destination, total, true).await?;

        let shared = Arc::new(Shared {
            cells: vec![SegmentCell {
                index: 0,
                start: 0,
                end: total - 1,
                downloaded: AtomicU64::new(0),
                lock: Mutex::new(()),
            }],
            queue: Mutex::new(VecDeque::from([0usize])),
            downloaded: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            gate: AdaptiveGate::new(1),
            abort: CancellationToken::new(),
            total,
            use_ranges: false,
            limiter: request
                .options
                .bandwidth_limit_bps
                .filter(|v| *v > 0)
                .map(|rate| Arc::new(BandwidthLimiter::new(rate))),
        });

        let client = build_client(&request.options, None)?;
        let monitor_stop = CancellationToken::new();
        let monitor = spawn_monitor(
            shared.clone(),
            context.progress.clone(),
            request.options.max_connections,
            monitor_stop.clone(),
        );

        let mut set = JoinSet::new();
        {
            let worker_shared = shared.clone();
            let worker_client = client.clone();
            let worker_url = request.url.clone();
            let destination = request.destination.clone();
            let etag = request.etag.clone().or_else(|| info.etag.clone());
            let last_modified = request
                .last_modified
                .clone()
                .or_else(|| info.last_modified.clone());
            let pause = context.pause.clone();
            let cancel = context.cancel.clone();
            let max_retries = request.options.max_retries;
            set.spawn(async move {
                worker_loop(
                    0,
                    worker_client,
                    worker_url,
                    destination,
                    etag,
                    last_modified,
                    worker_shared,
                    pause,
                    cancel,
                    max_retries,
                )
                .await
            });
        }

        let mut first_error = None;
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                        shared.abort.cancel();
                    }
                }
                Err(join_error) => {
                    if first_error.is_none() {
                        first_error = Some(EngineError::Internal(format!(
                            "download worker panicked: {join_error}"
                        )));
                        shared.abort.cancel();
                    }
                }
            }
        }

        monitor_stop.cancel();
        let _ = monitor.await;
        if let Some(error) = first_error {
            return Err(error);
        }
        sync_file(&request.destination).await?;

        let segments = shared.snapshot_segments();
        let downloaded = segments.first().map(|s| s.downloaded).unwrap_or(0);
        if context.cancel.is_cancelled() {
            return Ok(DownloadOutcome::Cancelled {
                downloaded,
                total: Some(total),
                segments,
            });
        }
        if context.pause.is_cancelled() || downloaded < total {
            return Ok(DownloadOutcome::Paused {
                downloaded,
                total: Some(total),
                segments,
            });
        }

        Ok(DownloadOutcome::Completed {
            downloaded: total,
            total: Some(total),
            segments,
        })
    }

    async fn run_unknown_length(
        &self,
        request: &DownloadRequest,
        info: &ResourceInfo,
        context: &EngineContext,
    ) -> Result<DownloadOutcome, EngineError> {
        if let Some(parent) = request.destination.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(EngineError::from_io)?;
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&request.destination)
            .await
            .map_err(EngineError::from_io)?;

        let client = build_client(&request.options, None)?;
        let cancel = context.cancel.clone();
        let pause = context.pause.clone();

        let mut response = tokio::select! {
            _ = cancel.cancelled() => {
                return Ok(DownloadOutcome::Cancelled { downloaded: 0, total: None, segments: vec![] });
            }
            _ = pause.cancelled() => {
                return Ok(DownloadOutcome::Paused { downloaded: 0, total: None, segments: vec![] });
            }
            response = client.get(request.url.clone()).send() => response.map_err(EngineError::from_reqwest)?,
        };

        if !response.status().is_success() {
            return Err(status_error(response.status()));
        }
        if let Some(expected) = request.etag.as_deref() {
            if let Some(actual) = header_string(&response, ETAG) {
                if actual != expected {
                    return Err(EngineError::ResourceChanged);
                }
            }
        }

        let total = response.content_length();
        let mut downloaded = 0u64;
        let started = Instant::now();
        let limiter = request
            .options
            .bandwidth_limit_bps
            .filter(|v| *v > 0)
            .map(BandwidthLimiter::new);
        let mut last_report = Instant::now();
        let mut last_bytes = 0u64;
        let mut speed = 0.0f64;

        loop {
            let chunk = tokio::select! {
                _ = cancel.cancelled() => {
                    let _ = file.flush().await;
                    return Ok(DownloadOutcome::Cancelled { downloaded, total, segments: vec![] });
                }
                _ = pause.cancelled() => {
                    let _ = file.flush().await;
                    return Ok(DownloadOutcome::Paused { downloaded, total, segments: vec![] });
                }
                chunk = response.chunk() => chunk,
            };
            match chunk {
                Ok(Some(bytes)) => {
                    file.write_all(&bytes).await.map_err(EngineError::from_io)?;
                    downloaded += bytes.len() as u64;
                    if let Some(limiter) = &limiter {
                        tokio::select! {
                            _ = limiter.acquire(bytes.len() as u64) => {}
                            _ = cancel.cancelled() => { let _ = file.flush().await; return Ok(DownloadOutcome::Cancelled { downloaded, total, segments: vec![] }); }
                            _ = pause.cancelled() => { let _ = file.flush().await; return Ok(DownloadOutcome::Paused { downloaded, total, segments: vec![] }); }
                        }
                    }
                    if last_report.elapsed() >= Duration::from_millis(400) {
                        let elapsed = last_report.elapsed().as_secs_f64();
                        let current =
                            (downloaded.saturating_sub(last_bytes)) as f64 / elapsed.max(0.001);
                        speed = if speed <= 0.0 {
                            current
                        } else {
                            speed * 0.65 + current * 0.35
                        };
                        last_report = Instant::now();
                        last_bytes = downloaded;
                        let _ = context.progress.try_send(ProgressSnapshot {
                            downloaded_bytes: downloaded,
                            total_bytes: total,
                            speed_bps: speed,
                            active_connections: 1,
                            segments: vec![],
                            adaptive_note: None,
                        });
                    }
                }
                Err(error) => return Err(EngineError::from_reqwest(error)),
                Ok(None) => break,
            }
        }

        file.flush().await.map_err(EngineError::from_io)?;
        file.sync_all().await.map_err(EngineError::from_io)?;
        let _ = info;
        debug!(task = %request.task_id, downloaded, elapsed = ?started.elapsed(), "unknown-length download finished");
        let _ = context.progress.try_send(ProgressSnapshot {
            downloaded_bytes: downloaded,
            total_bytes: total,
            speed_bps: 0.0,
            active_connections: 0,
            segments: vec![],
            adaptive_note: None,
        });
        Ok(DownloadOutcome::Completed {
            downloaded,
            total,
            segments: vec![],
        })
    }
}

#[allow(clippy::too_many_arguments)]
async fn worker_loop(
    worker_id: u32,
    client: Client,
    url: Url,
    destination: PathBuf,
    etag: Option<String>,
    last_modified: Option<String>,
    shared: Arc<Shared>,
    pause: CancellationToken,
    cancel: CancellationToken,
    max_retries: u32,
) -> Result<(), EngineError> {
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(&destination)
        .await
        .map_err(EngineError::from_io)?;

    loop {
        if let Some(stop) = stop_requested(&pause, &cancel, &shared.abort) {
            return stop_result(stop);
        }

        shared.gate.acquire().await;
        let next = {
            let mut queue = shared.queue.lock().await;
            queue.pop_front()
        };
        let Some(cell_index) = next else {
            shared.gate.release().await;
            return Ok(());
        };

        let result = process_segment(
            worker_id,
            cell_index,
            &mut file,
            &client,
            &url,
            &etag,
            &last_modified,
            &shared,
            &pause,
            &cancel,
            max_retries,
        )
        .await;
        shared.gate.release().await;

        match result {
            Ok(None) => {}
            Ok(Some(WorkerStop::Pause)) => {
                shared.queue.lock().await.push_back(cell_index);
                return Ok(());
            }
            Ok(Some(WorkerStop::Cancel)) => return Ok(()),
            Ok(Some(WorkerStop::Abort)) => return Ok(()),
            Err(error) => {
                shared.queue.lock().await.push_back(cell_index);
                return Err(error);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn process_segment(
    _worker_id: u32,
    cell_index: usize,
    file: &mut tokio::fs::File,
    client: &Client,
    url: &Url,
    etag: &Option<String>,
    last_modified: &Option<String>,
    shared: &Arc<Shared>,
    pause: &CancellationToken,
    cancel: &CancellationToken,
    max_retries: u32,
) -> Result<Option<WorkerStop>, EngineError> {
    let cell = &shared.cells[cell_index];
    let _guard = cell.lock.lock().await;

    let total_len = cell.end.saturating_sub(cell.start).saturating_add(1);
    if cell.downloaded.load(Ordering::Relaxed) >= total_len {
        return Ok(None);
    }

    let mut attempt = 0u32;
    loop {
        if let Some(stop) = stop_requested(pause, cancel, &shared.abort) {
            return Ok(Some(stop));
        }

        let offset = if shared.use_ranges {
            cell.start + cell.downloaded.load(Ordering::Relaxed)
        } else {
            // Without byte-range support a partial body cannot be appended
            // safely, so every attempt restarts from byte zero.
            let previous = cell.downloaded.swap(0, Ordering::Relaxed);
            if previous > 0 {
                shared.downloaded.fetch_sub(previous, Ordering::Relaxed);
            }
            0
        };
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(EngineError::from_io)?;

        match transfer_range(
            cell,
            file,
            client,
            url,
            etag,
            last_modified,
            shared,
            pause,
            cancel,
            offset,
        )
        .await
        {
            Ok(()) => return Ok(None),
            Err(TransferError::Stop(stop)) => return Ok(Some(stop)),
            Err(TransferError::Fatal(error)) => return Err(error),
            Err(TransferError::Retry(error)) => {
                shared.errors.fetch_add(1, Ordering::Relaxed);
                attempt += 1;
                if attempt > max_retries {
                    return Err(error);
                }
                let delay = Duration::from_millis(300u64 * 2u64.pow(attempt.min(5)));
                warn!(
                    segment = cell.index,
                    attempt, "retrying segment after error: {error}"
                );
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = pause.cancelled() => return Ok(Some(WorkerStop::Pause)),
                    _ = cancel.cancelled() => return Ok(Some(WorkerStop::Cancel)),
                    _ = shared.abort.cancelled() => return Ok(Some(WorkerStop::Abort)),
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn transfer_range(
    cell: &SegmentCell,
    file: &mut tokio::fs::File,
    client: &Client,
    url: &Url,
    etag: &Option<String>,
    last_modified: &Option<String>,
    shared: &Arc<Shared>,
    pause: &CancellationToken,
    cancel: &CancellationToken,
    mut offset: u64,
) -> Result<(), TransferError> {
    if offset > cell.end {
        return Ok(());
    }

    let mut request = client.get(url.clone());
    if shared.use_ranges {
        request = request.header(RANGE, format!("bytes={}-{}", offset, cell.end));
    }

    let mut response = tokio::select! {
        _ = cancel.cancelled() => return Err(TransferError::Stop(WorkerStop::Cancel)),
        _ = pause.cancelled() => return Err(TransferError::Stop(WorkerStop::Pause)),
        _ = shared.abort.cancelled() => return Err(TransferError::Stop(WorkerStop::Abort)),
        result = request.send() => result.map_err(EngineError::from_reqwest).map_err(|e| {
            if e.is_retryable() { TransferError::Retry(e) } else { TransferError::Fatal(e) }
        })?,
    };

    let status = response.status();
    if shared.use_ranges {
        if status == StatusCode::PARTIAL_CONTENT {
            let range = header_string(&response, CONTENT_RANGE);
            match range.as_deref().and_then(parse_content_range) {
                Some((start, _, _)) if start == offset => {}
                _ => return Err(TransferError::Fatal(EngineError::ResourceChanged)),
            }
        } else if status == StatusCode::RANGE_NOT_SATISFIABLE || status.is_success() {
            return Err(TransferError::Fatal(EngineError::ResourceChanged));
        } else {
            let error = status_error(status);
            return Err(if error.is_retryable() {
                TransferError::Retry(error)
            } else {
                TransferError::Fatal(error)
            });
        }
    } else if !status.is_success() {
        let error = status_error(status);
        return Err(if error.is_retryable() {
            TransferError::Retry(error)
        } else {
            TransferError::Fatal(error)
        });
    }

    if let Some(expected) = etag.as_deref() {
        if let Some(actual) = header_string(&response, ETAG) {
            if actual != expected {
                return Err(TransferError::Fatal(EngineError::ResourceChanged));
            }
        }
    } else if let Some(expected) = last_modified.as_deref() {
        if let Some(actual) = header_string(&response, LAST_MODIFIED) {
            if actual != expected {
                return Err(TransferError::Fatal(EngineError::ResourceChanged));
            }
        }
    }

    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => return Err(TransferError::Stop(WorkerStop::Cancel)),
            _ = pause.cancelled() => return Err(TransferError::Stop(WorkerStop::Pause)),
            _ = shared.abort.cancelled() => return Err(TransferError::Stop(WorkerStop::Abort)),
            chunk = response.chunk() => chunk,
        };
        match chunk {
            Ok(Some(bytes)) => {
                if bytes.is_empty() {
                    continue;
                }
                let allowed = (cell.end - offset + 1).min(bytes.len() as u64) as usize;
                file.write_all(&bytes[..allowed])
                    .await
                    .map_err(|e| TransferError::Fatal(EngineError::from_io(e)))?;
                offset += allowed as u64;
                cell.downloaded.fetch_add(allowed as u64, Ordering::Relaxed);
                shared
                    .downloaded
                    .fetch_add(allowed as u64, Ordering::Relaxed);

                if let Some(limiter) = &shared.limiter {
                    tokio::select! {
                        _ = limiter.acquire(allowed as u64) => {}
                        _ = cancel.cancelled() => return Err(TransferError::Stop(WorkerStop::Cancel)),
                        _ = pause.cancelled() => return Err(TransferError::Stop(WorkerStop::Pause)),
                        _ = shared.abort.cancelled() => return Err(TransferError::Stop(WorkerStop::Abort)),
                    }
                }
                if offset > cell.end {
                    return Ok(());
                }
            }
            Err(error) => {
                let error = EngineError::from_reqwest(error);
                return Err(if error.is_retryable() {
                    TransferError::Retry(error)
                } else {
                    TransferError::Fatal(error)
                });
            }
            Ok(None) => {
                if offset > cell.end {
                    return Ok(());
                }
                return Err(TransferError::Retry(EngineError::Network(
                    "connection closed before the range was complete".into(),
                )));
            }
        }
    }
}

fn spawn_monitor(
    shared: Arc<Shared>,
    progress: mpsc::Sender<ProgressSnapshot>,
    max_connections: u32,
    stop: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut controller = AdaptiveController::new(max_connections);
        let mut last_bytes = shared.downloaded.load(Ordering::Relaxed);
        let mut last_tick = Instant::now();
        let mut speed = 0.0f64;
        let mut note: Option<String> = None;
        let mut ticker = tokio::time::interval(Duration::from_millis(500));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = ticker.tick() => {
                    let now = Instant::now();
                    let downloaded = shared.downloaded.load(Ordering::Relaxed);
                    let elapsed = now.duration_since(last_tick).as_secs_f64().max(0.001);
                    let instant = downloaded.saturating_sub(last_bytes) as f64 / elapsed;
                    speed = if speed <= 0.0 { instant } else { speed * 0.6 + instant * 0.4 };
                    last_bytes = downloaded;
                    last_tick = now;

                    let errors = shared.errors.load(Ordering::Relaxed);
                    let adaptation = controller.on_sample(speed, errors);
                    if let Some(reason) = adaptation.reason {
                        note = Some(reason.clone());
                        shared.gate.set_limit(adaptation.connections).await;
                        debug!(connections = adaptation.connections, "{reason}");
                    }

                    let snapshot = ProgressSnapshot {
                        downloaded_bytes: downloaded,
                        total_bytes: Some(shared.total),
                        speed_bps: speed.max(0.0),
                        active_connections: shared.gate.active().await,
                        segments: shared.snapshot_segments(),
                        adaptive_note: note.take(),
                    };
                    if progress.try_send(snapshot).is_err() {
                        // The receiver may be applying backpressure. Dropping a
                        // sample is preferable to stalling the transfer.
                    }
                }
            }
        }

        let _ = progress.try_send(ProgressSnapshot {
            downloaded_bytes: shared.downloaded.load(Ordering::Relaxed),
            total_bytes: Some(shared.total),
            speed_bps: 0.0,
            active_connections: 0,
            segments: shared.snapshot_segments(),
            adaptive_note: None,
        });
    })
}

fn stop_requested(
    pause: &CancellationToken,
    cancel: &CancellationToken,
    abort: &CancellationToken,
) -> Option<WorkerStop> {
    if cancel.is_cancelled() {
        Some(WorkerStop::Cancel)
    } else if abort.is_cancelled() {
        Some(WorkerStop::Abort)
    } else if pause.is_cancelled() {
        Some(WorkerStop::Pause)
    } else {
        None
    }
}

fn stop_result(stop: WorkerStop) -> Result<(), EngineError> {
    match stop {
        WorkerStop::Pause | WorkerStop::Cancel | WorkerStop::Abort => Ok(()),
    }
}

fn plan_is_valid(segments: &[DownloadSegment], total: u64) -> bool {
    if segments.is_empty() {
        return false;
    }
    let mut expected = 0u64;
    for segment in segments {
        if segment.start != expected
            || segment.end < segment.start
            || segment.downloaded > segment.len()
        {
            return false;
        }
        expected = segment.end + 1;
    }
    expected == total
}

async fn prepare_file(path: &Path, total: u64, truncate: bool) -> Result<(), EngineError> {
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
        .await
        .map_err(EngineError::from_io)?;
    if truncate {
        file.set_len(0).await.map_err(EngineError::from_io)?;
    }
    let current = file.metadata().await.map_err(EngineError::from_io)?.len();
    if current != total {
        file.set_len(total).await.map_err(EngineError::from_io)?;
    }
    Ok(())
}

async fn sync_file(path: &Path) -> Result<(), EngineError> {
    let file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .await
        .map_err(EngineError::from_io)?;
    file.sync_all().await.map_err(EngineError::from_io)
}

fn build_client(
    options: &EngineOptions,
    overall_timeout: Option<Duration>,
) -> Result<Client, EngineError> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        ACCEPT_ENCODING,
        reqwest::header::HeaderValue::from_static("identity"),
    );

    let mut builder = Client::builder()
        .user_agent(options.user_agent.clone())
        .default_headers(headers)
        .connect_timeout(Duration::from_secs(options.timeout_secs.clamp(3, 120)))
        .read_timeout(Duration::from_secs(options.timeout_secs.clamp(10, 300)))
        .redirect(reqwest::redirect::Policy::limited(10))
        .pool_max_idle_per_host(32)
        .tcp_keepalive(Duration::from_secs(30))
        .gzip(false)
        .brotli(false)
        .deflate(false);

    if let Some(timeout) = overall_timeout {
        builder = builder.timeout(timeout);
    }
    if let Some(proxy) = options.proxy.as_deref().filter(|p| !p.trim().is_empty()) {
        let proxy = reqwest::Proxy::all(proxy)
            .map_err(|e| EngineError::InvalidRequest(format!("invalid proxy: {e}")))?;
        builder = builder.proxy(proxy);
    }

    builder
        .build()
        .map_err(|e| EngineError::Internal(format!("failed to build HTTP client: {e}")))
}

fn fill_from_response(info: &mut ResourceInfo, response: &reqwest::Response, url: &Url) {
    info.final_url = response.url().to_string();
    if info.file_name.is_none() {
        if let Some(disposition) = header_string(response, CONTENT_DISPOSITION) {
            info.file_name = filename_from_content_disposition(&disposition);
        }
    }
    if info.file_name.is_none() {
        info.file_name = Some(filename_from_url(url));
    }
    if let Some(length) =
        header_string(response, CONTENT_LENGTH).and_then(|v| v.parse::<u64>().ok())
    {
        info.total_bytes = Some(length);
    }
    if let Some(ranges) = header_string(response, ACCEPT_RANGES) {
        info.accepts_ranges = ranges.trim().eq_ignore_ascii_case("bytes");
    }
    if let Some(etag) = header_string(response, ETAG) {
        info.etag = Some(etag);
    }
    if let Some(modified) = header_string(response, LAST_MODIFIED) {
        info.last_modified = Some(modified);
    }
    if let Some(content_type) = header_string(response, CONTENT_TYPE) {
        info.content_type = Some(content_type);
    }
    if let Some(server) = header_string(response, SERVER) {
        info.server = Some(server);
    }
}

fn header_string(response: &reqwest::Response, name: HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn parse_content_range(value: &str) -> Option<(u64, u64, Option<u64>)> {
    let rest = value.trim().strip_prefix("bytes")?.trim();
    let (range, total) = rest.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let start = start.trim().parse::<u64>().ok()?;
    let end = end.trim().parse::<u64>().ok()?;
    let total = total.trim().parse::<u64>().ok();
    Some((start, end, total))
}

fn status_error(status: StatusCode) -> EngineError {
    EngineError::Http {
        status: status.as_u16(),
        message: status
            .canonical_reason()
            .unwrap_or("unexpected response")
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_range_variants() {
        assert_eq!(
            parse_content_range("bytes 0-99/1000"),
            Some((0, 99, Some(1000)))
        );
        assert_eq!(parse_content_range("bytes 10-19/*"), Some((10, 19, None)));
        assert_eq!(parse_content_range("garbage"), None);
    }

    #[test]
    fn validates_plans() {
        let plan = SegmentPlan::build(1_000_000, 4).unwrap();
        assert!(plan_is_valid(&plan, 1_000_000));
        assert!(!plan_is_valid(&plan, 999_999));
        assert!(!plan_is_valid(&[], 10));
    }
}
