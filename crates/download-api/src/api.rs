use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Path, Query, Request, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use chrono::Utc;
use download_core::{Category, DownloadStatus, DownloadTask};
use download_rules::{validate_rule, Rule};
use download_storage::{Queue, StoredRule, TaskFilter};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing::warn;

use crate::error::ServiceError;
use crate::service::{
    AddResult, BenchmarkResult, Diagnostics, DownloadService, DuplicateGroup, NewDownload,
};

#[derive(Debug, Clone, Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub details: Value,
}

#[derive(Clone)]
struct ApiState {
    service: DownloadService,
    token: Arc<String>,
    limiter: Arc<RateLimiter>,
    started: Instant,
}

struct RateLimiter {
    window: StdMutex<HashMap<String, Bucket>>,
    limit: u32,
}

struct Bucket {
    started: Instant,
    count: u32,
}

impl RateLimiter {
    fn new(limit: u32) -> Self {
        Self {
            window: StdMutex::new(HashMap::new()),
            limit,
        }
    }

    fn allow(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut windows = self.window.lock().unwrap_or_else(|e| e.into_inner());
        if windows.len() > 4096 {
            windows
                .retain(|_, bucket| now.duration_since(bucket.started) < Duration::from_secs(120));
        }
        let bucket = windows.entry(key.to_string()).or_insert(Bucket {
            started: now,
            count: 0,
        });
        if now.duration_since(bucket.started) >= Duration::from_secs(60) {
            bucket.started = now;
            bucket.count = 0;
        }
        bucket.count += 1;
        bucket.count <= self.limit
    }
}

#[derive(Debug, Clone)]
pub struct ApiOptions {
    pub token: String,
    pub frontend_dir: Option<PathBuf>,
    pub rate_limit_per_minute: u32,
}

impl ApiOptions {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            frontend_dir: None,
            rate_limit_per_minute: 900,
        }
    }
}

/// Build the complete Axum router. Useful for tests and for embedding the API
/// inside the Tauri desktop shell.
pub fn build_router(service: DownloadService, options: &ApiOptions) -> Router {
    let state = ApiState {
        service: service.clone(),
        token: Arc::new(options.token.clone()),
        limiter: Arc::new(RateLimiter::new(options.rate_limit_per_minute.max(60))),
        started: Instant::now(),
    };

    let public = Router::new().route("/health", get(health));

    let protected = Router::new()
        .route("/tasks", get(list_tasks).post(create_task))
        .route(
            "/tasks/{id}",
            get(get_task).delete(delete_task).patch(patch_task),
        )
        .route("/tasks/{id}/pause", post(pause_task))
        .route("/tasks/{id}/resume", post(resume_task))
        .route("/tasks/{id}/retry", post(retry_task))
        .route("/tasks/{id}/cancel", post(cancel_task))
        .route("/tasks/{id}/events", get(task_events))
        .route(
            "/tasks/{id}/integrity",
            get(task_integrity).post(verify_task),
        )
        .route("/tasks/{id}/diagnostics", get(task_diagnostics))
        .route("/stats", get(stats))
        .route("/analytics", get(analytics))
        .route("/categories", get(list_categories).post(upsert_category))
        .route("/categories/{id}", delete(delete_category))
        .route("/queues", get(list_queues).post(create_queue))
        .route("/queues/{id}", patch(update_queue).delete(delete_queue))
        .route("/queues/{id}/pause", post(pause_queue))
        .route("/queues/{id}/start", post(start_queue))
        .route(
            "/collections",
            get(list_collections).post(create_collection),
        )
        .route("/collections/{id}", delete(delete_collection))
        .route("/rules", get(list_rules).post(upsert_rule))
        .route("/rules/{id}", delete(delete_rule))
        .route("/settings", get(get_settings).put(update_settings))
        .route("/benchmark", post(benchmark))
        .route("/duplicates", get(duplicates))
        .route("/history", get(history))
        .route("/history/export", get(export_history))
        .route("/audit", get(audit_log))
        .route("/shutdown", post(shutdown))
        .route_layer(middleware::from_fn_with_state(state.clone(), authorize))
        .route_layer(middleware::from_fn_with_state(state.clone(), rate_limit))
        .route_layer(middleware::from_fn_with_state(state.clone(), audit));

    let api = Router::new()
        .merge(public)
        .merge(protected)
        .fallback(api_not_found)
        .with_state(state);

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| {
            let Ok(value) = origin.to_str() else {
                return false;
            };
            value.starts_with("http://localhost:")
                || value.starts_with("http://127.0.0.1:")
                || value.starts_with("chrome-extension://")
                || value.starts_with("moz-extension://")
                || value == "tauri://localhost"
                || value == "http://tauri.localhost"
                || value == "https://tauri.localhost"
        }))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            AUTHORIZATION,
            CONTENT_TYPE,
            HeaderName::from_static("x-api-token"),
            HeaderName::from_static("range"),
        ])
        .max_age(Duration::from_secs(3600));

    let mut app = Router::new()
        .nest("/api/v1", api)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    if let Some(directory) = &options.frontend_dir {
        if directory.exists() {
            let index = directory.join("index.html");
            app = app.fallback_service(ServeDir::new(directory).fallback(ServeFile::new(index)));
        } else {
            warn!(path = %directory.display(), "frontend directory does not exist; serving API only");
        }
    }

    app
}

