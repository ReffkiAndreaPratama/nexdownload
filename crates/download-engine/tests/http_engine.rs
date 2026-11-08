//! Integration tests against a real local HTTP server. The server implements
//! byte ranges, redirects, servers without range support, chunked responses and
//! changing ETags so the engine is exercised over the network rather than with
//! mocked responses.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::State;
use axum::http::header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG, RANGE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::Router;
use bytes::Bytes;
use download_core::{DownloadSegment, EngineOptions, SegmentPlan};
use download_engine::{
    DownloadEngine, DownloadOutcome, DownloadRequest, EngineContext, EngineError, NativeHttpEngine,
};
use futures_util::stream;
use tokio::sync::mpsc;
use url::Url;

const SIZE: usize = 1_048_576;
const ETAG_VALUE: &str = "\"nex-v1\"";
const PLAIN_ETAG: &str = "\"plain-v1\"";

struct TestState {
    data: Vec<u8>,
    ranges: Mutex<Vec<(usize, usize)>>,
}

impl TestState {
    fn new() -> Self {
        Self {
            data: (0..SIZE).map(|index| (index * 31 % 251) as u8).collect(),
            ranges: Mutex::new(Vec::new()),
        }
    }
}

fn parse_range(headers: &HeaderMap) -> Option<(usize, usize)> {
    let value = headers.get(RANGE)?.to_str().ok()?;
    let rest = value.strip_prefix("bytes=")?;
    let (start, end) = rest.split_once('-')?;
    let start: usize = start.parse().ok()?;
    if start >= SIZE {
        return None;
    }
    let end = if end.trim().is_empty() {
        SIZE - 1
    } else {
        end.parse::<usize>().ok()?.min(SIZE - 1)
    };
    (start <= end).then_some((start, end))
}

fn head_response(length: Option<usize>, etag: &str, ranges: bool) -> Response {
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/octet-stream")
        .header(ETAG, etag);
    if let Some(length) = length {
        builder = builder.header(CONTENT_LENGTH, length.to_string());
    }
    if ranges {
        builder = builder.header(ACCEPT_RANGES, "bytes");
    }
    builder.body(Body::empty()).unwrap()
}

async fn ranged_head(State(_state): State<Arc<TestState>>) -> Response {
    head_response(Some(SIZE), ETAG_VALUE, true)
}

async fn ranged_get(State(state): State<Arc<TestState>>, headers: HeaderMap) -> Response {
    if let Some((start, end)) = parse_range(&headers) {
        state.ranges.lock().unwrap().push((start, end));
        let body = state.data[start..=end].to_vec();
        return Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(CONTENT_TYPE, "application/octet-stream")
            .header(CONTENT_RANGE, format!("bytes {start}-{end}/{SIZE}"))
            .header(CONTENT_LENGTH, body.len().to_string())
            .header(ACCEPT_RANGES, "bytes")
            .header(ETAG, ETAG_VALUE)
            .body(Body::from(body))
            .unwrap();
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/octet-stream")
        .header(CONTENT_LENGTH, SIZE.to_string())
        .header(ACCEPT_RANGES, "bytes")
        .header(ETAG, ETAG_VALUE)
        .body(Body::from(state.data.clone()))
        .unwrap()
}

async fn plain_head() -> Response {
    head_response(Some(SIZE), PLAIN_ETAG, false)
}

async fn plain_get(State(state): State<Arc<TestState>>, _headers: HeaderMap) -> Response {
    // Deliberately ignores Range: the engine must fall back to one connection.
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/octet-stream")
        .header(CONTENT_LENGTH, SIZE.to_string())
        .header(ETAG, PLAIN_ETAG)
        .body(Body::from(state.data.clone()))
        .unwrap()
}

async fn redirect_head() -> Response {
    Redirect::temporary("/file.bin").into_response()
}

async fn redirect_get() -> Response {
    Redirect::temporary("/file.bin").into_response()
}

async fn unknown_head() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/octet-stream")
        .header(ETAG, ETAG_VALUE)
        .body(Body::empty())
        .unwrap()
}

async fn unknown_get(State(state): State<Arc<TestState>>) -> Response {
    let data = state.data.clone();
    let chunks = stream::iter(vec![
        Ok::<Bytes, Infallible>(Bytes::copy_from_slice(&data[..SIZE / 2])),
        Ok::<Bytes, Infallible>(Bytes::copy_from_slice(&data[SIZE / 2..])),
    ]);
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/octet-stream")
        .header(ETAG, ETAG_VALUE)
        .body(Body::from_stream(chunks))
        .unwrap()
}

