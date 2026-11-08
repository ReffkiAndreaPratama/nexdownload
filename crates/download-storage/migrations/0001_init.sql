-- NexDownload initial schema.
-- Times are stored as RFC 3339 UTC strings for portability and readable backups.

CREATE TABLE IF NOT EXISTS categories (
    id                TEXT PRIMARY KEY,
    name              TEXT NOT NULL,
    extensions_json   TEXT NOT NULL DEFAULT '[]',
    destination_dir   TEXT,
    builtin           INTEGER NOT NULL DEFAULT 0,
    created_at        TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS queues (
    id             TEXT PRIMARY KEY,
    name           TEXT NOT NULL,
    priority       INTEGER NOT NULL DEFAULT 0,
    max_concurrent INTEGER NOT NULL DEFAULT 2,
    status         TEXT NOT NULL DEFAULT 'active',
    created_at     TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS collections (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    color       TEXT,
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tasks (
    id                TEXT PRIMARY KEY,
    url               TEXT NOT NULL,
    final_url         TEXT,
    file_name         TEXT NOT NULL,
    destination_dir   TEXT NOT NULL,
    file_path         TEXT NOT NULL,
    category          TEXT NOT NULL DEFAULT 'other',
    queue_id          TEXT REFERENCES queues(id) ON DELETE SET NULL,
    collection_id     TEXT REFERENCES collections(id) ON DELETE SET NULL,
    status            TEXT NOT NULL,
    priority          INTEGER NOT NULL DEFAULT 0,
    total_bytes       INTEGER,
    downloaded_bytes  INTEGER NOT NULL DEFAULT 0,
    connections       INTEGER NOT NULL DEFAULT 4,
    speed_bps         REAL NOT NULL DEFAULT 0,
    error             TEXT,
    error_kind        TEXT,
    etag              TEXT,
    last_modified     TEXT,
    accepts_ranges    INTEGER NOT NULL DEFAULT 0,
    content_type      TEXT,
    checksum_sha256   TEXT,
    expected_checksum TEXT,
    source_domain     TEXT,
    segments_json     TEXT NOT NULL DEFAULT '[]',
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    started_at        TEXT,
    completed_at      TEXT
);

CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
CREATE INDEX IF NOT EXISTS idx_tasks_category ON tasks(category);
CREATE INDEX IF NOT EXISTS idx_tasks_created ON tasks(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_tasks_queue ON tasks(queue_id);
CREATE INDEX IF NOT EXISTS idx_tasks_collection ON tasks(collection_id);
CREATE INDEX IF NOT EXISTS idx_tasks_checksum ON tasks(checksum_sha256);
CREATE INDEX IF NOT EXISTS idx_tasks_downloaded ON tasks(downloaded_bytes DESC);

CREATE TABLE IF NOT EXISTS task_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id    TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    from_state TEXT,
    to_state   TEXT NOT NULL,
    note       TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_task_events_task ON task_events(task_id, created_at DESC);

CREATE TABLE IF NOT EXISTS integrity_checks (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id      TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    algorithm    TEXT NOT NULL,
    expected     TEXT,
    actual       TEXT,
    status       TEXT NOT NULL,
    expected_size INTEGER,
    actual_size  INTEGER,
    checked_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_integrity_task ON integrity_checks(task_id, checked_at DESC);

CREATE TABLE IF NOT EXISTS download_rules (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,
    priority        INTEGER NOT NULL DEFAULT 0,
    conditions_json TEXT NOT NULL,
    actions_json    TEXT NOT NULL,
    created_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key        TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS speed_samples (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id          TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    sampled_at       TEXT NOT NULL,
    speed_bps        REAL NOT NULL,
    downloaded_bytes INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_speed_samples_time ON speed_samples(sampled_at DESC);
CREATE INDEX IF NOT EXISTS idx_speed_samples_task ON speed_samples(task_id, sampled_at DESC);

CREATE TABLE IF NOT EXISTS api_audit (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    method     TEXT NOT NULL,
    path       TEXT NOT NULL,
    status     INTEGER NOT NULL,
    remote     TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_api_audit_time ON api_audit(created_at DESC);