/// Run the HTTP server until Ctrl+C or an API shutdown request.
pub async fn serve(
    service: DownloadService,
    listener: tokio::net::TcpListener,
    options: ApiOptions,
) -> std::io::Result<()> {
    let app = build_router(service.clone(), &options);
    let shutdown_service = service.clone();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        let _ = tokio::signal::ctrl_c().await;
        shutdown_service.shutdown().await;
    })
    .await
}

async fn authorize(
    State(state): State<ApiState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .or_else(|| {
            headers
                .get("x-api-token")
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
        });

    match token {
        Some(candidate) if constant_time_eq(candidate.as_bytes(), state.token.as_bytes()) => {
            next.run(request).await
        }
        _ => {
            let body = ApiErrorBody {
                code: "unauthorized".into(),
                message: "A valid API token is required.".into(),
                details: Value::Null,
            };
            let mut response = (StatusCode::UNAUTHORIZED, Json(body)).into_response();
            response.headers_mut().insert(
                HeaderName::from_static("www-authenticate"),
                HeaderValue::from_static("Bearer realm=\"NexDownload\""),
            );
            response
        }
    }
}

async fn rate_limit(State(state): State<ApiState>, request: Request, next: Next) -> Response {
    let key = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip().to_string())
        .unwrap_or_else(|| "local".into());
    if !state.limiter.allow(&key) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ApiErrorBody {
                code: "rate_limited".into(),
                message: "Too many requests. Try again in a minute.".into(),
                details: Value::Null,
            }),
        )
            .into_response();
    }
    next.run(request).await
}

async fn audit(State(state): State<ApiState>, request: Request, next: Next) -> Response {
    let method = request.method().as_str().to_string();
    let path = request.uri().path().to_string();
    let remote = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip().to_string());
    let response = next.run(request).await;
    let status = response.status().as_u16() as i64;
    let storage = state.service.storage().clone();
    let remote_owned = remote.clone();
    tokio::spawn(async move {
        let _ = storage
            .audit(&method, &path, status, remote_owned.as_deref())
            .await;
    });
    response
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

async fn api_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(ApiErrorBody {
            code: "not_found".into(),
            message: "No such API endpoint.".into(),
            details: Value::Null,
        }),
    )
        .into_response()
}

async fn health(State(state): State<ApiState>) -> Json<Value> {
    static REQUESTS: AtomicU64 = AtomicU64::new(0);
    REQUESTS.fetch_add(1, Ordering::Relaxed);
    Json(json!({
        "status": "ok",
        "name": "NexDownload",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_secs": state.started.elapsed().as_secs(),
        "time": Utc::now().to_rfc3339(),
    }))
}

