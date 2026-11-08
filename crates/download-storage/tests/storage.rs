//! Persistence tests covering the task row, JSON segment checkpoints, metadata
//! tables and the statistics queries used by the API.

use chrono::Utc;
use download_core::{Category, DownloadSegment, DownloadStatus, DownloadTask};
use download_storage::{Storage, StoredRule, TaskFilter};
use serde_json::json;

fn task(id: &str, name: &str, status: DownloadStatus) -> DownloadTask {
    let now = Utc::now();
    DownloadTask {
        id: id.into(),
        url: format!("https://example.com/{name}"),
        final_url: None,
        file_name: name.into(),
        destination_dir: "/tmp/nexdownload".into(),
        file_path: format!("/tmp/nexdownload/{name}"),
        category: "documents".into(),
        queue_id: Some("default".into()),
        collection_id: None,
        status,
        priority: 0,
        total_bytes: Some(10),
        downloaded_bytes: 0,
        connections: 2,
        speed_bps: 0.0,
        error: None,
        error_kind: None,
        etag: None,
        last_modified: None,
        accepts_ranges: true,
        content_type: Some("application/octet-stream".into()),
        checksum_sha256: None,
        expected_checksum: None,
        source_domain: Some("example.com".into()),
        created_at: now,
        updated_at: now,
        started_at: None,
        completed_at: None,
        segments: vec![],
    }
}

#[tokio::test]
async fn seeds_builtins_and_reads_settings() {
    let storage = Storage::in_memory().await.unwrap();
    let categories = storage.categories().await.unwrap();
    assert_eq!(categories.len(), 7);
    assert!(categories.iter().all(|category| category.builtin));
    assert!(categories.iter().any(|category| category.id == "archives"));

    let queues = storage.queues().await.unwrap();
    assert_eq!(queues.len(), 1);
    assert_eq!(queues[0].id, "default");
    assert_eq!(queues[0].max_concurrent, 2);

    let settings = storage.settings().await.unwrap();
    assert_eq!(settings["max_concurrent_downloads"], json!(3));
    assert_eq!(settings["default_connections"], json!(4));

    storage
        .set_setting("max_concurrent_downloads", &json!(7))
        .await
        .unwrap();
    assert_eq!(
        storage.setting("max_concurrent_downloads").await.unwrap(),
        Some(json!(7))
    );
}

#[tokio::test]
async fn persists_task_segments_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("nexdownload.db");
    let mut original = task("t-roundtrip", "report.pdf", DownloadStatus::Paused);
    original.segments = vec![
        DownloadSegment {
            index: 0,
            start: 0,
            end: 4,
            downloaded: 4,
        },
        DownloadSegment {
            index: 1,
            start: 5,
            end: 9,
            downloaded: 1,
        },
    ];
    original.downloaded_bytes = 5;
    original.etag = Some("\"abc\"".into());
    original.started_at = Some(Utc::now());

    {
        let storage = Storage::open(&path).await.unwrap();
        storage.save_task(&original).await.unwrap();
        storage.close().await;
    }

    let storage = Storage::open(&path).await.unwrap();
    let loaded = storage.task("t-roundtrip").await.unwrap();
    assert_eq!(loaded.status, DownloadStatus::Paused);
    assert_eq!(loaded.file_name, "report.pdf");
    assert_eq!(loaded.segments.len(), 2);
    assert_eq!(loaded.segments[1].downloaded, 1);
    assert_eq!(loaded.etag.as_deref(), Some("\"abc\""));
    assert_eq!(loaded.downloaded_bytes, 5);
}