async fn start_server() -> (SocketAddr, Arc<TestState>) {
    let state = Arc::new(TestState::new());
    let app = Router::new()
        .route("/file.bin", get(ranged_get).head(ranged_head))
        .route("/plain.bin", get(plain_get).head(plain_head))
        .route("/redirect", get(redirect_get).head(redirect_head))
        .route("/unknown.bin", get(unknown_get).head(unknown_head))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (address, state)
}

fn request(url: Url, destination: std::path::PathBuf) -> DownloadRequest {
    DownloadRequest {
        task_id: "test-task".into(),
        url,
        destination,
        expected_size: None,
        etag: None,
        last_modified: None,
        accepts_ranges: false,
        segments: Vec::new(),
        options: EngineOptions {
            max_connections: 4,
            timeout_secs: 10,
            max_retries: 1,
            ..EngineOptions::default()
        },
    }
}

#[tokio::test]
async fn inspects_and_downloads_with_multiple_ranges() {
    let (address, state) = start_server().await;
    let engine = NativeHttpEngine::new();
    let url = Url::parse(&format!("http://{address}/file.bin")).unwrap();
    let info = engine
        .inspect(&url, &EngineOptions::default())
        .await
        .unwrap();
    assert_eq!(info.total_bytes, Some(SIZE as u64));
    assert!(info.accepts_ranges);
    assert_eq!(info.etag.as_deref(), Some(ETAG_VALUE));

    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("nested").join("file.bin");
    let (progress, _receiver) = mpsc::channel(64);
    let outcome = engine
        .run(
            request(url, destination.clone()),
            EngineContext::new(progress),
        )
        .await
        .unwrap();

    match outcome {
        DownloadOutcome::Completed {
            downloaded,
            total,
            segments,
        } => {
            assert_eq!(downloaded, SIZE as u64);
            assert_eq!(total, Some(SIZE as u64));
            assert!(segments.len() > 1);
            assert!(segments.iter().all(DownloadSegment::is_complete));
        }
        other => panic!("expected completed outcome, got {other:?}"),
    }
    assert_eq!(tokio::fs::read(&destination).await.unwrap(), state.data);
    assert!(!state.ranges.lock().unwrap().is_empty());
}

#[tokio::test]
async fn follows_redirects_and_uses_the_final_url() {
    let (address, state) = start_server().await;
    let engine = NativeHttpEngine::new();
    let url = Url::parse(&format!("http://{address}/redirect")).unwrap();
    let info = engine
        .inspect(&url, &EngineOptions::default())
        .await
        .unwrap();
    assert!(
        info.final_url.ends_with("/file.bin"),
        "final URL was {}",
        info.final_url
    );

    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("redirect.bin");
    let (progress, _receiver) = mpsc::channel(32);
    let outcome = engine
        .run(
            request(url, destination.clone()),
            EngineContext::new(progress),
        )
        .await
        .unwrap();
    assert_eq!(outcome.downloaded(), SIZE as u64);
    assert_eq!(tokio::fs::read(&destination).await.unwrap(), state.data);
}

#[tokio::test]
async fn falls_back_to_one_connection_without_range_support() {
    let (address, state) = start_server().await;
    let engine = NativeHttpEngine::new();
    let url = Url::parse(&format!("http://{address}/plain.bin")).unwrap();
    let info = engine
        .inspect(&url, &EngineOptions::default())
        .await
        .unwrap();
    assert_eq!(info.total_bytes, Some(SIZE as u64));
    assert!(!info.accepts_ranges);

    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("plain.bin");
    let (progress, _receiver) = mpsc::channel(32);
    let outcome = engine
        .run(
            request(url, destination.clone()),
            EngineContext::new(progress),
        )
        .await
        .unwrap();
    match outcome {
        DownloadOutcome::Completed {
            downloaded,
            total,
            segments,
        } => {
            assert_eq!(downloaded, SIZE as u64);
            assert_eq!(total, Some(SIZE as u64));
            assert_eq!(segments.len(), 1);
        }
        other => panic!("expected completed outcome, got {other:?}"),
    }
    assert_eq!(tokio::fs::read(&destination).await.unwrap(), state.data);
}

#[tokio::test]
async fn completes_chunked_unknown_length_downloads() {
    let (address, state) = start_server().await;
    let engine = NativeHttpEngine::new();
    let url = Url::parse(&format!("http://{address}/unknown.bin")).unwrap();
    let info = engine
        .inspect(&url, &EngineOptions::default())
        .await
        .unwrap();
    assert!(info.total_bytes.unwrap_or(0) == 0, "unexpected known size");

    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("unknown.bin");
    let (progress, _receiver) = mpsc::channel(32);
    let outcome = engine
        .run(
            request(url, destination.clone()),
            EngineContext::new(progress),
        )
        .await
        .unwrap();
    assert_eq!(outcome.downloaded(), SIZE as u64);
    assert_eq!(tokio::fs::read(&destination).await.unwrap(), state.data);
}

