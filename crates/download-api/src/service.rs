use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use download_core::{
    classify_extension, filename_from_url, parse_and_validate_url, safe_destination,
    sanitize_filename, unique_path, validate_sha256, DownloadStatus, DownloadTask, EngineOptions,
    ProgressSnapshot, SegmentPlan,
};
use download_engine::{
    DownloadEngine, DownloadOutcome, DownloadRequest, EngineContext, EngineError, NativeHttpEngine,
};
use download_integrity::{verify_file, IntegrityReport, IntegrityStatus};
use download_rules::{evaluate, Rule, RuleActions, RuleContext};
use download_scheduler::{select_next, SchedulerConfig, SchedulerTask};
use download_storage::{
    IntegrityRecord, Queue, SpeedSample, Stats, Storage, StoredRule, TaskEvent, TaskFilter,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex, RwLock};
use tokio::task::AbortHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::error::ServiceError;

#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub data_dir: PathBuf,
    pub default_download_dir: PathBuf,
    pub api_token: String,
}

#[derive(Debug, Clone)]
struct RuntimeConfig {
    max_concurrent: usize,
    default_connections: u32,
    per_domain_limit: usize,
    bandwidth_limit_bps: Option<u64>,
    timeout_secs: u64,
    retry_limit: u32,
    verify_integrity: bool,
    auto_resume: bool,
    adaptive_connections: bool,
    default_destination: PathBuf,
}

