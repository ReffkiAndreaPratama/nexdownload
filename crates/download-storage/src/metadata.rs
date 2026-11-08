use chrono::{DateTime, Utc};
use download_core::Category;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;

use crate::{dt, now_rfc3339, Storage, StorageError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Queue {
    pub id: String,
    pub name: String,
    pub priority: i32,
    pub max_concurrent: i32,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub conditions: Value,
    pub actions: Value,
    pub created_at: DateTime<Utc>,
}

impl Storage {
    pub async fn categories(&self) -> Result<Vec<Category>, StorageError> {
        let rows = sqlx::query(
            "SELECT id, name, extensions_json, destination_dir, builtin FROM categories ORDER BY builtin DESC, name",
        )
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|row| {
                let extensions: Vec<String> =
                    serde_json::from_str(&row.try_get::<String, _>("extensions_json")?)?;
                Ok(Category {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    extensions,
                    destination_dir: row.try_get("destination_dir")?,
                    builtin: row.try_get::<i64, _>("builtin")? != 0,
                })
            })
            .collect()
    }

    pub async fn category(&self, id: &str) -> Result<Option<Category>, StorageError> {
        Ok(self.categories().await?.into_iter().find(|c| c.id == id))
    }

    pub async fn upsert_category(&self, category: &Category) -> Result<(), StorageError> {
        if category.id.trim().is_empty() || category.name.trim().is_empty() {
            return Err(StorageError::Invalid(
                "category id and name are required".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO categories (id, name, extensions_json, destination_dir, builtin, created_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                extensions_json = excluded.extensions_json,
                destination_dir = excluded.destination_dir",
        )
        .bind(&category.id)
        .bind(&category.name)
        .bind(serde_json::to_string(&category.extensions)?)
        .bind(&category.destination_dir)
        .bind(if category.builtin { 1i64 } else { 0i64 })
        .bind(now_rfc3339())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn delete_category(&self, id: &str) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM categories WHERE id = ? AND builtin = 0")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn queues(&self) -> Result<Vec<Queue>, StorageError> {
        let rows = sqlx::query(
            "SELECT id, name, priority, max_concurrent, status, created_at FROM queues
             ORDER BY priority DESC, created_at ASC",
        )
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|row| {
                Ok(Queue {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    priority: row.try_get("priority")?,
                    max_concurrent: row.try_get("max_concurrent")?,
                    status: row.try_get("status")?,
                    created_at: dt(&row.try_get::<String, _>("created_at")?),
                })
            })
            .collect()
    }

    pub async fn create_queue(
        &self,
        name: &str,
        max_concurrent: i32,
    ) -> Result<Queue, StorageError> {
        if name.trim().is_empty() {
            return Err(StorageError::Invalid("queue name is required".into()));
        }
        let queue = Queue {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.trim().to_string(),
            priority: 0,
            max_concurrent: max_concurrent.clamp(1, 32),
            status: "active".into(),
            created_at: Utc::now(),
        };
        sqlx::query(
            "INSERT INTO queues (id, name, priority, max_concurrent, status, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&queue.id)
        .bind(&queue.name)
        .bind(queue.priority)
        .bind(queue.max_concurrent)
        .bind(&queue.status)
        .bind(queue.created_at.to_rfc3339())
        .execute(self.pool())
        .await?;
        Ok(queue)
    }

    pub async fn update_queue(&self, queue: &Queue) -> Result<(), StorageError> {
        let result = sqlx::query(
            "UPDATE queues SET name = ?, priority = ?, max_concurrent = ?, status = ? WHERE id = ?",
        )
        .bind(&queue.name)
        .bind(queue.priority)
        .bind(queue.max_concurrent.clamp(1, 32))
        .bind(&queue.status)
        .bind(&queue.id)
        .execute(self.pool())
        .await?;
        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound(format!("queue {}", queue.id)));
        }
        Ok(())
    }

    pub async fn delete_queue(&self, id: &str) -> Result<bool, StorageError> {
        if id == "default" {
            return Err(StorageError::Invalid(
                "the default queue cannot be deleted".into(),
            ));
        }
        let result = sqlx::query("DELETE FROM queues WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn collections(&self) -> Result<Vec<Collection>, StorageError> {
        let rows = sqlx::query(
            "SELECT id, name, description, color, created_at FROM collections ORDER BY created_at DESC",
        )
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|row| {
                Ok(Collection {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    description: row.try_get("description")?,
                    color: row.try_get("color")?,
                    created_at: dt(&row.try_get::<String, _>("created_at")?),
                })
            })
            .collect()
    }

    pub async fn create_collection(
        &self,
        name: &str,
        description: Option<&str>,
        color: Option<&str>,
    ) -> Result<Collection, StorageError> {
        if name.trim().is_empty() {
            return Err(StorageError::Invalid("collection name is required".into()));
        }
        let collection = Collection {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.trim().to_string(),
            description: description.map(|s| s.to_string()),
            color: color.map(|s| s.to_string()),
            created_at: Utc::now(),
        };
        sqlx::query(
            "INSERT INTO collections (id, name, description, color, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&collection.id)
        .bind(&collection.name)
        .bind(&collection.description)
        .bind(&collection.color)
        .bind(collection.created_at.to_rfc3339())
        .execute(self.pool())
        .await?;
        Ok(collection)
    }

    pub async fn delete_collection(&self, id: &str) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM collections WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn rules(&self) -> Result<Vec<StoredRule>, StorageError> {
        let rows = sqlx::query(
            "SELECT id, name, enabled, priority, conditions_json, actions_json, created_at
             FROM download_rules ORDER BY priority DESC, created_at ASC",
        )
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|row| {
                Ok(StoredRule {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    enabled: row.try_get::<i64, _>("enabled")? != 0,
                    priority: row.try_get("priority")?,
                    conditions: serde_json::from_str(
                        &row.try_get::<String, _>("conditions_json")?,
                    )?,
                    actions: serde_json::from_str(&row.try_get::<String, _>("actions_json")?)?,
                    created_at: dt(&row.try_get::<String, _>("created_at")?),
                })
            })
            .collect()
    }

    pub async fn upsert_rule(&self, rule: &StoredRule) -> Result<(), StorageError> {
        if rule.name.trim().is_empty() {
            return Err(StorageError::Invalid("rule name is required".into()));
        }
        sqlx::query(
            "INSERT INTO download_rules (id, name, enabled, priority, conditions_json, actions_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                enabled = excluded.enabled,
                priority = excluded.priority,
                conditions_json = excluded.conditions_json,
                actions_json = excluded.actions_json",
        )
        .bind(&rule.id)
        .bind(&rule.name)
        .bind(if rule.enabled { 1i64 } else { 0i64 })
        .bind(rule.priority)
        .bind(serde_json::to_string(&rule.conditions)?)
        .bind(serde_json::to_string(&rule.actions)?)
        .bind(rule.created_at.to_rfc3339())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn delete_rule(&self, id: &str) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM download_rules WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn settings(&self) -> Result<std::collections::HashMap<String, Value>, StorageError> {
        let rows = sqlx::query("SELECT key, value_json FROM settings")
            .fetch_all(self.pool())
            .await?;
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let key: String = row.try_get("key")?;
            let raw: String = row.try_get("value_json")?;
            let value = serde_json::from_str(&raw).unwrap_or(Value::String(raw));
            map.insert(key, value);
        }
        Ok(map)
    }

    pub async fn setting(&self, key: &str) -> Result<Option<Value>, StorageError> {
        let row = sqlx::query("SELECT value_json FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.and_then(|row| {
            row.try_get::<String, _>("value_json")
                .ok()
                .and_then(|raw| serde_json::from_str(&raw).ok())
        }))
    }

    pub async fn set_setting(&self, key: &str, value: &Value) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO settings (key, value_json, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(serde_json::to_string(value)?)
        .bind(now_rfc3339())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn audit(
        &self,
        method: &str,
        path: &str,
        status: i64,
        remote: Option<&str>,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO api_audit (method, path, status, remote, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(method)
        .bind(path)
        .bind(status)
        .bind(remote)
        .bind(now_rfc3339())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn recent_audit(&self, limit: i64) -> Result<Vec<Value>, StorageError> {
        let rows = sqlx::query(
            "SELECT method, path, status, remote, created_at FROM api_audit ORDER BY id DESC LIMIT ?",
        )
        .bind(limit.clamp(1, 500))
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "method": row.try_get::<String, _>("method").unwrap_or_default(),
                    "path": row.try_get::<String, _>("path").unwrap_or_default(),
                    "status": row.try_get::<i64, _>("status").unwrap_or(0),
                    "remote": row.try_get::<Option<String>, _>("remote").unwrap_or(None),
                    "created_at": row.try_get::<String, _>("created_at").unwrap_or_default(),
                })
            })
            .collect())
    }
}
