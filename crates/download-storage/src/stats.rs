use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::tasks::SpeedSample;
use crate::{Storage, StorageError};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Stats {
    pub total_tasks: i64,
    pub active: i64,
    pub queued: i64,
    pub paused: i64,
    pub completed: i64,
    pub failed: i64,
    pub cancelled: i64,
    pub total_downloaded_bytes: i64,
    pub total_known_bytes: i64,
    pub current_speed_bps: f64,
    pub peak_speed_bps: f64,
    pub average_speed_bps: f64,
    pub average_duration_secs: f64,
    pub success_rate: f64,
    pub completed_today: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryStat {
    pub category: String,
    pub count: i64,
    pub bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyStat {
    pub date: String,
    pub completed: i64,
    pub bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStat {
    pub queue_id: String,
    pub name: String,
    pub active: i64,
    pub queued: i64,
    pub total: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Analytics {
    pub stats: Stats,
    pub categories: Vec<CategoryStat>,
    pub daily: Vec<DailyStat>,
    pub queues: Vec<QueueStat>,
    pub speed_history: Vec<SpeedSample>,
}

impl Storage {
    pub async fn stats(&self) -> Result<Stats, StorageError> {
        let today = chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .to_rfc3339();
        let row = sqlx::query(
            "SELECT
                COUNT(*) AS total_tasks,
                COALESCE(SUM(CASE WHEN status IN ('preparing','connecting','downloading','pausing','verifying') THEN 1 ELSE 0 END), 0) AS active,
                COALESCE(SUM(CASE WHEN status = 'queued' THEN 1 ELSE 0 END), 0) AS queued,
                COALESCE(SUM(CASE WHEN status = 'paused' THEN 1 ELSE 0 END), 0) AS paused,
                COALESCE(SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END), 0) AS completed,
                COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) AS failed,
                COALESCE(SUM(CASE WHEN status = 'cancelled' THEN 1 ELSE 0 END), 0) AS cancelled,
                COALESCE(SUM(downloaded_bytes), 0) AS total_downloaded,
                COALESCE(SUM(total_bytes), 0) AS total_known,
                COALESCE(SUM(speed_bps), 0.0) AS current_speed,
                COALESCE(SUM(CASE WHEN status = 'completed' AND completed_at >= ? THEN 1 ELSE 0 END), 0) AS completed_today
             FROM tasks",
        )
        .bind(today)
        .fetch_one(self.pool())
        .await?;

        let peak: f64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(speed_bps), 0.0) FROM speed_samples")
                .fetch_one(self.pool())
                .await?;

        let durations = sqlx::query(
            "SELECT
                COALESCE(AVG(CAST(downloaded_bytes AS REAL) /
                    MAX((julianday(completed_at) - julianday(started_at)) * 86400.0, 1.0)), 0.0) AS avg_speed,
                COALESCE(AVG((julianday(completed_at) - julianday(started_at)) * 86400.0), 0.0) AS avg_duration
             FROM tasks
             WHERE status = 'completed' AND started_at IS NOT NULL AND completed_at IS NOT NULL",
        )
        .fetch_one(self.pool())
        .await?;

        let completed: i64 = row.try_get("completed")?;
        let failed: i64 = row.try_get("failed")?;
        let finished = completed + failed;
        let success_rate = if finished > 0 {
            completed as f64 / finished as f64 * 100.0
        } else {
            100.0
        };

        Ok(Stats {
            total_tasks: row.try_get("total_tasks")?,
            active: row.try_get("active")?,
            queued: row.try_get("queued")?,
            paused: row.try_get("paused")?,
            completed,
            failed,
            cancelled: row.try_get("cancelled")?,
            total_downloaded_bytes: row.try_get("total_downloaded")?,
            total_known_bytes: row.try_get("total_known")?,
            current_speed_bps: row.try_get("current_speed")?,
            peak_speed_bps: peak,
            average_speed_bps: durations.try_get("avg_speed").unwrap_or(0.0),
            average_duration_secs: durations.try_get("avg_duration").unwrap_or(0.0),
            success_rate,
            completed_today: row.try_get("completed_today")?,
        })
    }

    pub async fn category_stats(&self) -> Result<Vec<CategoryStat>, StorageError> {
        let rows = sqlx::query(
            "SELECT category, COUNT(*) AS count, COALESCE(SUM(downloaded_bytes), 0) AS bytes
             FROM tasks GROUP BY category ORDER BY bytes DESC",
        )
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|row| {
                Ok(CategoryStat {
                    category: row.try_get("category")?,
                    count: row.try_get("count")?,
                    bytes: row.try_get("bytes")?,
                })
            })
            .collect()
    }

    pub async fn daily_stats(&self, days: i64) -> Result<Vec<DailyStat>, StorageError> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(days.clamp(1, 365)))
            .date_naive()
            .to_string();
        let rows = sqlx::query(
            "SELECT substr(completed_at, 1, 10) AS day, COUNT(*) AS completed,
                    COALESCE(SUM(downloaded_bytes), 0) AS bytes
             FROM tasks
             WHERE status = 'completed' AND completed_at IS NOT NULL AND substr(completed_at, 1, 10) >= ?
             GROUP BY day ORDER BY day ASC",
        )
        .bind(cutoff)
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|row| {
                Ok(DailyStat {
                    date: row.try_get("day")?,
                    completed: row.try_get("completed")?,
                    bytes: row.try_get("bytes")?,
                })
            })
            .collect()
    }

    pub async fn queue_stats(&self) -> Result<Vec<QueueStat>, StorageError> {
        let rows = sqlx::query(
            "SELECT q.id AS queue_id, q.name AS name,
                COALESCE(SUM(CASE WHEN t.status IN ('preparing','connecting','downloading','pausing','verifying') THEN 1 ELSE 0 END), 0) AS active,
                COALESCE(SUM(CASE WHEN t.status = 'queued' THEN 1 ELSE 0 END), 0) AS queued,
                COUNT(t.id) AS total
             FROM queues q LEFT JOIN tasks t ON t.queue_id = q.id
             GROUP BY q.id, q.name, q.priority ORDER BY q.priority DESC, q.created_at ASC",
        )
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|row| {
                Ok(QueueStat {
                    queue_id: row.try_get("queue_id")?,
                    name: row.try_get("name")?,
                    active: row.try_get("active")?,
                    queued: row.try_get("queued")?,
                    total: row.try_get("total")?,
                })
            })
            .collect()
    }

    pub async fn analytics(&self) -> Result<Analytics, StorageError> {
        let stats = self.stats().await?;
        let categories = self.category_stats().await?;
        let daily = self.daily_stats(14).await?;
        let queues = self.queue_stats().await?;
        let speed_history = self.speed_samples(24, 400).await?;
        Ok(Analytics {
            stats,
            categories,
            daily,
            queues,
            speed_history,
        })
    }

    pub async fn find_duplicate_groups(
        &self,
    ) -> Result<Vec<Vec<download_core::DownloadTask>>, StorageError> {
        let tasks = self.duplicate_candidates().await?;
        let mut groups: std::collections::BTreeMap<String, Vec<download_core::DownloadTask>> =
            std::collections::BTreeMap::new();
        for task in tasks {
            if let Some(checksum) = task.checksum_sha256.clone() {
                groups.entry(checksum).or_default().push(task);
            }
        }
        Ok(groups.into_values().collect())
    }
}
