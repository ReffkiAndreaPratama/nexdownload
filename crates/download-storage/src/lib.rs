//! SQLite persistence layer.
//!
//! Segment state is stored as a JSON array on the task row. Updating progress
//! and segment checkpoints is therefore a single atomic row update, which keeps
//! the write rate low and makes crash recovery consistent.

mod error;
mod metadata;
mod stats;
mod tasks;

use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

pub use error::StorageError;
pub use metadata::{Collection, Queue, StoredRule};
pub use stats::{Analytics, CategoryStat, DailyStat, Stats};
pub use tasks::{IntegrityRecord, SpeedSample, TaskEvent, TaskFilter};

use download_core::{builtin_categories, DownloadTask};

#[derive(Clone)]
pub struct Storage {
    pool: SqlitePool,
}

impl Storage {
    /// Open a database file, creating it and its parent directory if needed.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| StorageError::Invalid(format!("cannot create data directory: {e}")))?;
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(10));
        Self::connect(options, 5).await
    }

    /// Isolated in-memory database for tests.
    pub async fn in_memory() -> Result<Self, StorageError> {
        let options = SqliteConnectOptions::new()
            .in_memory(true)
            .foreign_keys(true);
        Self::connect(options, 1).await
    }

    async fn connect(
        options: SqliteConnectOptions,
        max_connections: u32,
    ) -> Result<Self, StorageError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(options)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;
        let storage = Self { pool };
        storage.seed().await?;
        Ok(storage)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }

    async fn seed(&self) -> Result<(), StorageError> {
        let now = now_rfc3339();
        for category in builtin_categories() {
            sqlx::query(
                "INSERT OR IGNORE INTO categories (id, name, extensions_json, destination_dir, builtin, created_at)
                 VALUES (?, ?, ?, NULL, 1, ?)",
            )
            .bind(&category.id)
            .bind(&category.name)
            .bind(serde_json::to_string(&category.extensions)?)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        }

        sqlx::query(
            "INSERT OR IGNORE INTO queues (id, name, priority, max_concurrent, status, created_at)
             VALUES ('default', 'Main queue', 0, 2, 'active', ?)",
        )
        .bind(&now)
        .execute(&self.pool)
        .await?;

        let defaults: [(&str, serde_json::Value); 14] = [
            ("theme", serde_json::json!("dark")),
            ("compact_rows", serde_json::json!(false)),
            ("max_concurrent_downloads", serde_json::json!(3)),
            ("default_connections", serde_json::json!(4)),
            ("default_destination", serde_json::json!("")),
            ("bandwidth_limit_bps", serde_json::json!(0)),
            ("adaptive_connections", serde_json::json!(true)),
            ("auto_resume", serde_json::json!(true)),
            ("verify_integrity", serde_json::json!(true)),
            ("notifications", serde_json::json!(true)),
            ("intercept_downloads", serde_json::json!(true)),
            ("lan_access", serde_json::json!(false)),
            ("retry_limit", serde_json::json!(4)),
            ("timeout_secs", serde_json::json!(30)),
        ];
        for (key, value) in defaults {
            sqlx::query(
                "INSERT OR IGNORE INTO settings (key, value_json, updated_at) VALUES (?, ?, ?)",
            )
            .bind(key)
            .bind(serde_json::to_string(&value)?)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn save_task(&self, task: &DownloadTask) -> Result<(), StorageError> {
        tasks::insert_task(self, task).await
    }

    pub async fn task(&self, id: &str) -> Result<DownloadTask, StorageError> {
        tasks::get_task(self, id).await
    }

    pub async fn sample_tasks(
        &self,
        filter: TaskFilter,
    ) -> Result<Vec<DownloadTask>, StorageError> {
        tasks::list_tasks(self, &filter).await
    }
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn dt(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .map(|v| v.with_timezone(&Utc))
        .unwrap_or_else(|_| DateTime::<Utc>::UNIX_EPOCH)
}

pub(crate) fn opt_dt(value: &Option<String>) -> Option<DateTime<Utc>> {
    value.as_deref().map(dt)
}

pub(crate) fn opt_time(value: &Option<DateTime<Utc>>) -> Option<String> {
    value.map(|v| v.to_rfc3339())
}