impl RuntimeConfig {
    fn from_service(config: &ServiceConfig) -> Self {
        Self {
            max_concurrent: 3,
            default_connections: 4,
            per_domain_limit: 4,
            bandwidth_limit_bps: None,
            timeout_secs: 30,
            retry_limit: 4,
            verify_integrity: true,
            auto_resume: true,
            adaptive_connections: true,
            default_destination: config.default_download_dir.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServiceEvent {
    TaskUpdated {
        task: Box<DownloadTask>,
    },
    TaskRemoved {
        id: String,
    },
    Stats {
        stats: Stats,
    },
    Notification {
        level: String,
        title: String,
        message: String,
    },
}

struct TaskControl {
    pause: CancellationToken,
    cancel: CancellationToken,
    abort: AbortHandle,
}

struct Inner {
    storage: Storage,
    engine: Arc<dyn DownloadEngine>,
    config: ServiceConfig,
    runtime: RwLock<RuntimeConfig>,
    running: Mutex<HashMap<String, TaskControl>>,
    schedule_lock: Mutex<()>,
    events: broadcast::Sender<ServiceEvent>,
}

/// Cloneable application facade shared by the REST API and the desktop shell.
#[derive(Clone)]
pub struct DownloadService {
    inner: Arc<Inner>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewDownload {
    pub url: String,
    pub file_name: Option<String>,
    pub directory: Option<String>,
    pub category: Option<String>,
    pub queue_id: Option<String>,
    pub collection_id: Option<String>,
    pub connections: Option<u32>,
    pub priority: Option<i32>,
    pub expected_checksum: Option<String>,
    #[serde(default)]
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddResult {
    pub task: DownloadTask,
    pub duplicate_of: Option<String>,
    pub rule_applied: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostics {
    pub task_id: String,
    pub summary: String,
    pub error_kind: Option<String>,
    pub error: Option<String>,
    pub hints: Vec<String>,
    pub source_reachable: Option<bool>,
    pub resource: Option<download_core::ResourceInfo>,
    pub events: Vec<TaskEvent>,
    pub integrity: Vec<IntegrityRecord>,
    pub suggested_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkResult {
    pub url: String,
    pub status: u16,
    pub connect_and_headers_ms: f64,
    pub download_ms: f64,
    pub bytes: u64,
    pub throughput_bps: f64,
    pub dns_ms: Option<f64>,
    pub tls_ms: Option<f64>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    pub checksum: String,
    pub tasks: Vec<DownloadTask>,
}

impl DownloadService {
    pub fn new(storage: Storage, engine: Arc<dyn DownloadEngine>, config: ServiceConfig) -> Self {
        let runtime = RuntimeConfig::from_service(&config);
        let (events, _) = broadcast::channel(256);
        Self {
            inner: Arc::new(Inner {
                storage,
                engine,
                config,
                runtime: RwLock::new(runtime),
                running: Mutex::new(HashMap::new()),
                schedule_lock: Mutex::new(()),
                events,
            }),
        }
    }

    pub fn with_native_engine(storage: Storage, config: ServiceConfig) -> Self {
        Self::new(storage, Arc::new(NativeHttpEngine::new()), config)
    }

    pub fn storage(&self) -> &Storage {
        &self.inner.storage
    }

    pub fn token(&self) -> &str {
        &self.inner.config.api_token
    }

    pub fn data_dir(&self) -> &Path {
        &self.inner.config.data_dir
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServiceEvent> {
        self.inner.events.subscribe()
    }

    pub async fn refresh_config(&self) -> Result<(), ServiceError> {
        let settings = self.inner.storage.settings().await?;
        let mut runtime = self.inner.runtime.write().await;
        if let Some(value) = settings
            .get("max_concurrent_downloads")
            .and_then(Value::as_u64)
        {
            runtime.max_concurrent = value.clamp(1, 16) as usize;
        }
        if let Some(value) = settings.get("default_connections").and_then(Value::as_u64) {
            runtime.default_connections = value.clamp(1, 32) as u32;
        }
        if let Some(value) = settings.get("bandwidth_limit_bps").and_then(Value::as_u64) {
            runtime.bandwidth_limit_bps = (value > 0).then_some(value);
        }
        if let Some(value) = settings.get("timeout_secs").and_then(Value::as_u64) {
            runtime.timeout_secs = value.clamp(5, 600);
        }
        if let Some(value) = settings.get("retry_limit").and_then(Value::as_u64) {
            runtime.retry_limit = value.min(20) as u32;
        }
        if let Some(value) = settings.get("verify_integrity").and_then(Value::as_bool) {
            runtime.verify_integrity = value;
        }
        if let Some(value) = settings.get("auto_resume").and_then(Value::as_bool) {
            runtime.auto_resume = value;
        }
        if let Some(value) = settings
            .get("adaptive_connections")
            .and_then(Value::as_bool)
        {
            runtime.adaptive_connections = value;
        }
        if let Some(value) = settings.get("per_domain_limit").and_then(Value::as_u64) {
            runtime.per_domain_limit = value.clamp(1, 32) as usize;
        }
        if let Some(value) = settings.get("default_destination").and_then(Value::as_str) {
            if !value.trim().is_empty() {
                runtime.default_destination = expand_home(value);
            }
        }
        Ok(())
    }

    /// Recover interrupted tasks and start the scheduler.
    pub async fn initialize(&self) -> Result<usize, ServiceError> {
        self.refresh_config().await?;
        let auto_resume = self.inner.runtime.read().await.auto_resume;
        let tasks = self
            .inner
            .storage
            .tasks(&TaskFilter {
                limit: 5000,
                ..Default::default()
            })
            .await?;
        let mut recovered = 0usize;
        for mut task in tasks {
            if !matches!(
                task.status,
                DownloadStatus::Preparing
                    | DownloadStatus::Connecting
                    | DownloadStatus::Downloading
                    | DownloadStatus::Pausing
                    | DownloadStatus::Verifying
            ) {
                continue;
            }
            // Disk writes may not have been flushed at the moment of a crash.
            // Rewinding one megabyte per segment guarantees that no checkpoint
            // can point past durable file contents.
            const SAFETY_REWIND: u64 = 1024 * 1024;
            for segment in &mut task.segments {
                segment.downloaded = segment.downloaded.saturating_sub(SAFETY_REWIND);
            }
            task.recompute_downloaded();
            task.speed_bps = 0.0;
            task.error = None;
            task.error_kind = None;
            self.inner
                .storage
                .save_progress(
                    &task.id,
                    task.downloaded_bytes.max(0) as u64,
                    task.total_bytes.map(|v| v as u64),
                    0.0,
                    &task.segments,
                )
                .await?;
            let next = if auto_resume {
                DownloadStatus::Queued
            } else {
                DownloadStatus::Paused
            };
            self.inner
                .storage
                .set_status(&task.id, next, None, None, None)
                .await?;
            self.inner
                .storage
                .record_event(
                    &task.id,
                    Some(task.status),
                    next,
                    Some("recovered after application restart"),
                )
                .await?;
            recovered += 1;
        }
        info!(recovered, "startup recovery complete");
        let storage = self.inner.storage.clone();
        tokio::spawn(async move {
            let _ = storage.prune_speed_samples(30).await;
        });
        self.schedule().await?;
        Ok(recovered)
    }

    pub async fn add_task(&self, request: NewDownload) -> Result<AddResult, ServiceError> {
        let url = parse_and_validate_url(&request.url)?;
        let runtime = self.inner.runtime.read().await.clone();
        let connections = request
            .connections
            .unwrap_or(runtime.default_connections)
            .clamp(1, 32);
        let options = self.engine_options(connections, &runtime);
        let info = self.inner.engine.inspect(&url, &options).await?;

        let mut file_name = request
            .file_name
            .clone()
            .or_else(|| info.file_name.clone())
            .unwrap_or_else(|| filename_from_url(&url));
        file_name = sanitize_filename(&file_name);

        let rules = self.load_rules().await?;
        let context = RuleContext::new(
            &file_name,
            &url,
            info.total_bytes,
            info.content_type.as_deref(),
        );
        let matched = evaluate(&rules, &context);
        let mut actions = RuleActions::default();
        let mut rule_applied = None;
        if let Some(found) = matched {
            if found.actions.require_confirmation && !request.confirmed {
                return Err(ServiceError::ConfirmationRequired {
                    rule: found.rule.name.clone(),
                });
            }
            actions = found.actions.clone();
            rule_applied = Some(found.rule.name.clone());
        }

        let directory = request
            .directory
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .map(expand_home)
            .or_else(|| actions.destination_dir.as_deref().map(expand_home))
            .unwrap_or_else(|| runtime.default_destination.clone());
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(|e| ServiceError::Io(format!("cannot create destination: {e}")))?;
        let file_path = unique_path(&directory, &file_name)?;
        let file_name = file_path
            .file_name()
            .and_then(|v| v.to_str())
            .map(str::to_string)
            .unwrap_or(file_name);

        let category = request
            .category
            .clone()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| actions.category.clone())
            .unwrap_or_else(|| classify_extension(&file_name).to_string());

        let queues = self.inner.storage.queues().await?;
        let queue_id = request
            .queue_id
            .clone()
            .or_else(|| actions.queue_id.clone())
            .filter(|id| queues.iter().any(|q| &q.id == id))
            .or_else(|| queues.first().map(|q| q.id.clone()));

        let priority = request.priority.or(actions.priority).unwrap_or(0);
        let connections = request
            .connections
            .or(actions.connections)
            .unwrap_or(connections)
            .clamp(1, 32);
        let expected_checksum = request
            .expected_checksum
            .as_deref()
            .map(validate_sha256)
            .transpose()?;

        let total = info.total_bytes;
        let segments = if let Some(total) = total {
            if info.accepts_ranges {
                SegmentPlan::build(total, connections)?
            } else {
                SegmentPlan::single(Some(total))
            }
        } else {
            Vec::new()
        };

        let now = Utc::now();
        let task = DownloadTask {
            id: uuid::Uuid::new_v4().to_string(),
            url: url.to_string(),
            final_url: Some(info.final_url.clone()).filter(|v| !v.is_empty()),
            file_name,
            destination_dir: directory.to_string_lossy().to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            category,
            queue_id,
            collection_id: request.collection_id.clone(),
            status: DownloadStatus::Queued,
            priority,
            total_bytes: total.map(|v| v as i64),
            downloaded_bytes: 0,
            connections,
            speed_bps: 0.0,
            error: None,
            error_kind: None,
            etag: info.etag.clone(),
            last_modified: info.last_modified.clone(),
            accepts_ranges: info.accepts_ranges,
            content_type: info.content_type.clone(),
            checksum_sha256: None,
            expected_checksum,
            source_domain: url.host_str().map(str::to_string),
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            segments,
        };

        let duplicate_of = self.find_duplicate(&task).await?;
        self.inner.storage.save_task(&task).await?;
        self.inner
            .storage
            .record_event(
                &task.id,
                None,
                DownloadStatus::Queued,
                Some("download created"),
            )
            .await?;
        self.emit_task(&task.id).await;
        self.schedule().await?;
        let task = self.inner.storage.task(&task.id).await?;
        Ok(AddResult {
            task,
            duplicate_of,
            rule_applied,
        })
    }

    fn engine_options(&self, connections: u32, runtime: &RuntimeConfig) -> EngineOptions {
        EngineOptions {
            max_connections: connections.clamp(1, 32),
            timeout_secs: runtime.timeout_secs,
            max_retries: runtime.retry_limit,
            bandwidth_limit_bps: runtime.bandwidth_limit_bps,
            ..EngineOptions::default()
        }
    }

    async fn load_rules(&self) -> Result<Vec<Rule>, ServiceError> {
        let stored = self.inner.storage.rules().await?;
        stored
            .iter()
            .map(rule_from_stored)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ServiceError::Rule(e.to_string()))
    }

    async fn find_duplicate(&self, task: &DownloadTask) -> Result<Option<String>, ServiceError> {
        let existing = self
            .inner
            .storage
            .tasks(&TaskFilter {
                statuses: vec![DownloadStatus::Completed],
                limit: 200,
                ..Default::default()
            })
            .await?;
        Ok(existing
            .into_iter()
            .find(|other| {
                other.id != task.id
                    && (other.url == task.url
                        || (other.file_name == task.file_name
                            && other.total_bytes == task.total_bytes))
            })
            .map(|other| other.id))
    }

    fn schedule(&self) -> Pin<Box<dyn Future<Output = Result<(), ServiceError>> + Send + '_>> {
        Box::pin(async move {
            let _guard = self.inner.schedule_lock.lock().await;
            let runtime = self.inner.runtime.read().await.clone();

            for _ in 0..32 {
                let running = self.inner.running.lock().await.len();
                if running >= runtime.max_concurrent {
                    break;
                }
                let tasks = self
                    .inner
                    .storage
                    .tasks(&TaskFilter {
                        limit: 5000,
                        ..Default::default()
                    })
                    .await?;
                let queues = self.inner.storage.queues().await?;
                let queue_map: HashMap<&str, &Queue> = queues
                    .iter()
                    .map(|queue| (queue.id.as_str(), queue))
                    .collect();
                let scheduler_tasks: Vec<SchedulerTask> = tasks
                    .iter()
                    .map(|task| {
                        let queue = task
                            .queue_id
                            .as_deref()
                            .and_then(|id| queue_map.get(id).copied());
                        SchedulerTask {
                            id: task.id.clone(),
                            status: task.status.as_str().to_string(),
                            priority: task.priority,
                            queue_id: task.queue_id.clone(),
                            queue_priority: queue.map(|q| q.priority).unwrap_or(0),
                            queue_status: queue
                                .map(|q| q.status.clone())
                                .unwrap_or_else(|| "active".into()),
                            queue_max_concurrent: queue.map(|q| q.max_concurrent).unwrap_or(2),
                            domain: task.source_domain.clone(),
                            created_at: task.created_at,
                        }
                    })
                    .collect();
                let scheduler_config = SchedulerConfig {
                    max_concurrent: runtime.max_concurrent,
                    per_domain_limit: runtime.per_domain_limit,
                    per_queue_limit: None,
                };
                let Some(next) = select_next(&scheduler_tasks, &scheduler_config) else {
                    break;
                };
                if let Err(error) = self.start_task(&next).await {
                    warn!(task = %next, "failed to start queued download: {error}");
                }
            }
            Ok(())
        })
    }

    async fn start_task(&self, id: &str) -> Result<(), ServiceError> {
        if self.inner.running.lock().await.contains_key(id) {
            return Ok(());
        }
        let task = self.inner.storage.task(id).await?;
        if !matches!(
            task.status,
            DownloadStatus::Queued
                | DownloadStatus::Paused
                | DownloadStatus::Failed
                | DownloadStatus::Cancelled
                | DownloadStatus::WaitingForNetwork
        ) {
            return Err(ServiceError::Conflict(format!(
                "task is in state {} and cannot start",
                task.status
            )));
        }
        let runtime = self.inner.runtime.read().await.clone();
        self.inner
            .storage
            .set_status(id, DownloadStatus::Preparing, None, None, None)
            .await?;
        self.inner
            .storage
            .record_event(id, Some(task.status), DownloadStatus::Preparing, None)
            .await?;
        self.emit_task(id).await;

        tokio::fs::create_dir_all(&task.destination_dir)
            .await
            .map_err(|e| ServiceError::Io(format!("cannot create destination: {e}")))?;

        let url = parse_and_validate_url(&task.url)?;
        let options = self.engine_options(task.connections, &runtime);
        let info = match self.inner.engine.inspect(&url, &options).await {
            Ok(info) => info,
            Err(error) => {
                self.mark_failed(id, &error, task.status).await;
                return Err(ServiceError::Engine(error));
            }
        };

        let total = info.total_bytes.or(task.total_bytes.map(|v| v as u64));
        let mut segments = task.segments.clone();
        let remote_changed = task
            .total_bytes
            .map(|saved| Some(saved as u64) != info.total_bytes)
            .unwrap_or(false)
            || task
                .etag
                .as_deref()
                .zip(info.etag.as_deref())
                .map(|(saved, fresh)| saved != fresh)
                .unwrap_or(false);
        if remote_changed || !info.accepts_ranges {
            segments.clear();
        }
        if segments.is_empty() {
            segments = match total {
                Some(total) if info.accepts_ranges => SegmentPlan::build(total, task.connections)?,
                Some(total) => SegmentPlan::single(Some(total)),
                None => Vec::new(),
            };
        }
        let downloaded: u64 = segments.iter().map(|s| s.downloaded).sum();

        self.inner
            .storage
            .save_metadata(
                id,
                Some(&info.final_url),
                total,
                info.etag.as_deref(),
                info.last_modified.as_deref(),
                info.accepts_ranges,
                info.content_type.as_deref(),
                url.host_str(),
            )
            .await?;
        self.inner
            .storage
            .save_progress(id, downloaded, total, 0.0, &segments)
            .await?;
        self.inner.storage.set_started(id, Utc::now()).await?;
        self.inner
            .storage
            .set_status(id, DownloadStatus::Connecting, None, None, None)
            .await?;
        self.inner
            .storage
            .record_event(
                id,
                Some(DownloadStatus::Preparing),
                DownloadStatus::Connecting,
                if remote_changed {
                    Some("remote resource changed; restarting from a clean plan")
                } else {
                    None
                },
            )
            .await?;
        self.emit_task(id).await;

        let destination = PathBuf::from(&task.file_path);
        let request = DownloadRequest {
            task_id: id.to_string(),
            url,
            destination,
            expected_size: total,
            etag: info.etag.clone().or(task.etag.clone()),
            last_modified: info.last_modified.clone().or(task.last_modified.clone()),
            accepts_ranges: info.accepts_ranges,
            segments,
            options,
        };

        let (progress_tx, progress_rx) = mpsc::channel::<ProgressSnapshot>(8);
        let context = EngineContext::new(progress_tx);
        let pause = context.pause.clone();
        let cancel = context.cancel.clone();
        let (gate_tx, gate_rx) = oneshot::channel::<()>();

        let service = self.clone();
        let task_id = id.to_string();
        let handle = tokio::spawn(async move {
            let _ = gate_rx.await;
            service
                .run_transfer(task_id, request, context, progress_rx)
                .await;
        });
        {
            let mut running = self.inner.running.lock().await;
            running.insert(
                id.to_string(),
                TaskControl {
                    pause,
                    cancel,
                    abort: handle.abort_handle(),
                },
            );
        }
        let _ = gate_tx.send(());
        Ok(())
    }

    async fn run_transfer(
        self,
        id: String,
        request: DownloadRequest,
        context: EngineContext,
        mut progress: mpsc::Receiver<ProgressSnapshot>,
    ) {
        let engine = self.inner.engine.clone();
        let future = engine.run(request, context);
        tokio::pin!(future);

        let mut last_persist = Instant::now() - Duration::from_secs(2);
        let mut last_sample = Instant::now();
        let mut marked_downloading = false;

        let result = loop {
            tokio::select! {
                result = &mut future => break result,
                sample = progress.recv() => {
                    if let Some(sample) = sample {
                        if last_persist.elapsed() >= Duration::from_millis(900) {
                            let _ = self.inner.storage.save_progress(
                                &id,
                                sample.downloaded_bytes,
                                sample.total_bytes,
                                sample.speed_bps,
                                &sample.segments,
                            ).await;
                            if !marked_downloading {
                                if let Ok(current) = self.inner.storage.task(&id).await {
                                    if matches!(current.status, DownloadStatus::Preparing | DownloadStatus::Connecting) {
                                        let _ = self.inner.storage.set_status(&id, DownloadStatus::Downloading, None, None, None).await;
                                        marked_downloading = true;
                                        self.emit_task(&id).await;
                                    }
                                }
                            }
                            last_persist = Instant::now();
                        }
                        if last_sample.elapsed() >= Duration::from_secs(5) && sample.speed_bps > 0.0 {
                            let _ = self.inner.storage.record_speed_sample(&id, sample.speed_bps, sample.downloaded_bytes).await;
                            last_sample = Instant::now();
                        }
                    }
                }
            }
        };

        match result {
            Ok(outcome) => {
                if let Err(error) = self.apply_outcome(&id, outcome).await {
                    error!(task = %id, "failed to finalize download: {error}");
                }
            }
            Err(error) => {
                self.mark_failed(&id, &error, DownloadStatus::Downloading)
                    .await;
            }
        }

        self.inner.running.lock().await.remove(&id);
        self.emit_task(&id).await;
        if let Err(error) = self.schedule().await {
            error!("scheduler error after task completion: {error}");
        }
    }

    async fn apply_outcome(&self, id: &str, outcome: DownloadOutcome) -> Result<(), ServiceError> {
        match outcome {
            DownloadOutcome::Completed {
                downloaded,
                total,
                segments,
            } => {
                self.inner
                    .storage
                    .save_progress(id, downloaded, total, 0.0, &segments)
                    .await?;
                self.inner
                    .storage
                    .set_status(id, DownloadStatus::Verifying, None, None, None)
                    .await?;
                self.emit_task(id).await;

                let task = self.inner.storage.task(id).await?;
                let runtime = self.inner.runtime.read().await.clone();
                let expected_size = task.total_bytes.map(|v| v as u64);
                let should_verify = runtime.verify_integrity || task.expected_checksum.is_some();

                if should_verify {
                    match verify_file(
                        &task.file_path,
                        expected_size,
                        task.expected_checksum.as_deref(),
                    )
                    .await
                    {
                        Ok(report) => {
                            self.inner
                                .storage
                                .record_integrity(
                                    id,
                                    report.algorithm,
                                    report.expected_sha256.as_deref(),
                                    Some(&report.actual_sha256),
                                    report.status.as_str(),
                                    report.expected_size,
                                    Some(report.actual_size),
                                )
                                .await?;
                            self.inner
                                .storage
                                .set_checksum(id, &report.actual_sha256)
                                .await?;
                            if report.status == IntegrityStatus::ChecksumMismatch
                                || report.status == IntegrityStatus::SizeMismatch
                            {
                                let message = if report.status == IntegrityStatus::ChecksumMismatch
                                {
                                    "SHA-256 checksum did not match the expected value"
                                } else {
                                    "downloaded file size did not match the resource size"
                                };
                                self.inner
                                    .storage
                                    .set_status(
                                        id,
                                        DownloadStatus::Failed,
                                        Some(message),
                                        Some("integrity"),
                                        None,
                                    )
                                    .await?;
                                self.notify("error", "Integrity check failed", message);
                                return Ok(());
                            }
                        }
                        Err(error) => {
                            warn!(task = %id, "integrity check could not be completed: {error}");
                        }
                    }
                }

                self.inner
                    .storage
                    .save_progress(id, downloaded, total, 0.0, &segments)
                    .await?;
                self.inner
                    .storage
                    .set_status(id, DownloadStatus::Completed, None, None, Some(Utc::now()))
                    .await?;
                self.inner
                    .storage
                    .record_event(
                        id,
                        Some(DownloadStatus::Verifying),
                        DownloadStatus::Completed,
                        Some("download completed and verified"),
                    )
                    .await?;
                self.notify("success", "Download complete", &task.file_name);
            }
            DownloadOutcome::Paused {
                downloaded,
                total,
                segments,
            } => {
                self.inner
                    .storage
                    .save_progress(id, downloaded, total, 0.0, &segments)
                    .await?;
                self.inner
                    .storage
                    .set_status(id, DownloadStatus::Paused, None, None, None)
                    .await?;
                self.inner
                    .storage
                    .record_event(
                        id,
                        Some(DownloadStatus::Downloading),
                        DownloadStatus::Paused,
                        None,
                    )
                    .await?;
            }
            DownloadOutcome::Cancelled {
                downloaded,
                total,
                segments,
            } => {
                self.inner
                    .storage
                    .save_progress(id, downloaded, total, 0.0, &segments)
                    .await?;
                self.inner
                    .storage
                    .set_status(id, DownloadStatus::Cancelled, None, None, Some(Utc::now()))
                    .await?;
                self.inner
                    .storage
                    .record_event(
                        id,
                        Some(DownloadStatus::Downloading),
                        DownloadStatus::Cancelled,
                        None,
                    )
                    .await?;
            }
        }
        Ok(())
    }

    async fn mark_failed(&self, id: &str, error: &EngineError, from: DownloadStatus) {
        let status = if matches!(error, EngineError::Cancelled) {
            DownloadStatus::Cancelled
        } else {
            DownloadStatus::Failed
        };
        let message = if status == DownloadStatus::Cancelled {
            None
        } else {
            Some(error.user_message())
        };
        let _ = self
            .inner
            .storage
            .set_status(
                id,
                status,
                message.as_deref(),
                Some(error.kind()),
                (status == DownloadStatus::Cancelled).then(Utc::now),
            )
            .await;
        let _ = self
            .inner
            .storage
            .record_event(id, Some(from), status, Some(&error.to_string()))
            .await;
        if status == DownloadStatus::Failed {
            self.notify("error", "Download failed", &error.user_message());
        }
        self.emit_task(id).await;
    }

    pub async fn pause_task(&self, id: &str) -> Result<DownloadTask, ServiceError> {
        let task = self.inner.storage.task(id).await?;
        let control = self
            .inner
            .running
            .lock()
            .await
            .get(id)
            .map(|c| c.pause.clone());
        if let Some(pause) = control {
            self.inner
                .storage
                .set_status(id, DownloadStatus::Pausing, None, None, None)
                .await?;
            self.inner
                .storage
                .record_event(id, Some(task.status), DownloadStatus::Pausing, None)
                .await?;
            pause.cancel();
        } else if matches!(
            task.status,
            DownloadStatus::Queued | DownloadStatus::Preparing | DownloadStatus::Connecting
        ) {
            self.inner
                .storage
                .set_status(id, DownloadStatus::Paused, None, None, None)
                .await?;
        } else {
            return Err(ServiceError::Conflict(format!(
                "cannot pause a task in state {}",
                task.status
            )));
        }
        self.emit_task(id).await;
        self.inner.storage.task(id).await.map_err(Into::into)
    }

    pub async fn resume_task(&self, id: &str, restart: bool) -> Result<DownloadTask, ServiceError> {
        let task = self.inner.storage.task(id).await?;
        if self.inner.running.lock().await.contains_key(id) {
            return Err(ServiceError::Conflict("task is already running".into()));
        }
        if !task.status.is_startable() {
            return Err(ServiceError::Conflict(format!(
                "cannot resume a task in state {}",
                task.status
            )));
        }
        let reset = restart || !task.accepts_ranges;
        let segments = if reset {
            Vec::new()
        } else {
            task.segments.clone()
        };
        let downloaded = if reset {
            0
        } else {
            task.downloaded_bytes.max(0) as u64
        };
        self.inner
            .storage
            .save_progress(
                id,
                downloaded,
                task.total_bytes.map(|v| v as u64),
                0.0,
                &segments,
            )
            .await?;
        self.inner
            .storage
            .set_status(id, DownloadStatus::Queued, None, None, None)
            .await?;
        self.inner
            .storage
            .record_event(
                id,
                Some(task.status),
                DownloadStatus::Queued,
                Some("queued by user"),
            )
            .await?;
        self.emit_task(id).await;
        self.schedule().await?;
        self.inner.storage.task(id).await.map_err(Into::into)
    }

    pub async fn retry_task(&self, id: &str, restart: bool) -> Result<DownloadTask, ServiceError> {
        self.resume_task(id, restart).await
    }

    pub async fn cancel_task(&self, id: &str) -> Result<DownloadTask, ServiceError> {
        let task = self.inner.storage.task(id).await?;
        let control = self
            .inner
            .running
            .lock()
            .await
            .get(id)
            .map(|c| c.cancel.clone());
        if let Some(cancel) = control {
            cancel.cancel();
        } else {
            self.inner
                .storage
                .set_status(id, DownloadStatus::Cancelled, None, None, Some(Utc::now()))
                .await?;
            self.inner
                .storage
                .record_event(
                    id,
                    Some(task.status),
                    DownloadStatus::Cancelled,
                    Some("cancelled by user"),
                )
                .await?;
        }
        self.emit_task(id).await;
        self.inner.storage.task(id).await.map_err(Into::into)
    }

    pub async fn remove_task(&self, id: &str, delete_file: bool) -> Result<(), ServiceError> {
        if let Some(control) = self.inner.running.lock().await.remove(id) {
            control.cancel.cancel();
            control.pause.cancel();
            control.abort.abort();
        }
        let task = self.inner.storage.task(id).await?;
        if delete_file {
            let safe = safe_destination(Path::new(&task.destination_dir), &task.file_name)?;
            if safe.exists() {
                tokio::fs::remove_file(&safe)
                    .await
                    .map_err(|e| ServiceError::Io(format!("cannot delete file: {e}")))?;
            }
        }
        self.inner.storage.remove_task(id).await?;
        let _ = self
            .inner
            .events
            .send(ServiceEvent::TaskRemoved { id: id.into() });
        Ok(())
    }

    pub async fn clear_history(&self, delete_files: bool) -> Result<usize, ServiceError> {
        let tasks = self
            .inner
            .storage
            .tasks(&TaskFilter {
                statuses: vec![
                    DownloadStatus::Completed,
                    DownloadStatus::Failed,
                    DownloadStatus::Cancelled,
                ],
                limit: 5000,
                ..Default::default()
            })
            .await?;
        let mut removed = 0;
        for task in tasks {
            if delete_files {
                let safe = safe_destination(Path::new(&task.destination_dir), &task.file_name)?;
                let _ = tokio::fs::remove_file(safe).await;
            }
            if self.inner.storage.remove_task(&task.id).await? {
                removed += 1;
            }
        }
        Ok(removed)
    }

    pub async fn organize_task(
        &self,
        id: &str,
        category: Option<&str>,
        queue_id: Option<&str>,
        collection_id: Option<&str>,
        priority: Option<i32>,
        connections: Option<u32>,
    ) -> Result<DownloadTask, ServiceError> {
        self.inner
            .storage
            .set_organization(id, category, queue_id, collection_id, priority, connections)
            .await?;
        self.emit_task(id).await;
        self.inner.storage.task(id).await.map_err(Into::into)
    }

    pub async fn pause_queue(&self, id: &str) -> Result<(), ServiceError> {
        let mut queue = self
            .inner
            .storage
            .queues()
            .await?
            .into_iter()
            .find(|queue| queue.id == id)
            .ok_or_else(|| ServiceError::NotFound(format!("queue {id}")))?;
        queue.status = "paused".into();
        self.inner.storage.update_queue(&queue).await?;
        let tasks = self
            .inner
            .storage
            .tasks(&TaskFilter {
                queue_id: Some(id.into()),
                limit: 5000,
                ..Default::default()
            })
            .await?;
        for task in tasks {
            if task.status == DownloadStatus::Queued {
                let _ = self
                    .inner
                    .storage
                    .set_status(&task.id, DownloadStatus::Paused, None, None, None)
                    .await;
            } else if self.inner.running.lock().await.contains_key(&task.id) {
                let _ = self.pause_task(&task.id).await;
            }
        }
        Ok(())
    }

    pub async fn start_queue(&self, id: &str) -> Result<(), ServiceError> {
        let mut queue = self
            .inner
            .storage
            .queues()
            .await?
            .into_iter()
            .find(|queue| queue.id == id)
            .ok_or_else(|| ServiceError::NotFound(format!("queue {id}")))?;
        queue.status = "active".into();
        self.inner.storage.update_queue(&queue).await?;
        self.schedule().await
    }

    pub async fn create_queue(
        &self,
        name: &str,
        max_concurrent: i32,
    ) -> Result<Queue, ServiceError> {
        Ok(self
            .inner
            .storage
            .create_queue(name, max_concurrent)
            .await?)
    }

    pub async fn update_queue(&self, queue: &Queue) -> Result<(), ServiceError> {
        Ok(self.inner.storage.update_queue(queue).await?)
    }

    pub async fn delete_queue(&self, id: &str) -> Result<(), ServiceError> {
        if !self.inner.storage.delete_queue(id).await? {
            return Err(ServiceError::NotFound(format!("queue {id}")));
        }
        Ok(())
    }

    pub async fn stats(&self) -> Result<Stats, ServiceError> {
        Ok(self.inner.storage.stats().await?)
    }

    pub async fn analytics(&self) -> Result<download_storage::Analytics, ServiceError> {
        Ok(self.inner.storage.analytics().await?)
    }

    pub async fn events_for(&self, id: &str) -> Result<Vec<TaskEvent>, ServiceError> {
        Ok(self.inner.storage.events(id, 100).await?)
    }

    pub async fn integrity_for(&self, id: &str) -> Result<Vec<IntegrityRecord>, ServiceError> {
        Ok(self.inner.storage.integrity_checks(id).await?)
    }

    pub async fn speed_samples(&self, hours: i64) -> Result<Vec<SpeedSample>, ServiceError> {
        Ok(self.inner.storage.speed_samples(hours, 500).await?)
    }

    pub async fn verify_task(&self, id: &str) -> Result<IntegrityReport, ServiceError> {
        let task = self.inner.storage.task(id).await?;
        if !Path::new(&task.file_path).exists() {
            return Err(ServiceError::NotFound("downloaded file".into()));
        }
        let report = verify_file(
            &task.file_path,
            task.total_bytes.map(|v| v as u64),
            task.expected_checksum.as_deref(),
        )
        .await
        .map_err(|e| ServiceError::Io(e.to_string()))?;
        self.inner
            .storage
            .record_integrity(
                id,
                report.algorithm,
                report.expected_sha256.as_deref(),
                Some(&report.actual_sha256),
                report.status.as_str(),
                report.expected_size,
                Some(report.actual_size),
            )
            .await?;
        self.inner
            .storage
            .set_checksum(id, &report.actual_sha256)
            .await?;
        Ok(report)
    }

    pub async fn diagnostics(&self, id: &str) -> Result<Diagnostics, ServiceError> {
        let task = self.inner.storage.task(id).await?;
        let events = self.inner.storage.events(id, 100).await?;
        let integrity = self.inner.storage.integrity_checks(id).await?;
        let runtime = self.inner.runtime.read().await.clone();
        let options = self.engine_options(task.connections, &runtime);
        let probe = match parse_and_validate_url(&task.url) {
            Ok(url) => self.inner.engine.inspect(&url, &options).await,
            Err(error) => Err(EngineError::InvalidRequest(error.to_string())),
        };
        let (source_reachable, resource, probe_error) = match probe {
            Ok(info) => (Some(true), Some(info), None),
            Err(error) => (Some(false), None, Some(error)),
        };
        let error_kind = task
            .error_kind
            .clone()
            .or_else(|| probe_error.as_ref().map(|error| error.kind().to_string()));
        let hints = probe_error
            .as_ref()
            .map(EngineError::hints)
            .unwrap_or_else(|| hints_for_kind(error_kind.as_deref()));
        let summary = match task.status {
            DownloadStatus::Failed => {
                "This download failed. The checks below can help identify why.".to_string()
            }
            DownloadStatus::Paused => "This download is paused and can be resumed.".to_string(),
            DownloadStatus::Completed => "This download completed successfully.".to_string(),
            _ => format!("Current task state: {}.", task.status),
        };
        let suggested_actions = if hints.is_empty() {
            vec!["Retry the download from the task menu.".to_string()]
        } else {
            hints.clone()
        };
        Ok(Diagnostics {
            task_id: id.to_string(),
            summary,
            error_kind,
            error: task
                .error
                .clone()
                .or_else(|| probe_error.map(|error| error.to_string())),
            hints,
            source_reachable,
            resource,
            events,
            integrity,
            suggested_actions,
        })
    }

    pub async fn benchmark(&self, url: &str) -> Result<BenchmarkResult, ServiceError> {
        let url = parse_and_validate_url(url)?;
        let runtime = self.inner.runtime.read().await.clone();
        let client = reqwest::Client::builder()
            .user_agent(concat!("NexDownload/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(runtime.timeout_secs.clamp(3, 60)))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|e| ServiceError::Engine(EngineError::Internal(e.to_string())))?;

        let started = Instant::now();
        let mut response = client
            .get(url.clone())
            .header(reqwest::header::RANGE, "bytes=0-2097151")
            .send()
            .await
            .map_err(|e| ServiceError::Engine(EngineError::from_reqwest(e)))?;
        let headers_at = started.elapsed();
        let status = response.status();
        let read_started = Instant::now();
        let mut bytes = 0u64;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| ServiceError::Engine(EngineError::from_reqwest(e)))?
        {
            bytes += chunk.len() as u64;
            if bytes >= 2 * 1024 * 1024 {
                break;
            }
        }
        let download_elapsed = read_started.elapsed();
        let throughput = if download_elapsed.as_secs_f64() > 0.0 {
            bytes as f64 / download_elapsed.as_secs_f64()
        } else {
            0.0
        };
        Ok(BenchmarkResult {
            url: url.to_string(),
            status: status.as_u16(),
            connect_and_headers_ms: headers_at.as_secs_f64() * 1000.0,
            download_ms: download_elapsed.as_secs_f64() * 1000.0,
            bytes,
            throughput_bps: throughput,
            dns_ms: None,
            tls_ms: None,
            note: "Timings are measured end-to-end with the system resolver and TLS stack. DNS and TLS sub-timings require OS-level tracing and are reported as unavailable rather than estimated.".into(),
        })
    }

    pub async fn duplicate_groups(&self) -> Result<Vec<DuplicateGroup>, ServiceError> {
        Ok(self
            .inner
            .storage
            .find_duplicate_groups()
            .await?
            .into_iter()
            .map(|tasks| DuplicateGroup {
                checksum: tasks
                    .first()
                    .and_then(|task| task.checksum_sha256.clone())
                    .unwrap_or_default(),
                tasks,
            })
            .collect())
    }

    pub async fn settings(&self) -> Result<HashMap<String, Value>, ServiceError> {
        Ok(self.inner.storage.settings().await?)
    }

    pub async fn update_settings(
        &self,
        patch: &serde_json::Map<String, Value>,
    ) -> Result<HashMap<String, Value>, ServiceError> {
        for (key, value) in patch {
            if let Some(number) = value.as_u64() {
                match key.as_str() {
                    "max_concurrent_downloads" if !(1..=16).contains(&number) => {
                        return Err(ServiceError::Validation(
                            "max_concurrent_downloads must be between 1 and 16".into(),
                        ))
                    }
                    "default_connections" if !(1..=32).contains(&number) => {
                        return Err(ServiceError::Validation(
                            "default_connections must be between 1 and 32".into(),
                        ))
                    }
                    "timeout_secs" if !(5..=600).contains(&number) => {
                        return Err(ServiceError::Validation(
                            "timeout_secs must be between 5 and 600".into(),
                        ))
                    }
                    "retry_limit" if number > 20 => {
                        return Err(ServiceError::Validation(
                            "retry_limit must be 20 or less".into(),
                        ))
                    }
                    _ => {}
                }
            }
            self.inner.storage.set_setting(key, value).await?;
        }
        self.refresh_config().await?;
        Ok(self.inner.storage.settings().await?)
    }

    pub async fn export_history_csv(&self) -> Result<String, ServiceError> {
        let tasks = self
            .inner
            .storage
            .tasks(&TaskFilter {
                statuses: vec![
                    DownloadStatus::Completed,
                    DownloadStatus::Failed,
                    DownloadStatus::Cancelled,
                ],
                limit: 5000,
                ..Default::default()
            })
            .await?;
        let mut csv = String::from(
            "id,file_name,url,file_path,size_bytes,status,started_at,completed_at,sha256,category\n",
        );
        for task in tasks {
            let fields = [
                task.id,
                task.file_name,
                task.url,
                task.file_path,
                task.total_bytes.unwrap_or(0).to_string(),
                task.status.to_string(),
                task.started_at.map(|v| v.to_rfc3339()).unwrap_or_default(),
                task.completed_at
                    .map(|v| v.to_rfc3339())
                    .unwrap_or_default(),
                task.checksum_sha256.unwrap_or_default(),
                task.category,
            ];
            csv.push_str(
                &fields
                    .iter()
                    .map(|field| csv_escape(field))
                    .collect::<Vec<_>>()
                    .join(","),
            );
            csv.push('\n');
        }
        Ok(csv)
    }

    pub async fn shutdown(&self) {
        let controls: Vec<(CancellationToken, CancellationToken, AbortHandle)> = {
            let running = self.inner.running.lock().await;
            running
                .values()
                .map(|c| (c.pause.clone(), c.cancel.clone(), c.abort.clone()))
                .collect()
        };
        for (_, cancel, _) in &controls {
            cancel.cancel();
        }
        for _ in 0..60 {
            if self.inner.running.lock().await.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        for (_, _, abort) in controls {
            abort.abort();
        }
        let _ = self.inner.engine.shutdown().await;
    }

    async fn emit_task(&self, id: &str) {
        if let Ok(task) = self.inner.storage.task(id).await {
            let _ = self.inner.events.send(ServiceEvent::TaskUpdated {
                task: Box::new(task),
            });
        }
    }

    fn notify(&self, level: &str, title: &str, message: &str) {
        let _ = self.inner.events.send(ServiceEvent::Notification {
            level: level.into(),
            title: title.into(),
            message: message.into(),
        });
    }
}

fn rule_from_stored(stored: &StoredRule) -> Result<Rule, serde_json::Error> {
    Ok(Rule {
        id: stored.id.clone(),
        name: stored.name.clone(),
        enabled: stored.enabled,
        priority: stored.priority,
        conditions: serde_json::from_value(stored.conditions.clone())?,
        actions: serde_json::from_value(stored.actions.clone())?,
    })
}

fn expand_home(input: &str) -> PathBuf {
    let trimmed = input.trim();
    if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
        if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(trimmed)
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn hints_for_kind(kind: Option<&str>) -> Vec<String> {
    let hints: &[&str] = match kind {
        Some("dns") => &[
            "Check your connection and DNS settings.",
            "Try opening the URL in a browser.",
        ],
        Some("tls") => &["Verify the system clock and the server certificate."],
        Some("permission") => &["Choose a writable destination folder."],
        Some("insufficient_storage") => &["Free some disk space and retry."],
        Some("resource_changed") => &["Retry the download to get a consistent copy."],
        Some("timeout") => &["Retry, or raise the timeout in Settings."],
        Some("http") => &["The link may have expired; try it in a browser."],
        Some("network") => &["Check the connection and retry."],
        _ => &[],
    };
    hints.iter().map(|value| (*value).to_string()).collect()
}

impl From<download_core::CoreError> for ServiceError {
    fn from(value: download_core::CoreError) -> Self {
        ServiceError::Validation(value.to_string())
    }
}

impl From<download_rules::RuleError> for ServiceError {
    fn from(value: download_rules::RuleError) -> Self {
        ServiceError::Rule(value.to_string())
    }
}

impl From<download_integrity::IntegrityError> for ServiceError {
    fn from(value: download_integrity::IntegrityError) -> Self {
        ServiceError::Io(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escaping_handles_quotes() {
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
        assert_eq!(csv_escape("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_escape("plain"), "plain");
    }

    #[test]
    fn home_expansion_is_safe() {
        assert_eq!(expand_home("relative/path"), PathBuf::from("relative/path"));
    }
}