#[tokio::test]
async fn updates_progress_metadata_and_status() {
    let storage = Storage::in_memory().await.unwrap();
    storage
        .save_task(&task("t-progress", "video.mp4", DownloadStatus::Queued))
        .await
        .unwrap();

    let segments = vec![DownloadSegment {
        index: 0,
        start: 0,
        end: 9,
        downloaded: 6,
    }];
    storage
        .save_progress("t-progress", 6, Some(10), 512.0, &segments)
        .await
        .unwrap();
    storage
        .save_metadata(
            "t-progress",
            Some("https://cdn.example.com/final.mp4"),
            Some(10),
            Some("\"v2\""),
            Some("Wed, 21 Oct 2015 07:28:00 GMT"),
            true,
            Some("video/mp4"),
            Some("cdn.example.com"),
        )
        .await
        .unwrap();
    storage.set_started("t-progress", Utc::now()).await.unwrap();
    storage
        .set_status(
            "t-progress",
            DownloadStatus::Completed,
            None,
            None,
            Some(Utc::now()),
        )
        .await
        .unwrap();

    let loaded = storage.task("t-progress").await.unwrap();
    assert_eq!(loaded.status, DownloadStatus::Completed);
    assert_eq!(loaded.downloaded_bytes, 6);
    assert_eq!(loaded.speed_bps, 512.0);
    assert_eq!(loaded.segments[0].downloaded, 6);
    assert_eq!(
        loaded.final_url.as_deref(),
        Some("https://cdn.example.com/final.mp4")
    );
    assert_eq!(loaded.etag.as_deref(), Some("\"v2\""));
    assert_eq!(loaded.content_type.as_deref(), Some("video/mp4"));
    assert!(loaded.completed_at.is_some());
    assert!(loaded.started_at.is_some());
}

#[tokio::test]
async fn filters_tasks_by_status_search_category_and_limit() {
    let storage = Storage::in_memory().await.unwrap();
    let mut report = task("t1", "annual-report.pdf", DownloadStatus::Completed);
    report.category = "documents".into();
    let mut photo = task("t2", "holiday.png", DownloadStatus::Failed);
    photo.category = "images".into();
    let mut movie = task("t3", "trailer.mp4", DownloadStatus::Downloading);
    movie.category = "videos".into();
    movie.priority = 5;
    for item in [report, photo, movie] {
        storage.save_task(&item).await.unwrap();
    }

    let completed = storage
        .tasks(&TaskFilter::default().statuses(vec![DownloadStatus::Completed]))
        .await
        .unwrap();
    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].id, "t1");

    let search = storage
        .tasks(&TaskFilter::default().search("report"))
        .await
        .unwrap();
    assert_eq!(search.len(), 1);
    assert_eq!(search[0].id, "t1");

    let by_category = TaskFilter {
        category: Some("images".into()),
        ..TaskFilter::default()
    };
    assert_eq!(storage.tasks(&by_category).await.unwrap()[0].id, "t2");

    let limited = storage
        .tasks(&TaskFilter::default().with_limit(1))
        .await
        .unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].status, DownloadStatus::Downloading);
    assert_eq!(
        storage.count_tasks(&TaskFilter::default()).await.unwrap(),
        3
    );
}

#[tokio::test]
async fn records_events_integrity_and_speed_samples() {
    let storage = Storage::in_memory().await.unwrap();
    storage
        .save_task(&task("t-audit", "bundle.zip", DownloadStatus::Completed))
        .await
        .unwrap();

    storage
        .record_event("t-audit", None, DownloadStatus::Queued, Some("created"))
        .await
        .unwrap();
    storage
        .record_event(
            "t-audit",
            Some(DownloadStatus::Queued),
            DownloadStatus::Completed,
            None,
        )
        .await
        .unwrap();
    let events = storage.events("t-audit", 10).await.unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].to_state, "completed");

    storage
        .record_integrity(
            "t-audit",
            "sha256",
            Some("abc"),
            Some("abc"),
            "verified",
            Some(3),
            Some(3),
        )
        .await
        .unwrap();
    let checks = storage.integrity_checks("t-audit").await.unwrap();
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].status, "verified");

    storage
        .record_speed_sample("t-audit", 1024.0, 2048)
        .await
        .unwrap();
    let samples = storage.speed_samples(24, 100).await.unwrap();
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].speed_bps, 1024.0);
}