#[derive(Debug, Deserialize)]
struct TaskQuery {
    status: Option<String>,
    category: Option<String>,
    queue_id: Option<String>,
    collection_id: Option<String>,
    search: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

fn parse_statuses(raw: &str) -> Result<Vec<DownloadStatus>, ServiceError> {
    raw.split(',')
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            DownloadStatus::parse(value.trim())
                .ok_or_else(|| ServiceError::Validation(format!("unknown status '{value}'")))
        })
        .collect()
}

async fn list_tasks(
    State(state): State<ApiState>,
    Query(query): Query<TaskQuery>,
) -> Result<Json<Vec<DownloadTask>>, ServiceError> {
    let statuses = match query.status.as_deref() {
        Some(raw) if !raw.trim().is_empty() => parse_statuses(raw)?,
        _ => Vec::new(),
    };
    let filter = TaskFilter {
        statuses,
        category: query.category,
        queue_id: query.queue_id,
        collection_id: query.collection_id,
        search: query.search,
        limit: query.limit.unwrap_or(500),
        offset: query.offset.unwrap_or(0),
    };
    Ok(Json(state.service.storage().tasks(&filter).await?))
}

async fn get_task(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<DownloadTask>, ServiceError> {
    Ok(Json(state.service.storage().task(&id).await?))
}

async fn create_task(
    State(state): State<ApiState>,
    Json(payload): Json<NewDownload>,
) -> Result<(StatusCode, Json<AddResult>), ServiceError> {
    let result = state.service.add_task(payload).await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[derive(Debug, Deserialize)]
struct DeleteQuery {
    #[serde(default)]
    delete_file: bool,
}

async fn delete_task(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Query(query): Query<DeleteQuery>,
) -> Result<StatusCode, ServiceError> {
    state.service.remove_task(&id, query.delete_file).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Default, Deserialize)]
struct ActionBody {
    #[serde(default)]
    restart: bool,
}

async fn pause_task(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<DownloadTask>, ServiceError> {
    Ok(Json(state.service.pause_task(&id).await?))
}

async fn resume_task(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    body: Option<Json<ActionBody>>,
) -> Result<Json<DownloadTask>, ServiceError> {
    let restart = body.map(|Json(body)| body.restart).unwrap_or(false);
    Ok(Json(state.service.resume_task(&id, restart).await?))
}

async fn retry_task(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    body: Option<Json<ActionBody>>,
) -> Result<Json<DownloadTask>, ServiceError> {
    let restart = body.map(|Json(body)| body.restart).unwrap_or(false);
    Ok(Json(state.service.retry_task(&id, restart).await?))
}

async fn cancel_task(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<DownloadTask>, ServiceError> {
    Ok(Json(state.service.cancel_task(&id).await?))
}

#[derive(Debug, Deserialize)]
struct PatchTaskBody {
    category: Option<String>,
    queue_id: Option<String>,
    collection_id: Option<String>,
    priority: Option<i32>,
    connections: Option<u32>,
}

async fn patch_task(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(body): Json<PatchTaskBody>,
) -> Result<Json<DownloadTask>, ServiceError> {
    Ok(Json(
        state
            .service
            .organize_task(
                &id,
                body.category.as_deref(),
                body.queue_id.as_deref(),
                body.collection_id.as_deref(),
                body.priority,
                body.connections,
            )
            .await?,
    ))
}

async fn task_events(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ServiceError> {
    Ok(Json(
        serde_json::to_value(state.service.events_for(&id).await?).unwrap_or(Value::Null),
    ))
}

async fn task_integrity(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ServiceError> {
    Ok(Json(
        serde_json::to_value(state.service.integrity_for(&id).await?).unwrap_or(Value::Null),
    ))
}

async fn verify_task(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ServiceError> {
    Ok(Json(
        serde_json::to_value(state.service.verify_task(&id).await?).unwrap_or(Value::Null),
    ))
}

async fn task_diagnostics(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<Diagnostics>, ServiceError> {
    Ok(Json(state.service.diagnostics(&id).await?))
}

async fn stats(State(state): State<ApiState>) -> Result<Json<Value>, ServiceError> {
    Ok(Json(
        serde_json::to_value(state.service.stats().await?).unwrap_or(Value::Null),
    ))
}

async fn analytics(State(state): State<ApiState>) -> Result<Json<Value>, ServiceError> {
    Ok(Json(
        serde_json::to_value(state.service.analytics().await?).unwrap_or(Value::Null),
    ))
}

async fn list_categories(
    State(state): State<ApiState>,
) -> Result<Json<Vec<Category>>, ServiceError> {
    Ok(Json(state.service.storage().categories().await?))
}

#[derive(Debug, Deserialize)]
struct CategoryBody {
    #[serde(default)]
    id: Option<String>,
    name: String,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    destination_dir: Option<String>,
}

async fn upsert_category(
    State(state): State<ApiState>,
    Json(body): Json<CategoryBody>,
) -> Result<Json<Category>, ServiceError> {
    if body.name.trim().is_empty() {
        return Err(ServiceError::Validation("category name is required".into()));
    }
    let id = body.id.unwrap_or_else(|| slug(&body.name));
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(ServiceError::Validation(
            "category id must contain lowercase letters, digits, '-' or '_'".into(),
        ));
    }
    let category = Category {
        id,
        name: body.name.trim().to_string(),
        extensions: body
            .extensions
            .into_iter()
            .map(|extension| {
                extension
                    .trim()
                    .trim_start_matches('.')
                    .to_ascii_lowercase()
            })
            .filter(|extension| !extension.is_empty())
            .collect(),
        destination_dir: body.destination_dir,
        builtin: false,
    };
    state.service.storage().upsert_category(&category).await?;
    Ok(Json(category))
}

async fn delete_category(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ServiceError> {
    if !state.service.storage().delete_category(&id).await? {
        return Err(ServiceError::Conflict(
            "built-in categories cannot be deleted".into(),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_queues(State(state): State<ApiState>) -> Result<Json<Vec<Queue>>, ServiceError> {
    Ok(Json(state.service.storage().queues().await?))
}

#[derive(Debug, Deserialize)]
struct QueueBody {
    name: String,
    #[serde(default = "default_queue_concurrency")]
    max_concurrent: i32,
}

fn default_queue_concurrency() -> i32 {
    2
}

async fn create_queue(
    State(state): State<ApiState>,
    Json(body): Json<QueueBody>,
) -> Result<(StatusCode, Json<Queue>), ServiceError> {
    let queue = state
        .service
        .create_queue(&body.name, body.max_concurrent)
        .await?;
    Ok((StatusCode::CREATED, Json(queue)))
}

async fn update_queue(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(mut queue): Json<Queue>,
) -> Result<Json<Queue>, ServiceError> {
    queue.id = id;
    state.service.update_queue(&queue).await?;
    Ok(Json(queue))
}

async fn delete_queue(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ServiceError> {
    state.service.delete_queue(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn pause_queue(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ServiceError> {
    state.service.pause_queue(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn start_queue(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ServiceError> {
    state.service.start_queue(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_collections(State(state): State<ApiState>) -> Result<Json<Value>, ServiceError> {
    Ok(Json(
        serde_json::to_value(state.service.storage().collections().await?).unwrap_or(Value::Null),
    ))
}

#[derive(Debug, Deserialize)]
struct CollectionBody {
    name: String,
    description: Option<String>,
    color: Option<String>,
}

async fn create_collection(
    State(state): State<ApiState>,
    Json(body): Json<CollectionBody>,
) -> Result<(StatusCode, Json<Value>), ServiceError> {
    let collection = state
        .service
        .storage()
        .create_collection(
            &body.name,
            body.description.as_deref(),
            body.color.as_deref(),
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(collection).unwrap_or(Value::Null)),
    ))
}

async fn delete_collection(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ServiceError> {
    if !state.service.storage().delete_collection(&id).await? {
        return Err(ServiceError::NotFound(format!("collection {id}")));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_rules(State(state): State<ApiState>) -> Result<Json<Vec<StoredRule>>, ServiceError> {
    Ok(Json(state.service.storage().rules().await?))
}

async fn upsert_rule(
    State(state): State<ApiState>,
    Json(mut rule): Json<Rule>,
) -> Result<Json<StoredRule>, ServiceError> {
    if rule.id.trim().is_empty() {
        rule.id = uuid::Uuid::new_v4().to_string();
    }
    validate_rule(&rule)?;
    let stored = StoredRule {
        id: rule.id.clone(),
        name: rule.name.clone(),
        enabled: rule.enabled,
        priority: rule.priority,
        conditions: serde_json::to_value(&rule.conditions)
            .map_err(|e| ServiceError::Validation(e.to_string()))?,
        actions: serde_json::to_value(&rule.actions)
            .map_err(|e| ServiceError::Validation(e.to_string()))?,
        created_at: Utc::now(),
    };
    state.service.storage().upsert_rule(&stored).await?;
    Ok(Json(stored))
}

async fn delete_rule(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ServiceError> {
    if !state.service.storage().delete_rule(&id).await? {
        return Err(ServiceError::NotFound(format!("rule {id}")));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn get_settings(State(state): State<ApiState>) -> Result<Json<Value>, ServiceError> {
    Ok(Json(
        serde_json::to_value(state.service.settings().await?).unwrap_or(Value::Null),
    ))
}

async fn update_settings(
    State(state): State<ApiState>,
    Json(patch): Json<serde_json::Map<String, Value>>,
) -> Result<Json<Value>, ServiceError> {
    Ok(Json(
        serde_json::to_value(state.service.update_settings(&patch).await?).unwrap_or(Value::Null),
    ))
}

#[derive(Debug, Deserialize)]
struct BenchmarkBody {
    url: String,
}

async fn benchmark(
    State(state): State<ApiState>,
    Json(body): Json<BenchmarkBody>,
) -> Result<Json<BenchmarkResult>, ServiceError> {
    Ok(Json(state.service.benchmark(&body.url).await?))
}

async fn duplicates(
    State(state): State<ApiState>,
) -> Result<Json<Vec<DuplicateGroup>>, ServiceError> {
    Ok(Json(state.service.duplicate_groups().await?))
}

async fn history(
    State(state): State<ApiState>,
    Query(query): Query<TaskQuery>,
) -> Result<Json<Vec<DownloadTask>>, ServiceError> {
    let filter = TaskFilter {
        statuses: vec![
            DownloadStatus::Completed,
            DownloadStatus::Failed,
            DownloadStatus::Cancelled,
        ],
        search: query.search,
        limit: query.limit.unwrap_or(500),
        offset: query.offset.unwrap_or(0),
        ..Default::default()
    };
    Ok(Json(state.service.storage().tasks(&filter).await?))
}

async fn export_history(State(state): State<ApiState>) -> Result<Response, ServiceError> {
    let csv = state.service.export_history_csv().await?;
    let mut response = csv.into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    response.headers_mut().insert(
        HeaderName::from_static("content-disposition"),
        HeaderValue::from_static("attachment; filename=\"nexdownload-history.csv\""),
    );
    Ok(response)
}

async fn audit_log(State(state): State<ApiState>) -> Result<Json<Value>, ServiceError> {
    Ok(Json(
        serde_json::to_value(state.service.storage().recent_audit(100).await?)
            .unwrap_or(Value::Null),
    ))
}

async fn shutdown(State(state): State<ApiState>) -> Json<Value> {
    let service = state.service.clone();
    tokio::spawn(async move {
        service.shutdown().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        std::process::exit(0);
    });
    Json(json!({ "status": "shutting_down" }))
}

fn slug(value: &str) -> String {
    let mut slug: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    slug.trim_matches('-').to_string()
}

#[allow(dead_code)]
fn _ensure_serializable(_: &ApiErrorBody) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_comparison_works() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secreT"));
        assert!(!constant_time_eq(b"secret", b"secre"));
    }

    #[test]
    fn slug_is_safe() {
        assert_eq!(slug("Game Assets!"), "game-assets");
        assert_eq!(slug("C++ / Rust"), "c-rust");
    }

    #[test]
    fn parses_status_lists() {
        let statuses = parse_statuses("queued,downloading").unwrap();
        assert_eq!(
            statuses,
            vec![DownloadStatus::Queued, DownloadStatus::Downloading]
        );
        assert!(parse_statuses("nonsense").is_err());
    }
}
