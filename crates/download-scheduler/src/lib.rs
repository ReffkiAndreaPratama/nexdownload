//! Queue scheduling policy.
//!
//! The scheduler is a pure function over task snapshots. That makes its
//! behaviour reproducible in tests and keeps it independent from Tokio.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub max_concurrent: usize,
    pub per_domain_limit: usize,
    pub per_queue_limit: Option<usize>,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 3,
            per_domain_limit: 4,
            per_queue_limit: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerTask {
    pub id: String,
    pub status: String,
    pub priority: i32,
    pub queue_id: Option<String>,
    pub queue_priority: i32,
    pub queue_status: String,
    pub queue_max_concurrent: i32,
    pub domain: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl SchedulerTask {
    pub fn is_active(&self) -> bool {
        matches!(
            self.status.as_str(),
            "preparing" | "connecting" | "downloading" | "pausing" | "verifying"
        )
    }

    pub fn is_queued(&self) -> bool {
        self.status == "queued"
    }
}

/// Select the next task id to start, or `None` when all limits are reached.
pub fn select_next(tasks: &[SchedulerTask], config: &SchedulerConfig) -> Option<String> {
    let active: Vec<&SchedulerTask> = tasks.iter().filter(|task| task.is_active()).collect();
    if active.len() >= config.max_concurrent.max(1) {
        return None;
    }

    let mut candidates: Vec<&SchedulerTask> = tasks
        .iter()
        .filter(|task| {
            task.is_queued()
                && task.queue_status == "active"
                && (task.queue_id.is_none() || task.queue_id.as_deref() != Some("paused"))
        })
        .collect();

    candidates.retain(|task| {
        if let Some(limit) = config.per_queue_limit {
            let queue_key = task.queue_id.as_deref().unwrap_or("default");
            let running = active
                .iter()
                .filter(|t| t.queue_id.as_deref().unwrap_or("default") == queue_key)
                .count();
            let queue_limit = (task.queue_max_concurrent.max(1) as usize).min(limit);
            if running >= queue_limit {
                return false;
            }
        } else {
            let queue_key = task.queue_id.as_deref().unwrap_or("default");
            let running = active
                .iter()
                .filter(|t| t.queue_id.as_deref().unwrap_or("default") == queue_key)
                .count();
            if running >= task.queue_max_concurrent.max(1) as usize {
                return false;
            }
        }

        if config.per_domain_limit > 0 {
            if let Some(domain) = task.domain.as_deref() {
                let running = active
                    .iter()
                    .filter(|t| t.domain.as_deref() == Some(domain))
                    .count();
                if running >= config.per_domain_limit {
                    return false;
                }
            }
        }
        true
    });

    candidates.sort_by(|a, b| {
        b.queue_priority
            .cmp(&a.queue_priority)
            .then_with(|| b.priority.cmp(&a.priority))
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });

    candidates.first().map(|task| task.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, priority: i32, queue: &str, created_secs: i64) -> SchedulerTask {
        SchedulerTask {
            id: id.into(),
            status: "queued".into(),
            priority,
            queue_id: Some(queue.into()),
            queue_priority: 0,
            queue_status: "active".into(),
            queue_max_concurrent: 2,
            domain: Some("example.com".into()),
            created_at: DateTime::<Utc>::UNIX_EPOCH + chrono::Duration::seconds(created_secs),
        }
    }

    #[test]
    fn respects_global_concurrency() {
        let tasks = vec![task("a", 0, "q", 1), task("b", 0, "q", 2)];
        let config = SchedulerConfig {
            max_concurrent: 1,
            ..Default::default()
        };
        let mut active = tasks.clone();
        active[0].status = "downloading".into();
        let all = [active, tasks].concat();
        // Two queued and one active: max_concurrent=1 means no new start.
        assert_eq!(select_next(&all, &config), None);
    }

    #[test]
    fn respects_queue_limit() {
        let mut tasks = vec![task("a", 0, "q", 1), task("b", 0, "q", 2)];
        tasks[0].status = "downloading".into();
        let config = SchedulerConfig {
            max_concurrent: 10,
            per_queue_limit: None,
            per_domain_limit: 0,
        };
        assert_eq!(select_next(&tasks, &config), Some("b".into()));
        tasks[0].queue_max_concurrent = 1;
        tasks[1].queue_max_concurrent = 1;
        assert_eq!(select_next(&tasks, &config), None);
    }

    #[test]
    fn paused_queues_are_skipped() {
        let mut tasks = vec![task("a", 0, "q", 1)];
        tasks[0].queue_status = "paused".into();
        assert_eq!(select_next(&tasks, &SchedulerConfig::default()), None);
    }

    #[test]
    fn priority_then_fifo() {
        let tasks = vec![
            task("low", 0, "q", 1),
            task("high-old", 5, "q", 1),
            task("high-new", 5, "q", 2),
        ];
        let next = select_next(&tasks, &SchedulerConfig::default()).unwrap();
        assert_eq!(next, "high-old");
    }
}