#[tokio::test]
async fn detects_duplicate_checksums() {
    let storage = Storage::in_memory().await.unwrap();
    storage
        .save_task(&task("t-one", "one.zip", DownloadStatus::Completed))
        .await
        .unwrap();
    storage
        .save_task(&task("t-two", "two.zip", DownloadStatus::Completed))
        .await
        .unwrap();
    storage.set_checksum("t-one", "deadbeef").await.unwrap();
    storage.set_checksum("t-two", "deadbeef").await.unwrap();

    let groups = storage.find_duplicate_groups().await.unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].len(), 2);
    assert!(storage
        .find_by_checksum("deadbeef")
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn organizer_crud_and_task_cascade() {
    let storage = Storage::in_memory().await.unwrap();
    storage
        .save_task(&task("t-organize", "asset.blend", DownloadStatus::Queued))
        .await
        .unwrap();

    storage
        .set_organization(
            "t-organize",
            Some("images"),
            Some("default"),
            None,
            Some(9),
            Some(8),
        )
        .await
        .unwrap();
    let organized = storage.task("t-organize").await.unwrap();
    assert_eq!(organized.category, "images");
    assert_eq!(organized.priority, 9);
    assert_eq!(organized.connections, 8);

    let queue = storage.create_queue("Night", 3).await.unwrap();
    assert_eq!(queue.max_concurrent, 3);
    let mut changed = queue.clone();
    changed.max_concurrent = 6;
    changed.status = "paused".into();
    storage.update_queue(&changed).await.unwrap();
    let paused = storage
        .queues()
        .await
        .unwrap()
        .into_iter()
        .find(|item| item.id == queue.id)
        .unwrap();
    assert_eq!(paused.max_concurrent, 6);
    assert_eq!(paused.status, "paused");
    assert!(storage.delete_queue(&queue.id).await.unwrap());

    let collection = storage
        .create_collection("Project", Some("Build assets"), Some("#38BDF8"))
        .await
        .unwrap();
    assert_eq!(storage.collections().await.unwrap().len(), 1);
    assert!(storage.delete_collection(&collection.id).await.unwrap());

    storage
        .upsert_category(&Category {
            id: "design".into(),
            name: "Design".into(),
            extensions: vec!["blend".into(), "psd".into()],
            destination_dir: Some("/tmp/design".into()),
            builtin: false,
        })
        .await
        .unwrap();
    assert_eq!(
        storage
            .category("design")
            .await
            .unwrap()
            .unwrap()
            .extensions,
        vec!["blend".to_string(), "psd".to_string()]
    );
    assert!(storage.delete_category("design").await.unwrap());
    assert!(storage.category("design").await.unwrap().is_none());

    let now = Utc::now();
    let rule = StoredRule {
        id: "rule-1".into(),
        name: "Blend files".into(),
        enabled: true,
        priority: 5,
        conditions: json!({"extensions": ["blend"]}),
        actions: json!({"category": "design"}),
        created_at: now,
    };
    storage.upsert_rule(&rule).await.unwrap();
    assert_eq!(storage.rules().await.unwrap()[0].name, "Blend files");
    assert!(storage.delete_rule("rule-1").await.unwrap());

    assert!(storage.remove_task("t-organize").await.unwrap());
    assert!(storage.find_task("t-organize").await.unwrap().is_none());
}

#[tokio::test]
async fn statistics_handle_empty_and_populated_databases() {
    let storage = Storage::in_memory().await.unwrap();

    let empty = storage.stats().await.unwrap();
    assert_eq!(empty.total_tasks, 0);
    assert_eq!(empty.current_speed_bps, 0.0);
    assert_eq!(empty.peak_speed_bps, 0.0);
    assert_eq!(empty.average_speed_bps, 0.0);
    assert_eq!(empty.average_duration_secs, 0.0);

    let empty_analytics = storage.analytics().await.unwrap();
    assert_eq!(empty_analytics.stats.total_tasks, 0);
    assert!(empty_analytics.speed_history.is_empty());

    let mut completed = task("t-completed", "report.pdf", DownloadStatus::Completed);
    completed.total_bytes = Some(100);
    completed.downloaded_bytes = 100;
    completed.started_at = Some(Utc::now() - chrono::Duration::seconds(2));
    completed.completed_at = Some(Utc::now());
    storage.save_task(&completed).await.unwrap();

    let populated = storage.stats().await.unwrap();
    assert_eq!(populated.total_tasks, 1);
    assert_eq!(populated.completed, 1);
    assert_eq!(populated.total_downloaded_bytes, 100);
    assert!(populated.average_speed_bps >= 0.0);

    let analytics = storage.analytics().await.unwrap();
    assert_eq!(analytics.stats.completed, 1);
    assert_eq!(analytics.daily.len(), 1);
    assert_eq!(analytics.daily[0].completed, 1);
    assert_eq!(analytics.daily[0].bytes, 100);
}