#[tokio::test]
async fn resumes_from_saved_segments() {
    let (address, state) = start_server().await;
    let engine = NativeHttpEngine::new();
    let url = Url::parse(&format!("http://{address}/file.bin")).unwrap();
    let plan = SegmentPlan::build(SIZE as u64, 4).unwrap();
    let first = plan[0].clone();
    let first_len = first.len() as usize;
    let mut segments = plan.clone();
    segments[0].downloaded = first.len();

    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("resume.bin");
    let mut partial = vec![0u8; SIZE];
    partial[..first_len].copy_from_slice(&state.data[..first_len]);
    tokio::fs::write(&destination, partial).await.unwrap();

    let mut resume = request(url, destination.clone());
    resume.expected_size = Some(SIZE as u64);
    resume.etag = Some(ETAG_VALUE.to_string());
    resume.segments = segments;
    let (progress, _receiver) = mpsc::channel(64);
    let outcome = engine
        .run(resume, EngineContext::new(progress))
        .await
        .unwrap();
    match outcome {
        DownloadOutcome::Completed {
            downloaded,
            total,
            segments,
        } => {
            assert_eq!(downloaded, SIZE as u64);
            assert_eq!(total, Some(SIZE as u64));
            assert!(segments.iter().all(DownloadSegment::is_complete));
        }
        other => panic!("expected completed outcome, got {other:?}"),
    }
    assert_eq!(tokio::fs::read(&destination).await.unwrap(), state.data);
    let ranges = state.ranges.lock().unwrap().clone();
    assert!(
        ranges
            .iter()
            .all(|(start, _)| *start >= first.len() as usize),
        "resume re-fetched the completed first segment: {ranges:?}"
    );
}

#[tokio::test]
async fn rejects_changed_resources_before_writing() {
    let (address, _state) = start_server().await;
    let engine = NativeHttpEngine::new();
    let url = Url::parse(&format!("http://{address}/file.bin")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("changed.bin");

    let mut wrong_size = request(url.clone(), destination.clone());
    wrong_size.expected_size = Some(SIZE as u64 + 1);
    wrong_size.etag = Some(ETAG_VALUE.to_string());
    wrong_size.segments = vec![DownloadSegment {
        index: 0,
        start: 0,
        end: SIZE as u64 - 1,
        downloaded: 0,
    }];
    let (progress, _receiver) = mpsc::channel(8);
    let error = engine
        .run(wrong_size, EngineContext::new(progress))
        .await
        .unwrap_err();
    assert!(matches!(error, EngineError::ResourceChanged));

    let mut wrong_etag = request(url, destination.clone());
    wrong_etag.expected_size = Some(SIZE as u64);
    wrong_etag.etag = Some("\"old-etag\"".to_string());
    wrong_etag.segments = vec![DownloadSegment {
        index: 0,
        start: 0,
        end: SIZE as u64 - 1,
        downloaded: 0,
    }];
    let (progress, _receiver) = mpsc::channel(8);
    let error = engine
        .run(wrong_etag, EngineContext::new(progress))
        .await
        .unwrap_err();
    assert!(matches!(error, EngineError::ResourceChanged));
    assert!(!destination.exists());
}

#[tokio::test]
async fn pause_before_transfer_returns_recoverable_segments() {
    let (address, state) = start_server().await;
    let engine = NativeHttpEngine::new();
    let url = Url::parse(&format!("http://{address}/file.bin")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("paused.bin");

    let (progress, _receiver) = mpsc::channel(8);
    let context = EngineContext::new(progress);
    context.pause.cancel();
    let paused = engine
        .run(request(url.clone(), destination.clone()), context)
        .await
        .unwrap();
    assert!(matches!(paused, DownloadOutcome::Paused { .. }));
    assert_eq!(paused.downloaded(), 0);
    let segments = paused.segments().to_vec();

    let mut resume = request(url, destination.clone());
    resume.expected_size = Some(SIZE as u64);
    resume.etag = Some(ETAG_VALUE.to_string());
    resume.segments = segments;
    let (progress, _receiver) = mpsc::channel(32);
    let completed = engine
        .run(resume, EngineContext::new(progress))
        .await
        .unwrap();
    assert!(matches!(completed, DownloadOutcome::Completed { .. }));
    assert_eq!(tokio::fs::read(&destination).await.unwrap(), state.data);
}
