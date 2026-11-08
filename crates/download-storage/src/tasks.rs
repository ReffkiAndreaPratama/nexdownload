use chrono::{DateTime, Utc};
use download_core::{DownloadSegment, DownloadStatus, DownloadTask};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteRow;
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::{dt, now_rfc3339, opt_dt, opt_time, Storage, StorageError};

#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub statuses: Vec<DownloadStatus>,
    pub category: Option<String>,
    pub queue_id: Option<String>,
    pub collection_id: Option<String>,
    pub search: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

impl TaskFilter {
    pub fn with_limit(mut self, limit: i64) -> Self {
        self.limit = limit;
        self
    }

    pub fn statuses(mut self, statuses: Vec<DownloadStatus>) -> Self {
        self.statuses = statuses;
        self
    }

    pub fn search(mut self, value: impl Into<String>) -> Self {
        let value = value.into();
        if !value.trim().is_empty() {
            self.search = Some(value.trim().to_string());
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub id: i64,
    pub task_id: String,
    pub from_state: Option<String>,
    pub to_state: String,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityRecord {
    pub id: i64,
    pub task_id: String,
    pub algorithm: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub status: String,
    pub expected_size: Option<i64>,
    pub actual_size: Option<i64>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedSample {
    pub sampled_at: DateTime<Utc>,
    pub speed_bps: f64,
    pub downloaded_bytes: i64,
}

struct TaskRow {
    id: String,
    url: String,
    final_url: Option<String>,
    file_name: String,
    destination_dir: String,
    file_path: String,
    category: String,
    queue_id: Option<String>,
    collection_id: Option<String>,
    status: String,
    priority: i32,
    total_bytes: Option<i64>,
    downloaded_bytes: i64,
    connections: i64,
    speed_bps: f64,
    error: Option<String>,
    error_kind: Option<String>,
    etag: Option<String>,
    last_modified: Option<String>,
    accepts_ranges: bool,
    content_type: Option<String>,
    checksum_sha256: Option<String>,
    expected_checksum: Option<String>,
    source_domain: Option<String>,
    segments_json: String,
    created_at: String,
    updated_at: String,
    started_at: Option<String>,
    completed_at: Option<String>,
}

impl TaskRow {
    fn from_sql(row: &SqliteRow) -> Result<Self, StorageError> {
        Ok(Self {
            id: row.try_get("id")?,
            url: row.try_get("url")?,
            final_url: row.try_get("final_url")?,
            file_name: row.try_get("file_name")?,
            destination_dir: row.try_get("destination_dir")?,
            file_path: row.try_get("file_path")?,
            category: row.try_get("category")?,
            queue_id: row.try_get("queue_id")?,
            collection_id: row.try_get("collection_id")?,
            status: row.try_get("status")?,
            priority: row.try_get("priority")?,
            total_bytes: row.try_get("total_bytes")?,
            downloaded_bytes: row.try_get("downloaded_bytes")?,
            connections: row.try_get("connections")?,
            speed_bps: row.try_get("speed_bps")?,
            error: row.try_get("error")?,
            error_kind: row.try_get("error_kind")?,
            etag: row.try_get("etag")?,
            last_modified: row.try_get("last_modified")?,
            accepts_ranges: row.try_get::<i64, _>("accepts_ranges")? != 0,
            content_type: row.try_get("content_type")?,
            checksum_sha256: row.try_get("checksum_sha256")?,
            expected_checksum: row.try_get("expected_checksum")?,
            source_domain: row.try_get("source_domain")?,
            segments_json: row.try_get("segments_json")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            started_at: row.try_get("started_at")?,
            completed_at: row.try_get("completed_at")?,
        })
    }

    fn into_task(self) -> Result<DownloadTask, StorageError> {
        let status = DownloadStatus::parse(&self.status).ok_or_else(|| {
            StorageError::Invalid(format!("unknown task status '{}'", self.status))
        })?;
        let segments: Vec<DownloadSegment> = serde_json::from_str(&self.segments_json)
            .map_err(|e| StorageError::Invalid(format!("invalid segment data: {e}")))?;
        Ok(DownloadTask {
            id: self.id,
            url: self.url,
            final_url: self.final_url,
            file_name: self.file_name,
            destination_dir: self.destination_dir,
            file_path: self.file_path,
            category: self.category,
            queue_id: self.queue_id,
            collection_id: self.collection_id,
            status,
            priority: self.priority,
            total_bytes: self.total_bytes,
            downloaded_bytes: self.downloaded_bytes,
            connections: self.connections.max(1) as u32,
            speed_bps: self.speed_bps,
            error: self.error,
            error_kind: self.error_kind,
            etag: self.etag,
            last_modified: self.last_modified,
            accepts_ranges: self.accepts_ranges,
            content_type: self.content_type,
            checksum_sha256: self.checksum_sha256,
            expected_checksum: self.expected_checksum,
            source_domain: self.source_domain,
            created_at: dt(&self.created_at),
            updated_at: dt(&self.updated_at),
            started_at: opt_dt(&self.started_at),
            completed_at: opt_dt(&self.completed_at),
            segments,
        })
    }
}

const TASK_COLUMNS: &str = "id, url, final_url, file_name, destination_dir, file_path, category, queue_id, collection_id, status, priority, total_bytes, downloaded_bytes, connections, speed_bps, error, error_kind, etag, last_modified, accepts_ranges, content_type, checksum_sha256, expected_checksum, source_domain, segments_json, created_at, updated_at, started_at, completed_at";

pub(crate) async fn insert_task(
    storage: &Storage,
    task: &DownloadTask,
) -> Result<(), StorageError> {
    let segments = serde_json::to_string(&task.segments)?;
    sqlx::query(
        "INSERT INTO tasks (
            id, url, final_url, file_name, destination_dir, file_path, category, queue_id,
            collection_id, status, priority, total_bytes, downloaded_bytes, connections,
            speed_bps, error, error_kind, etag, last_modified, accepts_ranges, content_type,
            checksum_sha256, expected_checksum, source_domain, segments_json, created_at,
            updated_at, started_at, completed_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&task.id)
    .bind(&task.url)
    .bind(&task.final_url)
    .bind(&task.file_name)
    .bind(&task.destination_dir)
    .bind(&task.file_path)
    .bind(&task.category)
    .bind(&task.queue_id)
    .bind(&task.collection_id)
    .bind(task.status.as_str())
    .bind(task.priority)
    .bind(task.total_bytes)
    .bind(task.downloaded_bytes)
    .bind(task.connections as i64)
    .bind(task.speed_bps)
    .bind(&task.error)
    .bind(&task.error_kind)
    .bind(&task.etag)
    .bind(&task.last_modified)
    .bind(if task.accepts_ranges { 1i64 } else { 0i64 })
    .bind(&task.content_type)
    .bind(&task.checksum_sha256)
    .bind(&task.expected_checksum)
    .bind(&task.source_domain)
    .bind(segments)
    .bind(task.created_at.to_rfc3339())
    .bind(task.updated_at.to_rfc3339())
    .bind(opt_time(&task.started_at))
    .bind(opt_time(&task.completed_at))
    .execute(storage.pool())
    .await?;
    Ok(())
}

pub(crate) async fn update_task(
    storage: &Storage,
    task: &DownloadTask,
) -> Result<(), StorageError> {
    let segments = serde_json::to_string(&task.segments)?;
    let result = sqlx::query(
        "UPDATE tasks SET
            url = ?, final_url = ?, file_name = ?, destination_dir = ?, file_path = ?,
            category = ?, queue_id = ?, collection_id = ?, status = ?, priority = ?,
            total_bytes = ?, downloaded_bytes = ?, connections = ?, speed_bps = ?, error = ?,
            error_kind = ?, etag = ?, last_modified = ?, accepts_ranges = ?, content_type = ?,
            checksum_sha256 = ?, expected_checksum = ?, source_domain = ?, segments_json = ?,
            updated_at = ?, started_at = ?, completed_at = ?
         WHERE id = ?",
    )
    .bind(&task.url)
    .bind(&task.final_url)
    .bind(&task.file_name)
    .bind(&task.destination_dir)
    .bind(&task.file_path)
    .bind(&task.category)
    .bind(&task.queue_id)
    .bind(&task.collection_id)
    .bind(task.status.as_str())
    .bind(task.priority)
    .bind(task.total_bytes)
    .bind(task.downloaded_bytes)
    .bind(task.connections as i64)
    .bind(task.speed_bps)
    .bind(&task.error)
    .bind(&task.error_kind)
    .bind(&task.etag)
    .bind(&task.last_modified)
    .bind(if task.accepts_ranges { 1i64 } else { 0i64 })
    .bind(&task.content_type)
    .bind(&task.checksum_sha256)
    .bind(&task.expected_checksum)
    .bind(&task.source_domain)
    .bind(segments)
    .bind(task.updated_at.to_rfc3339())
    .bind(opt_time(&task.started_at))
    .bind(opt_time(&task.completed_at))
    .bind(&task.id)
    .execute(storage.pool())
    .await?;
    if result.rows_affected() == 0 {
        return Err(StorageError::NotFound(format!("task {}", task.id)));
    }
    Ok(())
}

pub(crate) async fn get_task(storage: &Storage, id: &str) -> Result<DownloadTask, StorageError> {
    let row = sqlx::query(&format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?"))
        .bind(id)
        .fetch_optional(storage.pool())
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("task {id}")))?;
    TaskRow::from_sql(&row)?.into_task()
}

pub(crate) async fn find_task(
    storage: &Storage,
    id: &str,
) -> Result<Option<DownloadTask>, StorageError> {
    let row = sqlx::query(&format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?"))
        .bind(id)
        .fetch_optional(storage.pool())
        .await?;
    match row {
        Some(row) => Ok(Some(TaskRow::from_sql(&row)?.into_task()?)),
        None => Ok(None),
    }
}

fn apply_filters(qb: &mut QueryBuilder<'_, Sqlite>, filter: &TaskFilter) {
    if !filter.statuses.is_empty() {
        qb.push(" AND status IN (");
        let mut separated = qb.separated(", ");
        for status in &filter.statuses {
            separated.push_bind(status.as_str().to_owned());
        }
        separated.push_unseparated(")");
    }
    if let Some(category) = &filter.category {
        qb.push(" AND category = ").push_bind(category.clone());
    }
    if let Some(queue) = &filter.queue_id {
        qb.push(" AND queue_id = ").push_bind(queue.clone());
    }
    if let Some(collection) = &filter.collection_id {
        qb.push(" AND collection_id = ")
            .push_bind(collection.clone());
    }
    if let Some(search) = &filter.search {
        let pattern = format!("%{search}%");
        qb.push(" AND (file_name LIKE ")
            .push_bind(pattern.clone())
            .push(" OR url LIKE ")
            .push_bind(pattern)
            .push(")");
    }
}

pub(crate) async fn list_tasks(
    storage: &Storage,
    filter: &TaskFilter,
) -> Result<Vec<DownloadTask>, StorageError> {
    let limit = if filter.limit <= 0 {
        500
    } else {
        filter.limit.min(2000)
    };
    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new(format!("SELECT {TASK_COLUMNS} FROM tasks WHERE 1 = 1"));
    apply_filters(&mut qb, filter);
    qb.push(
        " ORDER BY CASE status
            WHEN 'downloading' THEN 0 WHEN 'connecting' THEN 1 WHEN 'preparing' THEN 2
            WHEN 'pausing' THEN 3 WHEN 'verifying' THEN 4 WHEN 'queued' THEN 5
            WHEN 'waiting_for_network' THEN 6 WHEN 'paused' THEN 7 WHEN 'failed' THEN 8
            ELSE 9 END,
          priority DESC, created_at DESC
          LIMIT ",
    )
    .push_bind(limit)
    .push(" OFFSET ")
    .push_bind(filter.offset.max(0));

    let rows = qb.build().fetch_all(storage.pool()).await?;
    rows.iter()
        .map(|row| TaskRow::from_sql(row)?.into_task())
        .collect()
}

pub(crate) async fn count_tasks(
    storage: &Storage,
    filter: &TaskFilter,
) -> Result<i64, StorageError> {
    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT COUNT(*) AS count FROM tasks WHERE 1 = 1");
    apply_filters(&mut qb, filter);
    let row = qb.build().fetch_one(storage.pool()).await?;
    Ok(row.try_get::<i64, _>("count")?)
}

pub(crate) async fn delete_task(storage: &Storage, id: &str) -> Result<bool, StorageError> {
    let result = sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(id)
        .execute(storage.pool())
        .await?;
    Ok(result.rows_affected() > 0)
}

pub(crate) async fn set_status(
    storage: &Storage,
    id: &str,
    status: DownloadStatus,
    error: Option<&str>,
    error_kind: Option<&str>,
    completed_at: Option<DateTime<Utc>>,
) -> Result<(), StorageError> {
    let result = sqlx::query(
        "UPDATE tasks SET status = ?, error = ?, error_kind = ?, updated_at = ?, completed_at = ? WHERE id = ?",
    )
    .bind(status.as_str())
    .bind(error)
    .bind(error_kind)
    .bind(now_rfc3339())
    .bind(opt_time(&completed_at))
    .bind(id)
    .execute(storage.pool())
    .await?;
    if result.rows_affected() == 0 {
        return Err(StorageError::NotFound(format!("task {id}")));
    }
    Ok(())
}

pub(crate) async fn set_started(
    storage: &Storage,
    id: &str,
    at: DateTime<Utc>,
) -> Result<(), StorageError> {
    sqlx::query(
        "UPDATE tasks SET started_at = COALESCE(started_at, ?), updated_at = ? WHERE id = ?",
    )
    .bind(at.to_rfc3339())
    .bind(now_rfc3339())
    .bind(id)
    .execute(storage.pool())
    .await?;
    Ok(())
}

pub(crate) async fn update_progress(
    storage: &Storage,
    id: &str,
    downloaded: u64,
    total: Option<u64>,
    speed_bps: f64,
    segments: &[DownloadSegment],
) -> Result<(), StorageError> {
    let segments_json = serde_json::to_string(segments)?;
    sqlx::query(
        "UPDATE tasks SET downloaded_bytes = ?, total_bytes = COALESCE(?, total_bytes),
         speed_bps = ?, segments_json = ?, updated_at = ? WHERE id = ?",
    )
    .bind(downloaded as i64)
    .bind(total.map(|v| v as i64))
    .bind(speed_bps)
    .bind(segments_json)
    .bind(now_rfc3339())
    .bind(id)
    .execute(storage.pool())
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn update_metadata(
    storage: &Storage,
    id: &str,
    final_url: Option<&str>,
    total: Option<u64>,
    etag: Option<&str>,
    last_modified: Option<&str>,
    accepts_ranges: bool,
    content_type: Option<&str>,
    source_domain: Option<&str>,
) -> Result<(), StorageError> {
    sqlx::query(
        "UPDATE tasks SET final_url = COALESCE(?, final_url), total_bytes = COALESCE(?, total_bytes),
         etag = COALESCE(?, etag), last_modified = COALESCE(?, last_modified),
         accepts_ranges = ?, content_type = COALESCE(?, content_type),
         source_domain = COALESCE(?, source_domain), updated_at = ? WHERE id = ?",
    )
    .bind(final_url)
    .bind(total.map(|v| v as i64))
    .bind(etag)
    .bind(last_modified)
    .bind(if accepts_ranges { 1i64 } else { 0i64 })
    .bind(content_type)
    .bind(source_domain)
    .bind(now_rfc3339())
    .bind(id)
    .execute(storage.pool())
    .await?;
    Ok(())
}

pub(crate) async fn set_checksum(
    storage: &Storage,
    id: &str,
    checksum: &str,
) -> Result<(), StorageError> {
    sqlx::query("UPDATE tasks SET checksum_sha256 = ?, updated_at = ? WHERE id = ?")
        .bind(checksum)
        .bind(now_rfc3339())
        .bind(id)
        .execute(storage.pool())
        .await?;
    Ok(())
}

pub(crate) async fn set_organization(
    storage: &Storage,
    id: &str,
    category: Option<&str>,
    queue_id: Option<&str>,
    collection_id: Option<&str>,
    priority: Option<i32>,
    connections: Option<u32>,
) -> Result<(), StorageError> {
    sqlx::query(
        "UPDATE tasks SET category = COALESCE(?, category), queue_id = COALESCE(?, queue_id),
         collection_id = COALESCE(?, collection_id), priority = COALESCE(?, priority),
         connections = COALESCE(?, connections), updated_at = ? WHERE id = ?",
    )
    .bind(category)
    .bind(queue_id)
    .bind(collection_id)
    .bind(priority)
    .bind(connections.map(|v| v as i64))
    .bind(now_rfc3339())
    .bind(id)
    .execute(storage.pool())
    .await?;
    Ok(())
}

pub(crate) async fn update_segments_only(
    storage: &Storage,
    id: &str,
    segments: &[DownloadSegment],
) -> Result<(), StorageError> {
    let json = serde_json::to_string(segments)?;
    sqlx::query("UPDATE tasks SET segments_json = ?, updated_at = ? WHERE id = ?")
        .bind(json)
        .bind(now_rfc3339())
        .bind(id)
        .execute(storage.pool())
        .await?;
    Ok(())
}

pub(crate) async fn record_event(
    storage: &Storage,
    task_id: &str,
    from: Option<DownloadStatus>,
    to: DownloadStatus,
    note: Option<&str>,
) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT INTO task_events (task_id, from_state, to_state, note, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(task_id)
    .bind(from.map(|s| s.as_str()))
    .bind(to.as_str())
    .bind(note)
    .bind(now_rfc3339())
    .execute(storage.pool())
    .await?;
    Ok(())
}

pub(crate) async fn list_events(
    storage: &Storage,
    task_id: &str,
    limit: i64,
) -> Result<Vec<TaskEvent>, StorageError> {
    let rows = sqlx::query(
        "SELECT id, task_id, from_state, to_state, note, created_at
         FROM task_events WHERE task_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(task_id)
    .bind(limit.clamp(1, 500))
    .fetch_all(storage.pool())
    .await?;
    rows.iter()
        .map(|row| {
            Ok(TaskEvent {
                id: row.try_get("id")?,
                task_id: row.try_get("task_id")?,
                from_state: row.try_get("from_state")?,
                to_state: row.try_get("to_state")?,
                note: row.try_get("note")?,
                created_at: dt(&row.try_get::<String, _>("created_at")?),
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn record_integrity(
    storage: &Storage,
    task_id: &str,
    algorithm: &str,
    expected: Option<&str>,
    actual: Option<&str>,
    status: &str,
    expected_size: Option<u64>,
    actual_size: Option<u64>,
) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT INTO integrity_checks
         (task_id, algorithm, expected, actual, status, expected_size, actual_size, checked_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(task_id)
    .bind(algorithm)
    .bind(expected)
    .bind(actual)
    .bind(status)
    .bind(expected_size.map(|v| v as i64))
    .bind(actual_size.map(|v| v as i64))
    .bind(now_rfc3339())
    .execute(storage.pool())
    .await?;
    Ok(())
}

pub(crate) async fn list_integrity(
    storage: &Storage,
    task_id: &str,
) -> Result<Vec<IntegrityRecord>, StorageError> {
    let rows = sqlx::query(
        "SELECT id, task_id, algorithm, expected, actual, status, expected_size, actual_size, checked_at
         FROM integrity_checks WHERE task_id = ? ORDER BY checked_at DESC LIMIT 100",
    )
    .bind(task_id)
    .fetch_all(storage.pool())
    .await?;
    rows.iter()
        .map(|row| {
            Ok(IntegrityRecord {
                id: row.try_get("id")?,
                task_id: row.try_get("task_id")?,
                algorithm: row.try_get("algorithm")?,
                expected: row.try_get("expected")?,
                actual: row.try_get("actual")?,
                status: row.try_get("status")?,
                expected_size: row.try_get("expected_size")?,
                actual_size: row.try_get("actual_size")?,
                checked_at: dt(&row.try_get::<String, _>("checked_at")?),
            })
        })
        .collect()
}

pub(crate) async fn record_speed_sample(
    storage: &Storage,
    task_id: &str,
    speed_bps: f64,
    downloaded_bytes: u64,
) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT INTO speed_samples (task_id, sampled_at, speed_bps, downloaded_bytes) VALUES (?, ?, ?, ?)",
    )
    .bind(task_id)
    .bind(now_rfc3339())
    .bind(speed_bps)
    .bind(downloaded_bytes as i64)
    .execute(storage.pool())
    .await?;
    Ok(())
}

pub(crate) async fn recent_speed_samples(
    storage: &Storage,
    hours: i64,
    limit: i64,
) -> Result<Vec<SpeedSample>, StorageError> {
    let cutoff = (Utc::now() - chrono::Duration::hours(hours.clamp(1, 24 * 30))).to_rfc3339();
    let rows = sqlx::query(
        "SELECT sampled_at, speed_bps, downloaded_bytes FROM speed_samples
         WHERE sampled_at >= ? ORDER BY sampled_at ASC LIMIT ?",
    )
    .bind(cutoff)
    .bind(limit.clamp(1, 5000))
    .fetch_all(storage.pool())
    .await?;
    rows.iter()
        .map(|row| {
            Ok(SpeedSample {
                sampled_at: dt(&row.try_get::<String, _>("sampled_at")?),
                speed_bps: row.try_get("speed_bps")?,
                downloaded_bytes: row.try_get("downloaded_bytes")?,
            })
        })
        .collect()
}

pub(crate) async fn prune_speed_samples(storage: &Storage, days: i64) -> Result<u64, StorageError> {
    let cutoff = (Utc::now() - chrono::Duration::days(days.clamp(1, 365))).to_rfc3339();
    let result = sqlx::query("DELETE FROM speed_samples WHERE sampled_at < ?")
        .bind(cutoff)
        .execute(storage.pool())
        .await?;
    Ok(result.rows_affected())
}

pub(crate) async fn tasks_with_duplicate_checksum(
    storage: &Storage,
) -> Result<Vec<DownloadTask>, StorageError> {
    let rows = sqlx::query(&format!(
        "SELECT {TASK_COLUMNS} FROM tasks
         WHERE checksum_sha256 IS NOT NULL AND checksum_sha256 IN (
            SELECT checksum_sha256 FROM tasks
            WHERE checksum_sha256 IS NOT NULL
            GROUP BY checksum_sha256 HAVING COUNT(*) > 1
         )
         ORDER BY checksum_sha256, created_at DESC"
    ))
    .fetch_all(storage.pool())
    .await?;
    rows.iter()
        .map(|row| TaskRow::from_sql(row)?.into_task())
        .collect()
}

pub(crate) async fn find_by_checksum(
    storage: &Storage,
    checksum: &str,
) -> Result<Option<DownloadTask>, StorageError> {
    let row = sqlx::query(&format!(
        "SELECT {TASK_COLUMNS} FROM tasks WHERE checksum_sha256 = ? LIMIT 1"
    ))
    .bind(checksum)
    .fetch_optional(storage.pool())
    .await?;
    match row {
        Some(row) => Ok(Some(TaskRow::from_sql(&row)?.into_task()?)),
        None => Ok(None),
    }
}

impl Storage {
    pub async fn update_task(&self, task: &DownloadTask) -> Result<(), StorageError> {
        update_task(self, task).await
    }

    pub async fn find_task(&self, id: &str) -> Result<Option<DownloadTask>, StorageError> {
        find_task(self, id).await
    }

    pub async fn tasks(&self, filter: &TaskFilter) -> Result<Vec<DownloadTask>, StorageError> {
        list_tasks(self, filter).await
    }

    pub async fn count_tasks(&self, filter: &TaskFilter) -> Result<i64, StorageError> {
        count_tasks(self, filter).await
    }

    pub async fn remove_task(&self, id: &str) -> Result<bool, StorageError> {
        delete_task(self, id).await
    }

    pub async fn set_status(
        &self,
        id: &str,
        status: DownloadStatus,
        error: Option<&str>,
        error_kind: Option<&str>,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<(), StorageError> {
        set_status(self, id, status, error, error_kind, completed_at).await
    }

    pub async fn set_started(&self, id: &str, at: DateTime<Utc>) -> Result<(), StorageError> {
        set_started(self, id, at).await
    }

    pub async fn save_progress(
        &self,
        id: &str,
        downloaded: u64,
        total: Option<u64>,
        speed_bps: f64,
        segments: &[DownloadSegment],
    ) -> Result<(), StorageError> {
        update_progress(self, id, downloaded, total, speed_bps, segments).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn save_metadata(
        &self,
        id: &str,
        final_url: Option<&str>,
        total: Option<u64>,
        etag: Option<&str>,
        last_modified: Option<&str>,
        accepts_ranges: bool,
        content_type: Option<&str>,
        source_domain: Option<&str>,
    ) -> Result<(), StorageError> {
        update_metadata(
            self,
            id,
            final_url,
            total,
            etag,
            last_modified,
            accepts_ranges,
            content_type,
            source_domain,
        )
        .await
    }

    pub async fn set_checksum(&self, id: &str, checksum: &str) -> Result<(), StorageError> {
        set_checksum(self, id, checksum).await
    }

    pub async fn set_organization(
        &self,
        id: &str,
        category: Option<&str>,
        queue_id: Option<&str>,
        collection_id: Option<&str>,
        priority: Option<i32>,
        connections: Option<u32>,
    ) -> Result<(), StorageError> {
        set_organization(
            self,
            id,
            category,
            queue_id,
            collection_id,
            priority,
            connections,
        )
        .await
    }

    pub async fn save_segments(
        &self,
        id: &str,
        segments: &[DownloadSegment],
    ) -> Result<(), StorageError> {
        update_segments_only(self, id, segments).await
    }

    pub async fn record_event(
        &self,
        task_id: &str,
        from: Option<DownloadStatus>,
        to: DownloadStatus,
        note: Option<&str>,
    ) -> Result<(), StorageError> {
        record_event(self, task_id, from, to, note).await
    }

    pub async fn events(&self, task_id: &str, limit: i64) -> Result<Vec<TaskEvent>, StorageError> {
        list_events(self, task_id, limit).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn record_integrity(
        &self,
        task_id: &str,
        algorithm: &str,
        expected: Option<&str>,
        actual: Option<&str>,
        status: &str,
        expected_size: Option<u64>,
        actual_size: Option<u64>,
    ) -> Result<(), StorageError> {
        record_integrity(
            self,
            task_id,
            algorithm,
            expected,
            actual,
            status,
            expected_size,
            actual_size,
        )
        .await
    }

    pub async fn integrity_checks(
        &self,
        task_id: &str,
    ) -> Result<Vec<IntegrityRecord>, StorageError> {
        list_integrity(self, task_id).await
    }

    pub async fn record_speed_sample(
        &self,
        task_id: &str,
        speed_bps: f64,
        downloaded_bytes: u64,
    ) -> Result<(), StorageError> {
        record_speed_sample(self, task_id, speed_bps, downloaded_bytes).await
    }

    pub async fn speed_samples(
        &self,
        hours: i64,
        limit: i64,
    ) -> Result<Vec<SpeedSample>, StorageError> {
        recent_speed_samples(self, hours, limit).await
    }

    pub async fn prune_speed_samples(&self, days: i64) -> Result<u64, StorageError> {
        prune_speed_samples(self, days).await
    }

    pub async fn duplicate_candidates(&self) -> Result<Vec<DownloadTask>, StorageError> {
        tasks_with_duplicate_checksum(self).await
    }

    pub async fn find_by_checksum(
        &self,
        checksum: &str,
    ) -> Result<Option<DownloadTask>, StorageError> {
        find_by_checksum(self, checksum).await
    }
}
